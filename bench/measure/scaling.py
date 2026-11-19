#!/usr/bin/env python3
"""How work spread over tasks speeds up with workers: scaling.la's five
patterns on 1, 2 and 4 workers, the fastest of three runs each.

    python3 bench/measure/scaling.py                      # the release lughac
    python3 bench/measure/scaling.py --lughac old=/path/to/lughac new=target/release/lughac

Each `--lughac NAME=PATH` builds the program with that compiler, so two
compilers can be compared side by side. Never in CI.
"""

import argparse
import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE = ROOT / "bench" / "measure" / "scaling.la"
BUILD = ROOT / "bench" / ".build"
PATTERNS = [
    ("spawn_many", "200000"),
    ("yield_many", "100000"),
    ("chained_spawn", "2000"),
    ("pairs", "100000"),
    ("trees", "14"),
]
WORKERS = ["1", "2", "4"]


def build(name, lughac):
    """`scaling.la` built with `lughac` at -O2, into bench/.build/."""
    BUILD.mkdir(exist_ok=True)
    exe = BUILD / f"scaling-{name}"
    subprocess.run([lughac, "build", str(SOURCE), "-O2", "-o", str(exe)], check=True)
    return exe


def fastest(exe, pattern, size, workers):
    """The fastest of three runs, in seconds."""
    env = dict(os.environ, LUGHA_WORKERS=workers)
    best = None
    for _ in range(3):
        started = time.perf_counter()
        subprocess.run([str(exe), pattern, size], env=env, check=True, stdout=subprocess.DEVNULL)
        took = time.perf_counter() - started
        best = took if best is None else min(best, took)
    return best


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lughac", action="append", default=[],
                        help="NAME=PATH of a lughac to build with (repeatable)")
    args = parser.parse_args()
    compilers = [c.split("=", 1) for c in args.lughac] or [
        ["lughac", str(ROOT / "target" / "release" / "lughac")]
    ]
    exes = [(name, build(name, path)) for name, path in compilers]
    header = ["pattern"] + [f"{name} w{w}" for name, _ in exes for w in WORKERS]
    print("| " + " | ".join(header) + " |")
    print("|" + "|".join("---" for _ in header) + "|")
    for pattern, size in PATTERNS:
        cells = [f"{pattern} {size}"]
        for _, exe in exes:
            for workers in WORKERS:
                cells.append(f"{fastest(exe, pattern, size, workers):.3f}s")
        print("| " + " | ".join(cells) + " |", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
