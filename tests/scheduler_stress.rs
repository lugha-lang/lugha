//! The scheduler under stress on four workers, after Go's own tests of its
//! run queues: through a C harness that includes `lugha_rt.c`, built plain
//! and with ThreadSanitizer. No task may be lost or run twice, a ping-pong
//! may not starve beside a busy task, and a task made ready as every worker
//! parks still runs.

// Unix only: the harness drives the runtime with POSIX threads, built by the system `cc`; Windows' runtime is tested through whole programs.
#![cfg(unix)]

#[path = "common/c_harness.rs"]
mod c_harness;

use std::process::Command;

/// The harness: each case is one argument; it prints what it saw.
const HARNESS: &str = r#"#include "lugha_rt.c"

enum { MANY = 10000, PAIRS = 8, TRIPS = 2000 };

static int runs[MANY];
static void *chans[2 * PAIRS + 2];
static int hog_stop;


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

enum { SENT = 4000 };
static void *pair[2];
static int seen[2 * SENT];
static int64_t received;

/* Sends its half of the values, unbuffered, on its own channel. */
static void feeder(void *code, void *env, void *result) {
    int64_t side = (intptr_t)code;
    for (int64_t v = side * SENT; v < (side + 1) * SENT; v++) {
        lugha_rt_chan_send(pair[side], &v, "h", 1, 1);
    }
    *(int64_t *)result = 0;
    (void)env;
}

/* Selects on both channels with a short timeout until every value came. */
static void selector(void *code, void *env, void *result) {
    int64_t a = 0, b = 0;
    while (__atomic_load_n(&received, __ATOMIC_ACQUIRE) < 2 * SENT) {
        LughaCase cases[2] = {{pair[0], 0, &a, 0}, {pair[1], 0, &b, 0}};
        int64_t fired = lugha_rt_task_select(cases, 2, 1, 0, "h", 1, 1);
        if (fired >= 0 && cases[fired].ok) {
            int64_t v = fired == 0 ? a : b;
            __atomic_fetch_add(&seen[v], 1, __ATOMIC_RELAXED);
            __atomic_fetch_add(&received, 1, __ATOMIC_ACQ_REL);
        }
    }
    *(int64_t *)result = 0;
    (void)code;
    (void)env;
}

enum { PINNED = 8, PINNED_TURNS = 300 };
static int64_t off_worker;
static void *feeds[PINNED];

/* Pins itself, then waits on its feed, yields and sleeps: woken each time
 * from other workers, it must always run on its own. */
static void pinned_task(void *code, void *env, void *result) {
    int64_t me = (intptr_t)code, v = 0;
    lugha_rt_pin_here();
    Worker *home = worker;
    for (int i = 0; i < PINNED_TURNS; i++) {
        lugha_rt_chan_recv(feeds[me], &v);
        if (worker != home) {
            __atomic_fetch_add(&off_worker, 1, __ATOMIC_RELAXED);
        }
        lugha_rt_task_yield();
        if (i % 50 == 0) {
            lugha_rt_task_sleep(1);
        }
        if (worker != home) {
            __atomic_fetch_add(&off_worker, 1, __ATOMIC_RELAXED);
        }
    }
    *(int64_t *)result = 0;
    (void)env;
}

/* Feeds every pinned task, unpinned, from whichever worker runs it. */
static void feeder_all(void *code, void *env, void *result) {
    for (int64_t i = 0; i < PINNED_TURNS; i++) {
        for (int p = 0; p < PINNED; p++) {
            lugha_rt_chan_send(feeds[p], &i, "h", 1, 1);
        }
        lugha_rt_task_yield();
    }
    *(int64_t *)result = 0;
    (void)code;
    (void)env;
}

enum { RING_SENT = 100000 };
static void *the_ring;

/* A thread of C's own: every value into the ring, retrying when full. */
static void *ring_writer(void *unused) {
    (void)unused;
    for (int64_t v = 0; v < RING_SENT; v++) {
        while (lugha_rt_ring_try_send(the_ring, &v) == 1) {
        }
    }
    return NULL;
}

/* Reads every value sent: how many came, in order or not. */
static void ring_reader(void *code, void *env, void *result) {
    int64_t v = 0, got = 0, last = -1, ordered = 1;
    while (got < RING_SENT && lugha_rt_chan_recv(the_ring, &v)) {
        ordered &= v > last;
        last = v;
        got++;
    }
    *(int64_t *)result = got * 2 + ordered;
    (void)code;
    (void)env;
}

static void *targets[1000];

/* A task joined twice: gives its index. */
static void target(void *code, void *env, void *result) {
    *(int64_t *)result = (intptr_t)code;
    (void)env;
}

/* Joins its target, from another task. */
static void joiner(void *code, void *env, void *result) {
    *(int64_t *)result = join(targets[(intptr_t)code]);
    (void)env;
}

/* Past its cancel point, the task is cancelled, as by another worker in
 * that instant; then it queues on a channel and parks as `recv` does. 1 if
 * the park ended cancelled instead of waiting for nothing. */
static void cancelled_late(void *code, void *env, void *result) {
    LughaChan *c = lugha_rt_chan_new(0, 8, "h", 1, 1);
    lugha_rt_cancel_point();
    cancel_fiber(current, REASON_REQUESTED, NULL, NULL);
    lock_take(&c->lock);
    int64_t v = 0;
    int32_t ok = 0;
    Waiter w = {.fiber = current, .gen = next_wait_gen(current), .data = &v, .ok = &ok,
                .index = 0};
    enqueue(&c->recvq, &w);
    current->cancellable = true;
    uint32_t *locks[1] = {&c->lock};
    int64_t fired = block_holding(locks, 1);
    current->cancellable = false;
    lock_take(&c->lock);
    unqueue(&c->recvq, current);
    lock_give(&c->lock);
    *(int64_t *)result = fired == CANCELLED;
    (void)code;
    (void)env;
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
    } else if (strcmp(c, "select") == 0) {
        pair[0] = lugha_rt_chan_new(0, 8, "h", 1, 1);
        pair[1] = lugha_rt_chan_new(0, 8, "h", 1, 1);
        void *tasks[6];
        for (int i = 0; i < 4; i++) {
            tasks[i] = go(selector, NULL);
        }
        tasks[4] = go(feeder, (void *)(intptr_t)0);
        tasks[5] = go(feeder, (void *)(intptr_t)1);
        for (int i = 0; i < 6; i++) {
            join(tasks[i]);
        }
        int once = 0;
        for (int i = 0; i < 2 * SENT; i++) {
            once += seen[i] == 1;
        }
        printf("%d received once\n", once);
    } else if (strcmp(c, "joins") == 0) {
        for (int i = 0; i < 1000; i++) {
            targets[i] = go(target, (void *)(intptr_t)i);
        }
        void **joiners = malloc(sizeof(void *) * 2000);
        for (int i = 0; i < 2000; i++) {
            joiners[i] = go(joiner, (void *)(intptr_t)(i / 2));
        }
        int64_t sum = 0;
        for (int i = 0; i < 2000; i++) {
            sum += join(joiners[i]);
        }
        free(joiners);
        printf("%ld\n", (long)sum);
    } else if (strcmp(c, "pinned") == 0) {
        void *tasks[PINNED + 3];
        for (int p = 0; p < PINNED; p++) {
            feeds[p] = lugha_rt_chan_new(1, 8, "h", 1, 1);
            tasks[p] = go(pinned_task, (void *)(intptr_t)p);
        }
        tasks[PINNED] = go(feeder_all, NULL);
        tasks[PINNED + 1] = go(hog, NULL);
        tasks[PINNED + 2] = go(hog, NULL);
        for (int i = 0; i <= PINNED; i++) {
            join(tasks[i]);
        }
        __atomic_store_n(&hog_stop, 1, __ATOMIC_RELEASE);
        join(tasks[PINNED + 1]);
        join(tasks[PINNED + 2]);
        printf("%ld off their worker\n", (long)off_worker);
    } else if (strcmp(c, "ring") == 0) {
        the_ring = lugha_rt_ring_new(64, 8, "h", 1, 1);
        lugha_rt_rc_mark(the_ring);
        void *reader = go(ring_reader, NULL);
        void *busy = go(hog, NULL);
        pthread_t writer;
        pthread_create(&writer, NULL, ring_writer, NULL);
        // Joined as a task first: this worker mustn't block in C meanwhile.
        int64_t r = join(reader);
        pthread_join(writer, NULL);
        lugha_rt_chan_close(the_ring, "h", 1, 1);
        __atomic_store_n(&hog_stop, 1, __ATOMIC_RELEASE);
        join(busy);
        printf("%ld received, in order %ld\n", (long)(r / 2), (long)(r % 2));
    } else if (strcmp(c, "cancel") == 0) {
        int64_t sum = 0;
        for (int i = 0; i < 100; i++) {
            sum += join(go(cancelled_late, NULL));
        }
        printf("%ld cancelled\n", (long)sum);
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

/// Runs `case` on four workers, plain and under ThreadSanitizer.
fn run(case: &str) -> String {
    c_harness::run("stress", HARNESS, case, "4")
}

/// Pinned tasks woken from other workers by a channel, a timer and their
/// own yields always run on the worker they pinned themselves to, while
/// busy unpinned tasks beside them keep the other workers stealing.
#[test]
fn pinned_tasks_woken_from_any_worker_run_only_on_their_own() {
    assert_eq!(run("pinned"), "0 off their worker\n");
}

/// A ring written by a thread of C's own, retrying when full, and read by
/// a task on four workers beside a busy one: every value once, in order.
#[test]
fn a_ring_written_from_cs_thread_delivers_every_value_in_order() {
    assert_eq!(run("ring"), "100000 received, in order 1\n");
}

/// A cancel landing after a task's cancel point but before it parks still
/// ends its wait, every time.
#[test]
fn a_cancel_just_before_a_park_is_never_missed() {
    assert_eq!(run("cancel"), "100 cancelled\n");
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

#[test]
fn a_select_racing_its_cases_and_a_timeout_loses_nothing() {
    assert_eq!(run("select"), "8000 received once\n");
}

#[test]
fn a_thousand_tasks_each_joined_by_two_others() {
    assert_eq!(run("joins"), format!("{}\n", 2 * (999 * 1000 / 2)));
}

/// Four actors, each called by eight tasks at once, 200 calls each: every
/// call answered, once, in order per caller. A Lugha program, since an
/// actor's handlers are generated code.
#[test]
fn actors_called_from_many_workers_answer_every_call() {
    let dir = std::env::temp_dir().join(format!("lugha-stress-actors-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir is writable");
    std::fs::write(
        dir.join("main.la"),
        "actor Counter {\n\
             n: i64,\n\
             fun add(mut self, by: i64): i64 { self.n += by; self.n }\n\
         }\n\
         fun caller(c: Counter, out: Sender<i64>) {\n\
             let mut last = 0;\n\
             for _ in 0..200 {\n\
                 let now = c.add(1);\n\
                 if now <= last { out.send(-1); }\n\
                 last = now;\n\
             }\n\
             out.send(1);\n\
         }\n\
         fun main(console: Console, root: Scope) {\n\
             let (tx, rx) = Chan<i64>.new(32);\n\
             let counters = [root.spawn(Counter { n: 0 }), root.spawn(Counter { n: 0 }),\n\
                             root.spawn(Counter { n: 0 }), root.spawn(Counter { n: 0 })];\n\
             let r = scope s {\n\
                 for c of counters { for _ in 0..8 { s.spawn(caller(c, tx)); } }\n\
             };\n\
             r.unwrap();\n\
             tx.close();\n\
             let mut good = 0;\n\
             for v of rx { good += v; }\n\
             let mut total = 0;\n\
             for c of counters { total += c.add(0); }\n\
             console.println(\"{good} {total}\");\n\
         }\n",
    )
    .expect("writable");
    let built = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["build", "main.la", "-O2", "-o", "main"])
        .current_dir(&dir)
        .output()
        .expect("lughac runs");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    for _ in 0..10 {
        let out = Command::new(dir.join("main"))
            .env("LUGHA_WORKERS", "4")
            .output()
            .expect("it runs");
        assert_eq!(
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).into_owned()
            ),
            (Some(0), "32 6400\n".to_string()),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
