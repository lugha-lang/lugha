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


