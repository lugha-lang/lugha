# Benchmark results

Tables appended by `python3 bench/run.py --save` (see README.md). Compare runs only on the same machine.

## 2026-10-31 21:23, commit 5d5ab18

- CPU: Intel(R) Core(TM) i5-4308U CPU @ 2.80GHz
- lugha: lughac 0.1.0
- c: gcc (Ubuntu 15.2.0-16ubuntu1) 15.2.0
- rust: rustc 1.99.0 (b940084d7 2026-09-28)
- go: go version go1.26.8 linux/amd64
- node: v22.22.1

Median wall time of 5 runs:

| benchmark   | lugha  | lugha-O0 | c      | rust   | go     | node   |
|-------------|--------|----------|--------|--------|--------|--------|
| binarytrees | 1.114s | 1.290s   | 0.519s | 1.371s | 1.076s | 0.542s |
| fannkuch    | 0.251s | 1.015s   | 0.216s | 0.216s | 0.226s | 0.458s |
| fib         | 0.562s | 1.074s   | 0.229s | 0.328s | 0.563s | 1.747s |
| mandelbrot  | 0.364s | 0.807s   | 0.361s | 0.406s | 0.378s | 0.542s |
| nbody       | 0.650s | 2.762s   | 0.533s | 0.605s | 0.685s | 0.977s |
| wordfreq    | 0.557s | 1.111s   | 0.096s | 0.152s | 0.349s | 1.106s |

Fastest run:

| benchmark   | lugha  | lugha-O0 | c      | rust   | go     | node   |
|-------------|--------|----------|--------|--------|--------|--------|
| binarytrees | 1.079s | 1.265s   | 0.513s | 1.339s | 0.881s | 0.530s |
| fannkuch    | 0.250s | 1.012s   | 0.215s | 0.215s | 0.223s | 0.453s |
| fib         | 0.558s | 1.074s   | 0.229s | 0.326s | 0.560s | 1.743s |
| mandelbrot  | 0.363s | 0.805s   | 0.361s | 0.404s | 0.372s | 0.538s |
| nbody       | 0.647s | 2.736s   | 0.530s | 0.600s | 0.681s | 0.968s |
| wordfreq    | 0.535s | 1.096s   | 0.095s | 0.151s | 0.334s | 1.101s |

Peak memory (MB, median):

| benchmark   | lugha | lugha-O0 | c    | rust | go    | node  |
|-------------|-------|----------|------|------|-------|-------|
| binarytrees | 70.3  | 70.2     | 9.8  | 18.1 | 11.9  | 108.8 |
| fannkuch    | 2.2   | 2.2      | 1.7  | 2.1  | 2.0   | 63.3  |
| fib         | 2.2   | 2.2      | 1.7  | 2.2  | 1.8   | 62.8  |
| mandelbrot  | 2.2   | 2.2      | 1.7  | 2.2  | 1.9   | 65.3  |
| nbody       | 2.2   | 2.2      | 1.7  | 2.1  | 2.0   | 66.0  |
| wordfreq    | 83.4  | 100.4    | 16.2 | 47.2 | 103.2 | 234.2 |

## 2026-10-31, commit 9d1e7f6, with Python

- CPU: Intel(R) Core(TM) i5-4308U CPU @ 2.80GHz
- lugha: lughac 0.1.0 (with `loop` and labels, milestone 152)
- c: gcc (Ubuntu 15.2.0-16ubuntu1) 15.2.0
- rust: rustc 1.99.0 (b940084d7 2026-09-28)
- go: go version go1.26.8 linux/amd64
- node: v22.22.1
- python: Python 3.14.4

Assembled from two runs of 3: `binarytrees`, `fannkuch`, `fib` and `mandelbrot` from a run of every language (`--lang lugha,lugha-O0,c,rust,go,node,python --runs 3 --timeout 300`), `nbody` and `wordfreq` from a rerun of those two (`--lang lugha,c,rust,go,node,python`). The first run's `nbody` and `wordfreq` came after many minutes of full load and every language was 1.5–4× slower in them (the CPU throttling), so they were measured again on a cool machine; the rerun had no `-O0` column (`—`).

Median wall time of 3 runs:

| benchmark   | lugha  | lugha-O0 | c      | rust   | go     | node   | python  |
|-------------|--------|----------|--------|--------|--------|--------|---------|
| binarytrees | 1.136s | 1.362s   | 0.559s | 1.496s | 0.857s | 0.595s | 4.818s  |
| fannkuch    | 0.272s | 1.204s   | 0.231s | 0.227s | 0.270s | 0.555s | 8.996s  |
| fib         | 0.674s | 1.217s   | 0.251s | 0.360s | 0.596s | 2.012s | 22.291s |
| mandelbrot  | 0.407s | 1.145s   | 0.367s | 0.439s | 0.411s | 0.669s | 27.672s |
| nbody       | 0.648s | —        | 0.535s | 0.603s | 0.682s | 1.012s | 28.382s |
| wordfreq    | 0.523s | —        | 0.095s | 0.152s | 0.353s | 1.138s | 1.012s  |

Fastest run:

| benchmark   | lugha  | lugha-O0 | c      | rust   | go     | node   | python  |
|-------------|--------|----------|--------|--------|--------|--------|---------|
| binarytrees | 1.103s | 1.288s   | 0.540s | 1.482s | 0.852s | 0.558s | 4.632s  |
| fannkuch    | 0.260s | 1.101s   | 0.225s | 0.227s | 0.268s | 0.532s | 8.852s  |
| fib         | 0.628s | 1.210s   | 0.246s | 0.359s | 0.586s | 1.970s | 22.172s |
| mandelbrot  | 0.391s | 0.888s   | 0.364s | 0.439s | 0.382s | 0.643s | 20.402s |
| nbody       | 0.648s | —        | 0.534s | 0.603s | 0.680s | 0.975s | 28.207s |
| wordfreq    | 0.518s | —        | 0.095s | 0.151s | 0.349s | 1.101s | 1.005s  |

Peak memory (MB, median):

| benchmark   | lugha | lugha-O0 | c    | rust | go    | node  | python |
|-------------|-------|----------|------|------|-------|-------|--------|
| binarytrees | 70.3  | 70.2     | 9.8  | 18.1 | 12.4  | 102.4 | 21.8   |
| fannkuch    | 2.2   | 2.1      | 1.7  | 2.2  | 2.0   | 58.2  | 9.7    |
| fib         | 2.1   | 2.2      | 1.7  | 2.1  | 2.0   | 57.6  | 9.3    |
| mandelbrot  | 2.2   | 2.2      | 1.7  | 2.0  | 2.1   | 60.3  | 10.1   |
| nbody       | 2.2   | —        | 1.7  | 2.1  | 2.0   | 61.2  | 10.3   |
| wordfreq    | 79.9  | —        | 16.2 | 47.3 | 108.1 | 229.4 | 158.5  |

## 2026-11-16 21:22, commit 570c72b

- CPU: Intel(R) Core(TM) i5-4308U CPU @ 2.80GHz
- lugha: lughac 0.1.0
- c: gcc (Ubuntu 15.2.0-16ubuntu1) 15.2.0
- rust: rustc 1.99.0 (b940084d7 2026-09-28)
- go: go version go1.26.8 linux/amd64
- node: v22.22.1

Median wall time of 5 runs:

| benchmark   | lugha  | lugha-O0 | c      | rust   | go     | node   |
|-------------|--------|----------|--------|--------|--------|--------|
| binarytrees | 1.093s | 1.260s   | 0.537s | 1.381s | 0.817s | 0.547s |
| fannkuch    | 0.249s | 1.018s   | 0.211s | 0.216s | 0.223s | 0.468s |
| fib         | 0.566s | 1.099s   | 0.230s | 0.363s | 0.584s | 1.939s |
| mandelbrot  | 0.367s | 0.815s   | 0.366s | 0.408s | 0.384s | 0.546s |
| nbody       | 0.656s | 2.748s   | 0.535s | 0.610s | 0.693s | 0.996s |
| wordfreq    | 0.581s | 1.154s   | 0.098s | 0.154s | 0.366s | 1.126s |

Fastest run:

| benchmark   | lugha  | lugha-O0 | c      | rust   | go     | node   |
|-------------|--------|----------|--------|--------|--------|--------|
| binarytrees | 1.084s | 1.246s   | 0.521s | 1.346s | 0.783s | 0.534s |
| fannkuch    | 0.245s | 0.991s   | 0.210s | 0.215s | 0.223s | 0.460s |
| fib         | 0.561s | 1.078s   | 0.229s | 0.354s | 0.570s | 1.881s |
| mandelbrot  | 0.365s | 0.811s   | 0.363s | 0.406s | 0.376s | 0.542s |
| nbody       | 0.654s | 2.723s   | 0.534s | 0.604s | 0.689s | 0.991s |
| wordfreq    | 0.557s | 1.140s   | 0.096s | 0.150s | 0.346s | 1.107s |

Peak memory (MB, median):

| benchmark   | lugha | lugha-O0 | c    | rust | go    | node  |
|-------------|-------|----------|------|------|-------|-------|
| binarytrees | 70.3  | 70.3     | 9.7  | 18.1 | 12.2  | 107.9 |
| fannkuch    | 2.0   | 2.1      | 1.7  | 2.1  | 1.8   | 62.4  |
| fib         | 2.2   | 2.1      | 1.7  | 2.1  | 1.8   | 61.9  |
| mandelbrot  | 2.2   | 2.0      | 1.7  | 2.0  | 1.9   | 64.5  |
| nbody       | 2.1   | 2.2      | 1.7  | 2.1  | 2.0   | 65.2  |
| wordfreq    | 83.3  | 100.4    | 16.2 | 47.1 | 118.9 | 233.4 |

## 2026-11-16, commit cb7a362, the reference-counting prototype

`--lang lugha,lugha-rc`: the collector against `--memory=rc` (PRP-222), both at `-O2`, same machine as the baseline above. Nothing decided yet: counts aren't elided anywhere, and memory comes from the C library's `malloc`.

Median wall time of 5 runs:

| benchmark   | lugha  | lugha-rc |
|-------------|--------|----------|
| binarytrees | 1.065s | 2.413s   |
| fannkuch    | 0.248s | 0.255s   |
| fib         | 0.565s | 0.566s   |
| mandelbrot  | 0.366s | 0.370s   |
| nbody       | 0.656s | 0.663s   |
| wordfreq    | 0.561s | 0.557s   |

Fastest run:

| benchmark   | lugha  | lugha-rc |
|-------------|--------|----------|
| binarytrees | 1.055s | 2.363s   |
| fannkuch    | 0.246s | 0.250s   |
| fib         | 0.560s | 0.559s   |
| mandelbrot  | 0.364s | 0.370s   |
| nbody       | 0.653s | 0.656s   |
| wordfreq    | 0.544s | 0.555s   |

Peak memory (MB, median):

| benchmark   | lugha | lugha-rc |
|-------------|-------|----------|
| binarytrees | 70.2  | 25.8     |
| fannkuch    | 2.2   | 1.9      |
| fib         | 2.2   | 1.8      |
| mandelbrot  | 2.2   | 1.8      |
| nbody       | 2.2   | 1.9      |
| wordfreq    | 83.5  | 138.4    |



## 2026-11-16, commit d232701, reference counting across tasks

`--lang lugha,lugha-rc,c,rust,go`, same machine as the baseline: counts now carry the shared bit (one unsigned compare inline), and `chanping` joins (C and Rust hand off between OS threads). Nothing decided yet.

Median wall time of 5 runs:

| benchmark   | lugha  | lugha-rc | c       | rust    | go     |
|-------------|--------|----------|---------|---------|--------|
| binarytrees | 1.084s | 2.428s   | 0.515s  | 1.337s  | 0.790s |
| chanping    | 0.285s | 0.277s   | 30.510s | 31.976s | 2.802s |
| fannkuch    | 0.258s | 0.255s   | 0.232s  | 0.216s  | 0.230s |
| fib         | 0.597s | 0.574s   | 0.229s  | 0.409s  | 0.576s |
| mandelbrot  | 0.365s | 0.364s   | 0.363s  | 0.405s  | 0.374s |
| nbody       | 0.656s | 0.656s   | 0.536s  | 0.605s  | 0.691s |
| wordfreq    | 0.533s | 0.575s   | 0.095s  | 0.151s  | 0.366s |

Fastest run:

| benchmark   | lugha  | lugha-rc | c       | rust    | go     |
|-------------|--------|----------|---------|---------|--------|
| binarytrees | 1.073s | 2.413s   | 0.508s  | 1.335s  | 0.778s |
| chanping    | 0.282s | 0.273s   | 28.490s | 29.446s | 2.413s |
| fannkuch    | 0.253s | 0.250s   | 0.215s  | 0.216s  | 0.229s |
| fib         | 0.562s | 0.566s   | 0.227s  | 0.360s  | 0.567s |
| mandelbrot  | 0.363s | 0.362s   | 0.361s  | 0.404s  | 0.374s |
| nbody       | 0.651s | 0.648s   | 0.531s  | 0.601s  | 0.681s |
| wordfreq    | 0.518s | 0.572s   | 0.094s  | 0.149s  | 0.323s |

Peak memory (MB, median):

| benchmark   | lugha | lugha-rc | c    | rust | go    |
|-------------|-------|----------|------|------|-------|
| binarytrees | 70.3  | 25.9     | 9.7  | 18.0 | 12.0  |
| chanping    | 2.0   | 1.9      | 1.8  | 2.3  | 2.0   |
| fannkuch    | 2.2   | 1.9      | 1.7  | 2.1  | 1.9   |
| fib         | 2.0   | 1.9      | 1.7  | 2.0  | 1.8   |
| mandelbrot  | 2.2   | 1.9      | 1.7  | 2.2  | 1.8   |
| nbody       | 2.1   | 1.9      | 1.7  | 2.1  | 2.0   |
| wordfreq    | 81.6  | 138.4    | 16.2 | 47.1 | 128.2 |




## 2026-11-17, commit 94f12a7, the memory measurement and the decision

`--lang lugha,lugha-rc,lugha-rc-calls,c,rust,go`, same machine. Since the last entry, for both models: an enum's payload is addressed so LLVM keeps it in registers, and the runtime is compiled at `-O2`. Under rc only: free lists by size class, a local released after its last mention, `if` and `match` values handed through. `lugha-rc-calls` adds to every function the stack check and scheduling countdown that growable stacks and preemption will need (`LUGHA_CALL_COSTS=1`).

Median wall time of 5 runs:

| benchmark   | lugha  | lugha-rc | lugha-rc-calls | c       | rust    | go     |
|-------------|--------|----------|----------------|---------|---------|--------|
| binarytrees | 0.648s | 0.719s   | 0.768s         | 0.524s  | 1.420s  | 0.858s |
| chanping    | 0.121s | 0.111s   | 0.113s         | 29.880s | 32.690s | 2.940s |
| fannkuch    | 0.256s | 0.252s   | 0.380s         | 0.212s  | 0.216s  | 0.224s |
| fib         | 0.563s | 0.568s   | 0.792s         | 0.240s  | 0.356s  | 0.570s |
| mandelbrot  | 0.363s | 0.368s   | 0.388s         | 0.363s  | 0.405s  | 0.377s |
| nbody       | 0.653s | 0.667s   | 0.756s         | 0.536s  | 0.611s  | 0.681s |
| wordfreq    | 0.503s | 0.402s   | 0.506s         | 0.111s  | 0.150s  | 0.361s |

Fastest run:

| benchmark   | lugha  | lugha-rc | lugha-rc-calls | c       | rust    | go     |
|-------------|--------|----------|----------------|---------|---------|--------|
| binarytrees | 0.631s | 0.709s   | 0.753s         | 0.519s  | 1.404s  | 0.821s |
| chanping    | 0.120s | 0.110s   | 0.113s         | 19.171s | 29.032s | 2.559s |
| fannkuch    | 0.250s | 0.245s   | 0.376s         | 0.209s  | 0.215s  | 0.222s |
| fib         | 0.560s | 0.560s   | 0.789s         | 0.235s  | 0.352s  | 0.562s |
| mandelbrot  | 0.363s | 0.365s   | 0.380s         | 0.363s  | 0.405s  | 0.373s |
| nbody       | 0.646s | 0.661s   | 0.746s         | 0.529s  | 0.603s  | 0.677s |
| wordfreq    | 0.493s | 0.391s   | 0.501s         | 0.097s  | 0.150s  | 0.343s |

Peak memory (MB, median):

| benchmark   | lugha | lugha-rc | lugha-rc-calls | c    | rust | go    |
|-------------|-------|----------|----------------|------|------|-------|
| binarytrees | 37.3  | 25.8     | 25.9           | 9.8  | 18.0 | 11.8  |
| chanping    | 2.1   | 1.9      | 1.9            | 1.7  | 2.3  | 2.0   |
| fannkuch    | 2.1   | 1.8      | 1.8            | 1.7  | 2.1  | 1.8   |
| fib         | 2.1   | 1.8      | 1.9            | 1.7  | 2.0  | 1.8   |
| mandelbrot  | 2.1   | 1.8      | 1.8            | 1.7  | 2.2  | 1.8   |
| nbody       | 2.1   | 1.8      | 1.8            | 1.7  | 2.1  | 2.0   |
| wordfreq    | 81.5  | 120.1    | 120.2          | 16.2 | 47.2 | 110.9 |

Pauses (`bench/measure/pauses.la 20000`, two runs each: 256 live trees, one replaced per round, a round normally well under 1 ms):

| | longest round | rounds of 2 ms or more | time | peak memory |
|---|---|---|---|---|
| collector | 16 ms | 97, 98 of 20000 | 2.9 s | 63 MB |
| reference counting | 4 ms | 3, 7 of 20000 | 3.1 s | 51 MB |

### Against the targets

The Draft's baseline is 2026-10-31 (binarytrees 1.114 s / 70.3 MB, wordfreq 0.557 s / 83.4 MB, fib 0.562 s). The collector itself is now faster on binarytrees (0.648 s / 37 MB), so each line gives both comparisons.

| target | reference counting | met? |
|---|---|---|
| PRF-01: fannkuch, fib, mandelbrot, nbody within 3% | 0.252 / 0.568 / 0.368 / 0.667 s: within 2% of the collector and of the baseline | yes |
| PRF-02: binarytrees at most 10% slower | 0.719 s: 35% faster than the baseline, 11% slower than today's collector | yes against the baseline; just over against today's collector |
| PRF-02: wordfreq at most 10% slower | 0.402 s: 20% faster than the collector | yes |
| PRF-03: binarytrees at most 35 MB | 25.8 MB | yes |
| PRF-03: wordfreq no worse than 83.4 MB (aim 47) | 120.1 MB | **no** |
| PRF-04: no pause over 1 ms | longest 4 ms, in 3 to 7 rounds of 20000, against the collector's 16 ms in about 100 | nearly: the few slow rounds free the same amount as every other round, so they look like the system's own scheduling, not memory management |
| PRF-05: channel ping-pong within 1.5× of Go | 0.111 s against Go's 2.94 s | yes |
| PRF-05: parallel binarytrees, idle task size | not measurable before the multi-threaded runtime | — |
| PRF-06 and PRF-09: fib with every new per-call cost, within 3% | 0.792 s, +39% (fannkuch +51%, nbody +13%) | **no**, but not because of counting: fib holds no counted values, and the whole cost is the stack check and the countdown |

### What the misses would cost to close

- **wordfreq's memory** is the header. Its two million short strings take 32 or 48 bytes each under counting, against 16 or 32 under the collector (header 16 bytes, length 8, bytes and NUL). Releasing `picked` at its last use saved 16 MB. A one-word header (count and a type index) with 8-byte size classes would bring the strings to 24 or 32 bytes, about 105 MB peak (estimated from the size-class counts), still above the target. Only a smaller representation for short strings (bytes stored inline, no object at all up to 15 bytes) would reach the collector's figure or the 47 MB aim. That is a change to strings, not to the memory model, and plan B's per-task heaps would need a mark bit and size information per object too.
- **The per-call costs** are the same under either model: plan B needs the stack check and the countdown as much as counting does. They decide how growable stacks and preemption are built (a countdown only at loop back-edges and in functions that loop, or a stack check folded into a frame's existing setup), not which memory model to use.
- **binarytrees against today's collector** misses by about 1%; freeing every node through an indirect call to its visit, where the collector frees nothing one at a time, is what remains.

### Recommendation

**Reference counting.** Every target it can affect is met or nearly met, except wordfreq's memory, and that miss comes from per-object headers on very small strings, which a tracing heap would also pay for in some form. It is faster than the collector on wordfreq and chanping, uses a third less memory on binarytrees, and its pauses are a quarter of the collector's at their longest. Plan B would need its own allocator and a scan of every task's stack to reach numbers the prototype already has. The two follow-ups are separate decisions: short strings stored inline (for PRF-03), and how stack checks and the countdown are placed (for PRF-06 and PRF-09).
