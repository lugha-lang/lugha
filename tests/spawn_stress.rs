//! Spawning under stress: many tasks started and finished in scopes from
//! several workers at once, each scope counting under its own lock, and
//! the per-worker cache of task stacks. Through a C harness that includes
//! `lugha_rt.c`, built plain and with ThreadSanitizer; a scope failing
//! while its owner spawns needs a panic to unwind, so that one is a Lugha
//! program.

#[path = "common/c_harness.rs"]
mod c_harness;

use std::process::Command;

/// The harness: each case is one argument; it prints what it saw.
const HARNESS: &str = r#"#include "lugha_rt.c"

enum { SPAWNERS = 8, EACH = 10000, LIMITED = 1000 };

static int64_t share_done[SPAWNERS];
static int64_t ended_early;
static int64_t running_now, running_most, limited_done;
static void *stacks_seen[2];

/* One task of a spawner's share: counts itself done, unless its scope has
 * already ended, which no task may outlive. */
static void one(void *code, void *env, void *result) {
    int64_t spawner = (intptr_t)code;
    __atomic_fetch_add(&share_done[spawner], 1, __ATOMIC_RELAXED);
    *(int64_t *)result = 0;
    (void)env;
}

static void *in(LughaScope *s, void (*entry)(void *, void *, void *), void *code) {
    LughaTask *t = lugha_rt_task_spawn_in(s, entry, code, lugha_rt_alloc(8), 8);
    lugha_rt_task_go(t);
    return t;
}

static void enter(LughaScope *s, int64_t limit) {
    lugha_rt_scope_enter(s, NULL, 0, NULL, NULL, limit, -1, -1);
}

static int32_t leave(LughaScope *s) {
    void *table, *box;
    LughaString *panic, *task;
    lugha_rt_scope_end(s);
    int32_t failed = lugha_rt_scope_exit(s, &table, &box, &panic, &task);
    lugha_rt_rc_release(box);
    lugha_rt_rc_release(panic);
    lugha_rt_rc_release(task);
    return failed;
}

/* Spawns its share into a scope of its own, then ends the scope: by then
 * every task of the share is done. */
static void spawner(void *code, void *env, void *result) {
    int64_t me = (intptr_t)code;
    LughaScope s;
    enter(&s, -1);
    for (int i = 0; i < EACH; i++) {
        lugha_rt_rc_release(in(&s, one, code));
    }
    leave(&s);
    if (__atomic_load_n(&share_done[me], __ATOMIC_ACQUIRE) != EACH) {
        __atomic_fetch_add(&ended_early, 1, __ATOMIC_RELAXED);
    }
    *(int64_t *)result = 0;
    (void)env;
}

/* Under `limit 2`: counts itself running, lets others run, counts out. */
static void limited(void *code, void *env, void *result) {
    int64_t now = __atomic_add_fetch(&running_now, 1, __ATOMIC_ACQ_REL);
    int64_t most = __atomic_load_n(&running_most, __ATOMIC_ACQUIRE);
    while (now > most &&
           !__atomic_compare_exchange_n(&running_most, &most, now, false, __ATOMIC_ACQ_REL,
                                        __ATOMIC_ACQUIRE)) {
    }
    for (int i = 0; i < 3; i++) {
        lugha_rt_task_yield();
    }
    __atomic_sub_fetch(&running_now, 1, __ATOMIC_ACQ_REL);
    __atomic_fetch_add(&limited_done, 1, __ATOMIC_RELAXED);
    *(int64_t *)result = 0;
    (void)code;
    (void)env;
}

/* Spawns into a `limit 2` scope, as the other workers finish its tasks. */
static void limiter(void *code, void *env, void *result) {
    LughaScope s;
    enter(&s, 2);
    for (int i = 0; i < LIMITED; i++) {
        lugha_rt_rc_release(in(&s, limited, NULL));
    }
    leave(&s);
    *(int64_t *)result = 0;
    (void)code;
    (void)env;
}

/* Notes the stack it runs on. */
static void note_stack(void *code, void *env, void *result) {
    stacks_seen[(intptr_t)code] = current->map;
    *(int64_t *)result = 0;
    (void)env;
}

static void *go(void (*entry)(void *, void *, void *), void *code) {
    return lugha_rt_task_spawn(entry, code, lugha_rt_alloc(8), 8);
}

int main(int argc, char **argv) {
    lugha_rt_init(argc, argv);
    // As lockdown does: from here on, workers may start.
    helpers_allowed = true;
    const char *c = argv[1];
    if (strcmp(c, "spawners") == 0) {
        void *tasks[SPAWNERS];
        for (int i = 0; i < SPAWNERS; i++) {
            tasks[i] = go(spawner, (void *)(intptr_t)i);
        }
        int64_t all = 0;
        for (int i = 0; i < SPAWNERS; i++) {
            lugha_rt_task_join(tasks[i]);
            all += share_done[i];
        }
        printf("%ld ran, %ld scopes ended early\n", (long)all, (long)ended_early);
    } else if (strcmp(c, "limit") == 0) {
        lugha_rt_task_join(go(limiter, NULL));
        printf("%ld ran, at most %ld at once\n", (long)limited_done, (long)running_most);
    } else if (strcmp(c, "cache") == 0) {
        lugha_rt_task_join(go(note_stack, (void *)0));
        int64_t before = __atomic_load_n(&stacks_given_back, __ATOMIC_ACQUIRE);
        lugha_rt_task_join(go(note_stack, (void *)1));
        int64_t after = __atomic_load_n(&stacks_given_back, __ATOMIC_ACQUIRE);
        printf("same stack: %d, given back between: %ld\n", stacks_seen[0] == stacks_seen[1],
               (long)(after - before));
        // Nothing to run for a while: the worker parks.
        lugha_rt_task_sleep(5);
        Worker *w = worker;
        int holding = 0;
        for (int i = 0; i < w->stack_cached; i++) {
            holding += w->stack_dirty[i];
        }
        printf("cached %d, holding pages after parking: %d\n", w->stack_cached, holding);
    }
    return 0;
}
"#;

/// Runs `case` on `workers` workers, plain and under ThreadSanitizer.
fn run(case: &str, workers: &str) -> String {
    c_harness::run("spawn-stress", HARNESS, case, workers)
}

#[test]
fn eight_spawners_in_scopes_of_their_own_each_end_after_all_their_tasks() {
    assert_eq!(run("spawners", "4"), "80000 ran, 0 scopes ended early\n");
}

#[test]
fn a_limit_of_two_holds_while_other_workers_finish_its_tasks() {
    assert_eq!(run("limit", "4"), "1000 ran, at most 2 at once\n");
}

#[test]
fn a_finished_tasks_stack_is_reused_without_a_system_call_and_trimmed_on_parking() {
    assert_eq!(
        run("cache", "1"),
        "same stack: 1, given back between: 0\ncached 1, holding pages after parking: 0\n"
    );
}

/// A scope failing while its owner goes on spawning into it, on four
/// workers: the owner is cancelled at its next spawn, every task started
/// is cancelled or finishes before the scope ends (a task left over would
/// send on the closed channel and panic), and the scope reports the panic.
#[test]
fn a_scope_failing_as_its_owner_spawns_outlives_none_of_its_tasks() {
    let dir = std::env::temp_dir().join(format!("lugha-spawn-fail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir is writable");
    std::fs::write(
        dir.join("main.la"),
        "import std.task;\n\
         fun work(out: Sender<i64>) {\n\
             task.yield_now();\n\
             out.send(1);\n\
         }\n\
         fun fail_soon() {\n\
             task.yield_now();\n\
             panic(\"boom\");\n\
         }\n\
         fun main(console: Console) {\n\
             let (tx, rx) = Chan<i64>.unbounded();\n\
             let r = scope s {\n\
                 for i in 0..5000 {\n\
                     if i == 100 { s.spawn(fail_soon()); }\n\
                     s.spawn(work(tx));\n\
                 }\n\
             };\n\
             tx.close();\n\
             let mut sent = 0;\n\
             for v of rx { sent += v; }\n\
             match r {\n\
                 .Ok(_) => console.println(\"finished\"),\n\
                 .Err(_) => console.println(\"failed, {sent <= 5000}\"),\n\
             }\n\
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
            (Some(0), "failed, true\n".to_string()),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    // Clean-up only: a leftover temp directory harms nothing.
    let _ = std::fs::remove_dir_all(&dir);
}
