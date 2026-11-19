//! The scheduler under stress on four workers, after Go's own tests of its
//! run queues: through a C harness that includes `lugha_rt.c`, built plain
//! and with ThreadSanitizer. No task may be lost or run twice, a ping-pong
//! may not starve beside a busy task, and a task made ready as every worker
//! parks still runs.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

/// The harness: each case is one argument; it prints what it saw.
const HARNESS: &str = r#"#include "lugha_rt.c"

enum { MANY = 10000, PAIRS = 8, TRIPS = 2000 };

static int runs[MANY];
static void *chans[2 * PAIRS + 2];
static int hog_stop;

typedef struct { int64_t from; int64_t to; } Span;

static void *go(void (*entry)(void *, void *, void *), void *code) {
    return lugha_rt_task_spawn(entry, code, lugha_rt_alloc(8), 8);
}

/* One task of the many: counts that it ran. */
static void one(void *code, void *env, void *result) {
    __atomic_fetch_add(&runs[(intptr_t)code], 1, __ATOMIC_RELAXED);
    *(int64_t *)result = 0;
    (void)env;
}

/* Spawns a tenth of the many, from a task, and joins them. */
static void spawner(void *code, void *env, void *result) {
    int64_t from = (intptr_t)code * (MANY / 10);
    void **tasks = malloc(sizeof(void *) * (MANY / 10));
    for (int64_t i = 0; i < MANY / 10; i++) {
        tasks[i] = go(one, (void *)(intptr_t)(from + i));
    }
    for (int64_t i = 0; i < MANY / 10; i++) {
        lugha_rt_task_join(tasks[i]);
    }
    free(tasks);
    *(int64_t *)result = 0;
    (void)env;
}

/* Pair `p`'s pong side: answers each ping with one more. */
static void pong(void *code, void *env, void *result) {
    int64_t p = (intptr_t)code, v = 0;
    for (int i = 0; i < TRIPS; i++) {
        lugha_rt_chan_recv(chans[2 * p], &v);
        v += 1;
        lugha_rt_chan_send(chans[2 * p + 1], &v, "h", 1, 1);
    }
    *(int64_t *)result = v;
    (void)env;
}

/* Pair `p`'s ping side. */
static void ping(void *code, void *env, void *result) {
    int64_t p = (intptr_t)code, v = 0;
    for (int i = 0; i < TRIPS; i++) {
        lugha_rt_chan_send(chans[2 * p], &v, "h", 1, 1);
        lugha_rt_chan_recv(chans[2 * p + 1], &v);
    }
    *(int64_t *)result = v;
    (void)env;
}

/* Busy until told to stop, preempted by its countdown as generated code is. */
static void hog(void *code, void *env, void *result) {
    int64_t *budget = (int64_t *)(((uintptr_t)__builtin_frame_address(0) | lugha_rt_stack_mask) - 15);
    while (!__atomic_load_n(&hog_stop, __ATOMIC_ACQUIRE)) {
        if (--*budget <= 0) {
            lugha_rt_task_preempt();
        }
    }
    *(int64_t *)result = 0;
    (void)code;
    (void)env;
}

/* Sleeps a millisecond, then sends: woken by a timer, waking `main`. */
static void late(void *code, void *env, void *result) {
    lugha_rt_task_sleep(1);
    int64_t v = (intptr_t)code;
    lugha_rt_chan_send(chans[2 * PAIRS], &v, "h", 1, 1);
    *(int64_t *)result = 0;
    (void)env;
}

static int64_t join(void *task) {
    return *(int64_t *)lugha_rt_task_join(task);
}

int main(int argc, char **argv) {
    lugha_rt_init(argc, argv);
    // As lockdown does: from here on, workers may start.
    helpers_allowed = true;
    const char *c = argv[1];
    for (int i = 0; i < 2 * PAIRS + 2; i++) {
        chans[i] = lugha_rt_chan_new(0, 8, "h", 1, 1);
    }
    if (strcmp(c, "many") == 0) {
        void *spawners[10];
        for (int i = 0; i < 10; i++) {
            spawners[i] = go(spawner, (void *)(intptr_t)i);
        }
        for (int i = 0; i < 10; i++) {
            join(spawners[i]);
        }
        int once = 0;
        for (int i = 0; i < MANY; i++) {
            once += runs[i] == 1;
        }
        printf("%d ran once\n", once);
    } else if (strcmp(c, "pairs") == 0) {
        void *tasks[2 * PAIRS];
        for (int p = 0; p < PAIRS; p++) {
            tasks[2 * p] = go(pong, (void *)(intptr_t)p);
            tasks[2 * p + 1] = go(ping, (void *)(intptr_t)p);
        }
        int64_t sum = 0;
        for (int i = 0; i < 2 * PAIRS; i++) {
            sum += join(tasks[i]);
        }
        printf("%ld\n", (long)sum);
    } else if (strcmp(c, "hog") == 0) {
        void *busy = go(hog, NULL);
        void *a = go(pong, (void *)(intptr_t)0), *b = go(ping, (void *)(intptr_t)0);
        int64_t got = join(a) + join(b);
        __atomic_store_n(&hog_stop, 1, __ATOMIC_RELEASE);
        join(busy);
        printf("%ld\n", (long)got);
    } else if (strcmp(c, "park") == 0) {
        int64_t sum = 0, v = 0;
        for (int i = 0; i < 300; i++) {
            void *t = go(late, (void *)(intptr_t)i);
            lugha_rt_chan_recv(chans[2 * PAIRS], &v);
            sum += v;
            join(t);
        }
        printf("%ld\n", (long)sum);
    }
    return 0;
}
"#;

/// Builds the harness once per flavour: plain, or with ThreadSanitizer.
fn harness(tsan: bool) -> &'static PathBuf {
    static PLAIN: OnceLock<PathBuf> = OnceLock::new();
    static TSAN: OnceLock<PathBuf> = OnceLock::new();
    let built = if tsan { &TSAN } else { &PLAIN };
    built.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!(
            "lugha-stress-{}-{}",
            std::process::id(),
            u8::from(tsan)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        std::fs::write(dir.join("lugha_rt.c"), lugha::link::RUNTIME_SOURCE).expect("writable");
        std::fs::write(dir.join("harness.c"), HARNESS).expect("writable");
        let exe = dir.join("harness");
        let mut cc = Command::new("cc");
        cc.args(["-std=gnu11", "-O1", "-pthread", "-funwind-tables", "-I"])
            .arg(&dir);
        if tsan {
            cc.args(["-fsanitize=thread", "-g"]);
        }
        let out = cc
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

/// Runs `case` on four workers, both builds, killing a hung run: what it
/// printed, the same for both.
fn run(case: &str) -> String {
    let [plain, tsan] = [false, true].map(|tsan| {
        let mut child = Command::new(harness(tsan))
            .arg(case)
            .env("LUGHA_WORKERS", "4")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("harness runs");
        let started = std::time::Instant::now();
        let limit = Duration::from_secs(if tsan { 120 } else { 30 });
        while child.try_wait().expect("waits").is_none() {
            if started.elapsed() > limit {
                let _ = child.kill();
                panic!("{case} hung (ThreadSanitizer: {tsan})");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output().expect("ends");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            out.status.success() && stderr.is_empty(),
            "{case} (ThreadSanitizer: {tsan}): {stderr}"
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    });
    assert_eq!(plain, tsan, "{case}");
    plain
}

#[test]
fn ten_thousand_tasks_from_ten_spawners_each_run_once() {
    assert_eq!(run("many"), "10000 ran once\n");
}

#[test]
fn eight_ping_pongs_finish_while_workers_steal() {
    assert_eq!(run("pairs"), format!("{}\n", 2 * 8 * 2000));
}

#[test]
fn a_ping_pong_beside_a_busy_task_isnt_starved() {
    assert_eq!(run("hog"), format!("{}\n", 2 * 2000));
}

#[test]
fn a_task_woken_as_every_worker_parks_still_runs() {
    assert_eq!(run("park"), format!("{}\n", 299 * 300 / 2));
}
