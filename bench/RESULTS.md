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

## 2026-11-18, unwinding

Landing pads (a panic runs `defer`s and releases while unwinding). The machine was busy, so the compiler before and after were compared directly: each binary built from `main` and from the branch, run alternately, fastest of 5, `-O2`.

| benchmark | before | after | size before | size after |
|---|---|---|---|---|
| binarytrees | 0.77 s | 0.81 s | 101.6 KB | 106.5 KB |
| fib | 0.56 s | 0.57 s | 97.3 KB | 102.2 KB |
| wordfreq | 0.40 s | 0.39 s | 114.5 KB | 127.7 KB |
| nbody | 0.67 s | 0.67 s | 106.0 KB | 111.0 KB |

`fib` has no landing pads, so its code is unchanged; binarytrees' `make` holds counted values across its calls, which now unwind to pads (+5%). Binaries grow 5–12% with the unwind tables and pads.

## 2026-11-18 — the stack check and C on its own stack (PRP-237, milestone 205)

Each program built once by the previous compiler (`main`) and once by this one, at `-O2`, then run 9 times each, interleaved (one machine; `bench/run.py`'s consecutive runs swung by 10–30%). The check sits at the entry of each function left after optimising.

| benchmark   | before  | after   | change |
|-------------|---------|---------|--------|
| fib         | 0.578s  | 0.778s  | +34.6% |
| fannkuch    | 0.244s  | 0.254s  | +4.2%  |
| mandelbrot  | 0.365s  | 0.366s  | +0.4%  |
| nbody       | 0.743s  | 0.751s  | +1.0%  |
| binarytrees | 0.887s  | 0.887s  | -0.1%  |
| wordfreq    | 0.484s  | 0.482s  | -0.4%  |

With the check inserted before optimising, nbody was +11%: a function inlined into a loop checked on every turn. fib pays a load, a compare and a branch per call, which does little else; fib and fannkuch miss the 3% target, accepted for now (TODO.md).

## 2026-11-18 — helper threads (PRP-238, milestone 206)

`dir.read_file` of a 13-byte file 100,000 times, one task, at `-O2`, three runs each: inline (`LUGHA_SCHED=deterministic`, which is how every read ran before) against a helper thread.

| how    | 100,000 reads | one read |
|--------|---------------|----------|
| inline | 0.30–0.31 s   | ~3 µs    |
| helper | 1.93–2.35 s   | ~21 µs   |

Each read hands its job to a helper and is woken back through the `epoll` set: two wake-ups across threads, which cost far more than reading a cached file. Recorded and accepted for now; the fix (TODO.md) tries each call in place first and hands it over only when the kernel says it would wait.


## 2026-11-18 — groundwork for many cores (PRP-240, milestone 208)

Each program built once by the previous compiler (`main`) and once by this one, at `-O2`, then run 9 times each, interleaved; the median of each.

| benchmark   | before  | after   | change |
|-------------|---------|---------|--------|
| fib         | 0.785s  | 0.777s  | -1.0%  |
| fannkuch    | 0.257s  | 0.257s  | -0.1%  |
| mandelbrot  | 0.367s  | 0.367s  | +0.0%  |
| nbody       | 0.685s  | 0.690s  | +0.6%  |
| binarytrees | 0.867s  | 0.873s  | +0.7%  |
| wordfreq    | 0.483s  | 0.483s  | -0.1%  |
| chanping    | 0.154s  | 0.176s  | +13.7% |

What was tried on the way:

- **Atomic counts in generated code.** `monotonic` loads and stores cost fannkuch 5%: LLVM won't hoist a `monotonic` load out of a loop. `unordered` (never torn, defined under a race, still hoistable) costs nothing, and is what shipped.
- **The owner check's test in copy-on-write.** Testing for a checked count of one cost fannkuch 4%, even placed off the hot path; it is emitted only in builds made with `LUGHA_RC_CHECK=1`.
- **The waiting handoff.** In a C ping-pong harness built against the runtime, the compare-and-swaps cost about 20% and a flag read at run time to skip them about 8%; they are compiled out while one worker runs. Moving the new `Fiber` fields after the hot ones recovered a few percent. What remains on `chanping` is the handoff's extra checks: accepted for now, TODO.md.

## 2026-11-18 — an allocator for many threads (PRP-241, milestone 209)

Each program built by the previous compiler (milestone 208) and by this one, at `-O2`, then run 9 times each, interleaved; the median of each.

| benchmark   | before  | after   | change |
|-------------|---------|---------|--------|
| fib         | 0.784s  | 0.779s  | -0.6%  |
| fannkuch    | 0.260s  | 0.254s  | -2.4%  |
| mandelbrot  | 0.367s  | 0.368s  | +0.1%  |
| nbody       | 0.681s  | 0.679s  | -0.3%  |
| binarytrees | 0.893s  | 0.895s  | +0.2%  |
| wordfreq    | 0.486s  | 0.504s  | +3.8%  |
| chanping    | 0.174s  | 0.180s  | +3.4%  |

wordfreq moved +0.7% on the run before this one: noise. With `worker` a thread-local pointer to the record, binarytrees was +4.6% and chanping +12%: every field took two dependent loads, the pointer reloaded after each call. With the record itself thread-local, each field is one `fs`-relative access. The mimalloc baseline wasn't measured: it isn't installed here, and only blocks over 272 bytes would go through it.

## 2026-11-19 — tasks on every core (PRP-242, milestone 210)

Each program built by milestone 209's compiler and by this one, at `-O2`, run 9 times each, interleaved; the median of each. The machine has 2 cores, 4 threads.

| benchmark   | before  | after   | change |
|-------------|---------|---------|--------|
| fib         | 0.782s  | 0.690s  | -11.8% |
| fannkuch    | 0.260s  | 0.260s  | -0.3%  |
| mandelbrot  | 0.367s  | 0.369s  | +0.6%  |
| nbody       | 0.680s  | 0.679s  | -0.2%  |
| binarytrees | 0.912s  | 0.927s  | +1.7%  |
| wordfreq    | 0.500s  | 0.507s  | +1.5%  |
| chanping    | 0.183s  | 0.277s  | +51.5% |

Four tasks each summing `i % 7` for 400 million `i`, then sending the sum to `main`:

| workers | time  |
|---------|-------|
| 1       | 3.22s |
| 2       | 1.70s |
| 4       | 1.58s |

Two cores give 1.9×; hyperthreads add little to a loop of integer division.

- **The stack check and budget** from the stack pointer (`sp & mask`, the budget in the stack's top word) measured alone: fib +2.5%, nbody −0.3%, chanping −6% against milestone 209. fib's final −11.8% came with the rest; code layout, likely.
- **chanping** is slower on one worker too (0.25–0.27 s): measured in a C harness against the runtime, about 0.03 s of the 0.075 s is reading the thread pointer afresh on each access (a task may have moved between threads), about 0.04 s the lock's sections, even with the lock itself skipped while one thread runs, and the rest the switch's extra bookkeeping. Accepted for now; the next PRP reworks exactly these (TODO.md).

## 2026-11-20 — a scheduler for many cores (PRP-243, milestone 211)

Each program built by milestone 210's compiler and by this one, at `-O2`, run 9 times each, interleaved; the median of each. The machine has 2 cores, 4 threads.

| benchmark   | before  | after   | change |
|-------------|---------|---------|--------|
| fib         | 0.688s  | 0.694s  | +0.9%  |
| fannkuch    | 0.256s  | 0.258s  | +0.9%  |
| mandelbrot  | 0.371s  | 0.371s  | -0.1%  |
| nbody       | 0.679s  | 0.678s  | -0.3%  |
| binarytrees | 0.902s  | 0.896s  | -0.7%  |
| wordfreq    | 0.497s  | 0.501s  | +0.7%  |
| chanping    | 0.267s  | 0.390s  | +45.7% |

`chanping`, three runs each:

| workers | milestone 209 | milestone 210 | milestone 211 |
|---------|---------------|---------------|---------------|
| 1       | 0.17–0.18s    | 0.24–0.25s    | 0.29–0.37s    |
| 4       | 0.18s         | 0.26s         | 0.37–0.38s    |

PRP-242's four busy tasks, two runs each: 3.15–3.26 s on one worker, 1.58–1.70 s on two, 1.55–1.60 s on four, the same as milestone 210.

The target was chanping within 10% of milestone 209: missed, and shipped with the miss (the user's call). A C harness against the runtime (0.305 s for 2 million round trips on one worker, against 0.16 s at milestone 209) put the cost at about 0.05 s for reading the thread pointer afresh on each access, 0.03 s for the parking handoff's compare-and-swaps, 0.03 s for the scheduling tick, 0.01 s for the run-next exchanges and the rest in the sections. Kept: passing the lock between two fibers resuming inside sections (four workers 0.41 s → 0.36 s in the harness), plain handoff operations until workers start, a tick that does nothing when nothing is pending. Go takes about 2.5 s on the same benchmark here.

## 2026-11-20 — locks of their own for the hot paths (PRP-244, milestone 212)

`bench/measure/scaling.py`, milestone 211 against this one, the fastest of three runs, on 1, 2 and 4 workers (2 cores, 4 threads):

| pattern | m211 w1 | m211 w2 | m211 w4 | m212 w1 | m212 w2 | m212 w4 |
|---|---|---|---|---|---|---|
| spawn_many 200000 | 1.394s | 1.542s | 1.627s | 1.381s | 1.594s | 1.543s |
| yield_many 100000 | 0.028s | 0.015s | 0.013s | 0.031s | 0.016s | 0.011s |
| chained_spawn 2000 | 0.014s | 0.027s | 0.022s | 0.014s | 0.022s | 0.018s |
| pairs 100000 | 0.130s | 0.551s | 0.821s | 0.149s | 0.090s | 0.081s |
| trees 14 | 2.426s | 1.363s | 0.948s | 2.400s | 1.358s | 1.029s |

Eight ping-pong pairs on their own channels went from slowing down with workers (the shared lock taken by every send and receive) to scaling: 10× faster on four workers. Spawning many tasks from one doesn't scale yet (TODO.md).

The single-task benchmarks, 9 interleaved runs each, medians:

| benchmark   | before  | after   | change |
|-------------|---------|---------|--------|
| fib         | 0.687s  | 0.687s  | +0.1%  |
| fannkuch    | 0.257s  | 0.253s  | -1.6%  |
| mandelbrot  | 0.371s  | 0.375s  | +0.9%  |
| nbody       | 0.690s  | 0.682s  | -1.1%  |
| binarytrees | 0.914s  | 0.905s  | -1.0%  |
| wordfreq    | 0.497s  | 0.492s  | -1.0%  |
| chanping    | 0.379s  | 0.395s  | +4.2%  |

chanping on four workers is within the 5% target; on one worker it went from 0.29–0.35 s to 0.36–0.39 s: channel locks are real atomics even on one thread, where the shared lock was free.

## 2026-11-20 — spawning that scales (PRP-245, milestone 213)

`bench/measure/scaling.py`, milestone 212 against this one, the fastest of three runs, on 1, 2 and 4 workers (2 cores, 4 threads):

| pattern | m212 w1 | m212 w2 | m212 w4 | m213 w1 | m213 w2 | m213 w4 |
|---|---|---|---|---|---|---|
| spawn_many 200000 | 1.431s | 1.611s | 1.551s | 1.410s | 1.383s | 1.976s |
| spawners 200000 | 1.335s | 1.509s | 2.114s | 1.358s | 1.032s | 1.148s |
| yield_many 100000 | 0.032s | 0.018s | 0.012s | 0.031s | 0.016s | 0.010s |
| chained_spawn 2000 | 0.015s | 0.019s | 0.019s | 0.014s | 0.026s | 0.019s |
| pairs 100000 | 0.151s | 0.090s | 0.079s | 0.153s | 0.121s | 0.081s |
| trees 14 | 2.386s | 1.337s | 1.018s | 2.423s | 1.335s | 1.069s |

`spawn_many` varies from run to run; five runs each settle it: on one worker 1.30–1.35 s at m212 against 1.36–1.51 s now, on four 1.57–1.62 s against 1.40–1.43 s. Eight tasks spawning in scopes of their own went from slowing down with workers to scaling (2.11 s to 1.15 s on four). One task spawning many is 9% faster on four workers but about 5% slower on one: its 200,000 tasks are all queued before any finishes, so the stack cache never gets a stack back, and every finished stack past it still gives its pages back; the scope's lock is now a real atomic even on one thread (TODO.md).

The single-task benchmarks and chanping, 9 interleaved runs each, medians:

| benchmark   | before  | after   | change |
|-------------|---------|---------|--------|
| binarytrees | 0.885s  | 0.881s  | -0.5%  |
| chanping    | 0.393s  | 0.398s  | +1.2%  |
| fannkuch    | 0.259s  | 0.259s  | +0.0%  |
| fib         | 0.694s  | 0.691s  | -0.3%  |
| mandelbrot  | 0.370s  | 0.367s  | -0.6%  |
| nbody       | 0.685s  | 0.684s  | -0.1%  |
| wordfreq    | 0.558s  | 0.537s  | -3.9%  |

## 2026-11-20 — pinned handles and pinned scopes (PRP-246, milestone 214)

Nothing here pins, so these check that the inbox, and the lost wake-up fixed on the way, cost nothing. `bench/measure/scaling.py`, milestone 213 against this one, the fastest of three runs, on 1, 2 and 4 workers (2 cores, 4 threads):

| pattern | m213 w1 | m213 w2 | m213 w4 | m214 w1 | m214 w2 | m214 w4 |
|---|---|---|---|---|---|---|
| spawn_many 200000 | 1.295s | 1.309s | 1.355s | 1.381s | 1.477s | 1.404s |
| spawners 200000 | 1.595s | 0.996s | 0.957s | 1.486s | 0.948s | 0.858s |
| yield_many 100000 | 0.031s | 0.016s | 0.011s | 0.030s | 0.016s | 0.011s |
| chained_spawn 2000 | 0.014s | 0.020s | 0.018s | 0.015s | 0.021s | 0.018s |
| pairs 100000 | 0.154s | 0.084s | 0.087s | 0.147s | 0.082s | 0.083s |
| trees 14 | 2.212s | 1.543s | 1.015s | 2.355s | 1.400s | 0.829s |

Within run-to-run noise: `spawn_many` varies by 10% between runs of one build (milestone 213's entry), `trees` by as much. An earlier run of this script stalled past 25 minutes; 360 timed runs of every pattern on both builds afterwards found no hang.

The single-task benchmarks and chanping, 9 interleaved runs each, medians:

| benchmark   | before  | after   | change |
|-------------|---------|---------|--------|
| binarytrees | 0.830s  | 0.835s  | +0.7%  |
| chanping    | 0.393s  | 0.390s  | -0.6%  |
| fannkuch    | 0.250s  | 0.251s  | +0.4%  |
| fib         | 0.671s  | 0.669s  | -0.2%  |
| mandelbrot  | 0.368s  | 0.366s  | -0.4%  |
| nbody       | 0.675s  | 0.665s  | -1.5%  |
| wordfreq    | 0.487s  | 0.481s  | -1.1%  |
