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
    f.fired = wait_word(1, PARKING);
    f.wake_at = -1;
    return f;
}

static const char *harness_queued(const Fiber *f) {
    uint32_t head = worker->ring_head;
    bool first = worker->ring_tail != head && worker->ring[head % RUNQ_SIZE] == f;
    return first ? "queued" : "not queued";
}

int main(int argc, char **argv) {
    if (argc < 2) {
        return 2;
    }
    lugha_rt_init(argc, argv);
    const char *c = argv[1];
    Fiber f = parked();
    if (strcmp(c, "woken_while_parking") == 0) {
        wake(&f, 7);
        printf("%s %ld\n", harness_queued(&f), (long)wait_state(&f));
        worker->parking = &f;
        park_finish();
        printf("%s %ld\n", harness_queued(&f), (long)wait_state(&f));
    } else if (strcmp(c, "woken_once_parked") == 0) {
        worker->parking = &f;
        park_finish();
        printf("%s %d\n", harness_queued(&f), wait_state(&f) == WAITING);
        wake(&f, 3);
        printf("%s %ld\n", harness_queued(&f), (long)wait_state(&f));
    } else if (strcmp(c, "woken_twice") == 0) {
        worker->parking = &f;
        park_finish();
        wake(&f, 1);
        worker->ring_head = worker->ring_tail;
        wake(&f, 2);
        printf("%s %ld\n", harness_queued(&f), (long)wait_state(&f));
    } else if (strcmp(c, "stale") == 0) {
        // Woken otherwise, then waiting again: a timer or socket event
        // armed for the first wait misses the second.
        worker->parking = &f;
        park_finish();
        f.fired = wait_word(2, WAITING);
        // One call per statement: C picks the order of a call's arguments.
        bool first = wake_gen(&f, 1, TIMED_OUT);
        printf("%d %ld", first, (long)wait_state(&f));
        bool second = wake_gen(&f, 2, TIMED_OUT);
        printf(" %d %ld\n", second, (long)wait_state(&f));
    } else if (strcmp(c, "early") == 0) {
        // Wait 1 ends; a timer armed for wait 2 fires before `block` has
        // published it, so it can't claim it: it leaves a note instead.
        worker->parking = &f;
        park_finish();
        wake_gen(&f, 1, 3);
        worker->ring_head = worker->ring_tail;
        bool fired = wake_gen(&f, 2, TIMED_OUT);
        printf("%d %ld", fired, (long)wait_state(&f));
        // `block` publishes wait 2 and finds the note: it ends at once.
        f.fired = wait_word(2, PARKING);
        bool ended = early_wake(&f, 2);
        printf(" %d %ld", ended, (long)wait_state(&f));
        // A note for wait 2 never ends wait 3.
        f.fired = wait_word(3, PARKING);
        bool later = early_wake(&f, 3);
        printf(" %d %d\n", later, is_waiting(&f));
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
        std::fs::write(
            dir.join("lugha_rt.c"),
            lugha::link::runtime_source(lugha::link::System::host()).as_bytes(),
        )
        .expect("writable");
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
fn a_timer_firing_before_its_wait_is_published_still_ends_it() {
    // Before, the wake-up was lost: the task parked with no timer left,
    // and once all else was idle the program died of a false deadlock.
    assert_eq!(run("early"), "0 3 1 -2 0 1\n");
}

#[test]
fn parking_and_parked_both_count_as_waiting() {
    assert_eq!(run("waiting"), "1 1 0\n");
}

#[test]
fn a_wake_up_for_an_earlier_wait_misses_the_next() {
    assert_eq!(run("stale"), "0 -1 1 -2\n");
}
