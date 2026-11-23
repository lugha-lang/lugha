//! The allocator across real threads, through a C harness that includes
//! `lugha_rt.c` itself: each thread is a worker that allocates blocks of
//! every size, hands them to the next thread and frees what it is handed,
//! so most frees are of another worker's blocks. Built plainly and with
//! ThreadSanitizer.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

/// The harness: each case is one argument; it prints what it saw.
const HARNESS: &str = r#"#include "lugha_rt.c"

enum { THREADS = 8, BATCH = 64, LARGEST = 400 };

/* Blocks handed to each thread, guarded by its mutex. */
typedef struct {
    pthread_mutex_t lock;
    unsigned char **items;
    int64_t len;
    int64_t cap;
} Queue;

static Queue queues[THREADS];
static pthread_barrier_t all_done;
/* The threads and `main`: past it, `main` reads the totals. */
static pthread_barrier_t harness_finished;
static int64_t rounds;
static int bad;

static void queue_push(Queue *q, unsigned char *item) {
    pthread_mutex_lock(&q->lock);
    if (q->len == q->cap) {
        q->cap = q->cap == 0 ? 256 : q->cap * 2;
        q->items = realloc(q->items, (size_t)q->cap * sizeof *q->items);
    }
    q->items[q->len++] = item;
    pthread_mutex_unlock(&q->lock);
}

/* Frees every block handed to `q`, checking each still holds the pattern
 * its maker wrote. */
static void queue_drain(Queue *q) {
    pthread_mutex_lock(&q->lock);
    for (int64_t i = 0; i < q->len; i++) {
        unsigned char *item = q->items[i];
        int64_t size = *(int64_t *)item;
        for (int64_t b = 8; b < size; b++) {
            if (item[b] != (unsigned char)(size + b)) {
                __atomic_store_n(&bad, 1, __ATOMIC_RELAXED);
            }
        }
        lugha_rt_rc_release(item);
    }
    q->len = 0;
    pthread_mutex_unlock(&q->lock);
}

static void *thread_main(void *arg) {
    int64_t me = (int64_t)(intptr_t)arg;
    lugha_rt_worker_enter();
    int64_t size = 8 + me;
    for (int64_t r = 0; r < rounds; r++) {
        for (int i = 0; i < BATCH; i++) {
            size = size % LARGEST + 8;
            unsigned char *item = lugha_rt_alloc(size);
            *(int64_t *)item = size;
            for (int64_t b = 8; b < size; b++) {
                item[b] = (unsigned char)(size + b);
            }
            queue_push(&queues[(me + 1) % THREADS], item);
            size += 3;
        }
        queue_drain(&queues[me]);
    }
    pthread_barrier_wait(&all_done);
    queue_drain(&queues[me]);
    // A worker never exits: its record lives in the thread's storage.
    pthread_barrier_wait(&harness_finished);
    for (;;) {
        pause();
    }
    return NULL;
}

int main(int argc, char **argv) {
    lugha_rt_init(argc, argv);
    rounds = argc > 1 ? atol(argv[1]) : 100;
    pthread_barrier_init(&all_done, NULL, THREADS);
    pthread_barrier_init(&harness_finished, NULL, THREADS + 1);
    pthread_t threads[THREADS];
    for (int i = 0; i < THREADS; i++) {
        pthread_mutex_init(&queues[i].lock, NULL);
    }
    for (int i = 0; i < THREADS; i++) {
        pthread_create(&threads[i], NULL, thread_main, (void *)(intptr_t)i);
    }
    pthread_barrier_wait(&harness_finished);
    int64_t made, freed;
    rc_totals(&made, &freed);
    printf("made %ld, live %ld, intact %d, chunks %ld\n", (long)made, (long)(made - freed),
           !bad, (long)((rc_region_next - rc_region) / RC_CHUNK));
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
            "lugha-alloc-{}-{}",
            std::process::id(),
            u8::from(tsan)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        std::fs::write(dir.join("lugha_rt.c"), lugha::link::RUNTIME_SOURCE).expect("writable");
        std::fs::write(dir.join("harness.c"), HARNESS).expect("writable");
        let exe = dir.join("harness");
        let mut cc = Command::new("cc");
        cc.args(["-std=gnu11", "-O2", "-pthread", "-I"]).arg(&dir);
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

/// Runs the harness for `rounds`: its report, after checking it exited 0
/// with nothing on stderr (where ThreadSanitizer reports races).
fn run(tsan: bool, rounds: u32) -> (u64, u64, bool, u64) {
    let out = Command::new(harness(tsan))
        .arg(rounds.to_string())
        .output()
        .expect("harness runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && stderr.is_empty(), "{stderr}");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let words: Vec<&str> = text
        .split([' ', ',', '\n'])
        .filter(|w| !w.is_empty())
        .collect();
    let number = |key: &str| -> u64 {
        let at = words.iter().position(|w| *w == key).expect(&text);
        words[at + 1].parse().expect(&text)
    };
    (
        number("made"),
        number("live"),
        number("intact") == 1,
        number("chunks"),
    )
}

#[test]
fn eight_threads_free_each_others_blocks() {
    let (made, live, intact, _) = run(false, 200);
    assert_eq!(made, 8 * 200 * 64);
    assert_eq!(live, 0);
    assert!(intact);
}

#[test]
fn blocks_freed_elsewhere_are_taken_back_before_carving() {
    // Measured: about 470 chunks here, 1,229 on a busy CI machine;
    // without taking blocks back, each round's would need fresh ones,
    // about 3,200 for these rounds.
    let (_, live, _, chunks) = run(false, 4000);
    assert_eq!(live, 0);
    assert!(chunks < 2000, "{chunks} chunks");
}

#[test]
fn threadsanitizer_finds_no_race() {
    let (made, live, intact, _) = run(true, 50);
    assert_eq!(made, 8 * 50 * 64);
    assert_eq!(live, 0);
    assert!(intact);
}
