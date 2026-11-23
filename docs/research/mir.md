# A mid-level IR: research before any decision

Would Lugha gain from its own IR between the checker and LLVM (a "MIR": a control-flow graph with counting, cleanup and unwinding as explicit instructions)? This note gathers a profile of our benchmarks and what other reference-counted languages found. Nothing here is decided; adopting an IR would reopen decision 14 ("no separate typed IR").

Sources (2026-11-23): `perf` on `bench/programs/` at their timed sizes; Groff & Lattner, "Swift's High-Level IR" (LLVM Dev Mtg 2015); Swift's `docs/ARCOptimization.md`, `docs/SIL/Ownership.md` and the 2021 performance-predictability roadmap (Swift forums); Reinking, Xie, de Moura, Leijen, "Perceus" (PLDI 2021, MSR-TR-2020-42); Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019); "Introducing MIR" (Rust blog, 2016), the Rust 1.12 and NLL-by-default posts, rust-lang/rust#91743; Nim's `injectdestructors.nim` and the ORC announcement; Roc's Zulip thread on removing Morphic; LLVM's `ObjCARCOpts.cpp`; Stadler et al., "Partial Escape Analysis" (CGO 2014); the Go blog on allocation optimisations. Claims taken from abstracts only are marked *abstract*.

## 1. Where our time goes today

Profiled with `lugha_rc_inc`/`lugha_rc_dec` made separate functions so they can be seen (that build runs within ±15% of the normal one); share of samples:

| benchmark | counting | allocating | freeing | program | other |
|---|---|---|---|---|---|
| fib, fannkuch, mandelbrot | 0% | 0% | 0% | 99% | — |
| nbody | 0.8% | 0% | 0% | 99% | — |
| chanping | 0% | 0% | 0% | 3% | 90% scheduler and channels |
| wordfreq | 6% | 9% | 10.5% | 35% | libc 12%, kernel 18%, runtime 10% |
| binarytrees | 8% | 27% | 34% | 29% | — |

- Count operations are cheap: at most 8%, and `lugha_rc_inc` never appears. Moves on last use and borrowed reads already avoid most increments; what's left are decrements at frees.
- Allocation and freeing dominate the allocation-heavy programs, but binarytrees' trees outlive their functions (no stack allocation possible): the cost is the runtime's allocate and free paths (`lugha_rt_rc_alloc`, `rc_free_one`, `rc_block_free`). Lugha 1.2 s against C's 0.52 s, both allocating each node.
- wordfreq allocates a string per word slice; slices sharing the original text would remove more than an IR would.
- Numeric programs never touch counting; their gap to C is overflow checks and the step countdown.
- Counting is already cheap where it is unshared: the fast paths write a plain count, never an atomic one, for objects no other thread holds.

Seven small programs: a real one passing structs with counted fields, closures and interface values may show more. The next measurement is a profile of a real program (the skill's examples, lugha-gtk).

## 2. What others found

| language | IR | main counting and allocation work | reported gain | cost |
|---|---|---|---|---|
| Swift | SIL, later OSSA (CFG, SSA) | retain/release motion and pairing, stack promotion, devirtualisation, specialisation | none published; counting up to 80% of time in outliers (*abstract*) | very large; OSSA still incomplete after 8+ years |
| Koka | functional core with explicit control flow | precise dup/drop, reuse | over 2× on rbtree; about none on sharing-heavy code | small |
| Lean 4 | λpure → λRC (ANF, join points) | inc/dec, borrow inference, reset/reuse | reuse up to 3.2× (geomean ≈1.4×); borrow inference ≈1.15× on two benchmarks | moderate |
| Rust | MIR (CFG) | drop elaboration, borrow checking, MIR inliner | inliner −1.4% mean compile instructions | ≈1 year to codegen parity, ≈7 to retire the AST borrow checker |
| Nim | AST plus a CFG built on demand | move on last use, cursor inference | ORC vs mark-and-sweep: ≈5× lower latency, 2.4× less memory on one server | low to moderate |
| Roc | monomorphised IR plus whole-program alias analysis | Perceus, uniqueness | alias analysis: no measurable gain, a miscompile, proposed for removal | the analysis was costly |
| Objective-C | LLVM IR (ObjCARCOpts) | local pairing of runtime calls | — | fragile: recognises runtime calls by name |
| Java (Graal) | sea of nodes | partial escape analysis | up to 33% faster, 58.5% less allocation (*abstract*) | JIT-scale |

- **Why LLVM alone isn't enough** (Swift's ARC doc, ObjCARCOpts): a release can run arbitrary code, a uniqueness check (copy-on-write) blocks motion, critical edges block placement; LLVM sees calls, not ownership.
- **What needs a CFG-level IR:** precise drop placement and last-use moves across branches, loops and panic edges; moving and pairing retains and releases across blocks and out of loops; flow-sensitive diagnostics (definite initialisation, use after move); flow-sensitive escape analysis.
- **What doesn't:** Nim does move-on-last-use with a CFG built over the AST only when asked; Lean and Koka use a structured ANF tree, not basic blocks; function-level borrow inference.
- **The biggest wins came from a few counting-specific things** (explicit dup/drop, reuse, borrowing, avoiding atomics), never from a general optimiser. Avoiding atomic counts alone was worth 5–131% in Koka and Lean; Lugha already does this.

## 3. Lessons for Lugha

1. **Keep any IR narrow.** Lean's is a small ANF language that only makes ownership and joins explicit. An IR with counting, cleanup and unwind edges, not a SIL.
2. **Fix lifetime meaning first.** Swift's optimiser shortening lifetimes broke working code for years. The spec should say whether a value is let go of at scope end or at last use, and what a `defer` or a `Weak` may observe, before an optimiser moves releases.
3. **Adopt it incrementally, beside today's codegen.** Rust ran MIR beside the old path and compared outputs; Swift lowered ownership away so later passes didn't change; Nim builds a CFG over the AST only where a question needs it, the cheapest first step from where Lugha is.
4. **Verify ownership in the IR.** OSSA's "exactly one consuming use on every path" turned counting bugs into compile-time errors.
5. **Beware analyses that don't pay** (Roc's whole-program alias analysis).

## 4. What this suggests

On today's benchmarks the case for a MIR is weak: counting is ≤ 8%, and the allocation cost that dominates is mostly the runtime's, or the library's (string slices). The bigger levers now are the allocate and free paths, slices that share text, and the scheduler's hot path. An IR becomes worth it when a profile of real programs shows counting or avoidable allocation as a large share, or when a second backend (Cranelift for debug builds, an interpreter for replay) is wanted; then Nim's step, a CFG over the AST for last-use questions, comes first, and a narrow Lean-style IR after.

## 5. A playground beside the compiler

Apart from performance, a MIR is worth building to learn from, if it can't destabilise anything. The shape that allows it: a stage beside codegen, never on the build path.

```
source → AST → check ─┬→ codegen → LLVM          (unchanged, the only real path)
                      └→ mir (experimental) → --emit=mir, an interpreter
```

It reads the same AST and `Checked` codegen reads (decision 14 stays true: codegen's input doesn't change), and its only outputs are a printout and an interpreter used by tests. Anything not yet lowered says `unsupported: <construct>`.

### Types (`src/mir/`)

```rust
pub struct Body { pub name: String, pub params: Vec<Local>, pub locals: Vec<LocalDecl>, pub blocks: Vec<Block> }
pub struct Local(pub u32);       // _0 is the return value
pub struct BlockId(pub u32);     // bb0 is the entry
pub struct Block { pub statements: Vec<Statement>, pub terminator: Terminator }

pub enum Statement { Assign(Local, Rvalue), Retain(Local), Release(Local) }
pub enum Rvalue { Use(Operand), Binary(BinOp, Operand, Operand), Field(Local, u32), Struct(TypeId, Vec<Operand>) }
pub enum Operand { Local(Local), Const(Const) }
pub enum Terminator {
    Goto(BlockId),
    Branch { cond: Operand, then: BlockId, else_: BlockId },
    Call { func: FnRef, args: Vec<Operand>, dest: Local, next: BlockId, unwind: Option<BlockId> },
    Return, Resume, Panic(String),
}
```

Every type comes from `Checked`; the stage infers nothing.

### What `--emit=mir` prints

Control flow:

```lugha
fun sum_to(n: i64): i64 {
    let mut i = 0;
    let mut s = 0;
    while i < n { s += i; i += 1; }
    s
}
```
```
fn sum_to(_1: i64) -> i64
  let _2: i64  // i
  let _3: i64  // s
  let _4: bool
bb0: _2 = 0; _3 = 0; goto bb1
bb1: _4 = lt _2, _1; branch _4 → bb2, bb3
bb2: _3 = add _3, _2; _2 = add _2, 1; goto bb1      // add panics on overflow
bb3: _0 = _3; return
```

Counting, which today is spread over `rc_sites.rs`, `owning.rs` and the unwind code:

```lugha
fun show(p: Point) { log(p); log(p); }    // Point holds a string
```
```
bb0: retain _1                            // p is used again after this call
     call log(_1) → bb1  unwind bb3
bb1: call log(_1) → bb2  unwind bb4       // last use: moved, no retain
bb2: return
bb3: release _1; resume                   // a panic in the first log
bb4: resume                               // already handed over
```

`defer` on both paths:

```lugha
fun work(c: Console) {
    defer c.println("done");
    risky();
}
```
```
bb0: call risky() → bb1  unwind bb2
bb1: call Console.println(_1, "done") → bb3
bb2: call Console.println(_1, "done") → bb4
bb3: return
bb4: resume
```

### Verifier

After every lowering, in tests always: every block ends in one terminator; every local is assigned before it is read on every path; every block is reachable; later, every counted local is released or moved exactly once on every path, unwind paths included (OSSA's rule).

### Interpreter and the comparison test

`mir::run(bodies, stdout) -> Result<i32, Panic>`: a loop over blocks, locals as values, counted values with a shadow count so a leak or a double release is an error. `tests/mir.rs` runs every `tests/programs/` case that lowers fully and compares stdout and exit code with the compiled program's files:

```
mir: 142 of 610 programs lower; 142 of 142 match the compiled output
```

The interpreter is also a first step toward what replay and constant evaluation would want.

### Layout and order

```
src/mir/  mod.rs (types)  lower.rs (+ one file per construct)  verify.rs  print.rs  interp.rs
tests/mir.rs
```

Each step one small PRP, the stage marked experimental and deletable:

1. Integers, `bool`, `let`, `if`/`while`/`loop`, calls, `return`; printer, verifier, interpreter, comparison test.
2. Structs, enums, `match`, arrays (no counting yet).
3. Counting: `retain`/`release`, moves on last use, the ownership rule in the verifier.
4. Panics, `defer`, unwind edges, `?`.
5. Closures, interfaces, generics after monomorphisation.
6. A retain/release-pair removal pass measured by the interpreter's shadow counts; later stack allocation.

Tasks, channels and actors last, if ever (the interpreter could run them on one thread).

**Costs:** compile and test time (small); drift, since each new language feature needs a lowering or stays `unsupported`, which is fine while nothing depends on it.
