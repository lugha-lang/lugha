//! The handoff between a task that waits and whoever wakes it, through a C
//! harness that includes `lugha_rt.c` itself, so it reaches the scheduler's
//! own functions. On one thread a waker never meets a task still parking;
//! these cases build that moment by hand.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

/// The harness: each case is one argument; it prints what it saw.
const HARNESS: &str = r#"#include "lugha_rt.c"

/* A fiber made by hand, parking: `block` has run, its switch hasn't. */
static Fiber parked(void) {
    Fiber f;
    memset(&f, 0, sizeof f);
    f.fired = PARKING;
    f.wake_at = -1;
    return f;
}

static const char *harness_queued(const Fiber *f) {
    return worker->ready_head == f ? "queued" : "not queued";
}

int main(int argc, char **argv) {
    if (argc < 2) {
        return 2;
    }
    const char *c = argv[1];
    Fiber f = parked();
    if (strcmp(c, "woken_while_parking") == 0) {
        wake(&f, 7);
        printf("%s %ld\n", harness_queued(&f), (long)f.fired);
        worker->parking = &f;
        park_finish();
        printf("%s %ld\n", harness_queued(&f), (long)f.fired);
    } else if (strcmp(c, "woken_once_parked") == 0) {
        worker->parking = &f;
        park_finish();
        printf("%s %d\n", harness_queued(&f), f.fired == WAITING);
        wake(&f, 3);
        printf("%s %ld\n", harness_queued(&f), (long)f.fired);
    } else if (strcmp(c, "woken_twice") == 0) {
        worker->parking = &f;
        park_finish();
        wake(&f, 1);
        worker->ready_head = worker->ready_tail = NULL;
        wake(&f, 2);
        printf("%s %ld\n", harness_queued(&f), (long)f.fired);
    } else if (strcmp(c, "waiting") == 0) {
        printf("%d", is_waiting(&f));
        worker->parking = &f;
        park_finish();
        printf(" %d", is_waiting(&f));
        wake(&f, 0);
        printf(" %d\n", is_waiting(&f));
    }
    return 0;
}
"#;

/// Builds the harness once for every test: with plain loads and stores, as
/// one worker runs, and with the atomic ones tasks crossing threads need.
fn harness(atomic: bool) -> &'static PathBuf {
    static PLAIN: OnceLock<PathBuf> = OnceLock::new();
    static ATOMIC: OnceLock<PathBuf> = OnceLock::new();
    let built = if atomic { &ATOMIC } else { &PLAIN };
    built.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!(
            "lugha-waiting-{}-{}",
            std::process::id(),
            u8::from(atomic)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        std::fs::write(dir.join("lugha_rt.c"), lugha::link::RUNTIME_SOURCE).expect("writable");
        std::fs::write(dir.join("harness.c"), HARNESS).expect("writable");
        let exe = dir.join("harness");
        let define = format!("-DLUGHA_TASKS_CROSS_THREADS={}", u8::from(atomic));
        let out = Command::new("cc")
            .args(["-std=c11", "-O2", &define, "-I"])
            .arg(&dir)
            .arg(dir.join("harness.c"))
            .args(["-lm", "-o"])
            .arg(&exe)
            .output()
            .expect("cc runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        exe
    })
}

/// Runs one case both ways; both must print the same.
fn run(case: &str) -> String {
    let [plain, atomic] = [false, true].map(|atomic| {
        let out = Command::new(harness(atomic))
            .arg(case)
            .output()
            .expect("harness runs");
        assert_eq!(out.status.code(), Some(0), "{case}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    });
    assert_eq!(plain, atomic, "{case}");
    plain
}

#[test]
fn a_task_woken_while_parking_is_queued_once_saved() {
    assert_eq!(run("woken_while_parking"), "not queued 7\nqueued 7\n");
}

#[test]
fn a_parked_task_is_queued_by_its_waker() {
    assert_eq!(run("woken_once_parked"), "not queued 1\nqueued 3\n");
}

#[test]
fn only_the_first_wake_counts() {
    assert_eq!(run("woken_twice"), "not queued 1\n");
}

#[test]
fn parking_and_parked_both_count_as_waiting() {
    assert_eq!(run("waiting"), "1 1 0\n");
}
