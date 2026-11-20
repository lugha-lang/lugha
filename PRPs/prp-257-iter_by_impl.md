## FEATURE: `for … of`, the iterator helpers and `Iter<T>` values reach a type's iterator only through `impl Iter<T> for X`, never by a method named `next`; milestone 225.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
struct Count { n: i64 }

impl Iter<i64> for Count {
    fun next(mut self): Option<i64> {
        self.n += 1;
        if self.n <= 3 { Some(self.n) } else { None }
    }
}

fun main(console: Console) {
    for x of Count { n: 0 } { console.println(x); }            // 1 2 3
    let evens = Count { n: 0 }.filter(fun(x: i64): bool = x % 2 == 0).collect();
    let it: Iter<i64> = Count { n: 0 };                         // an interface value
}
```

```
struct Ticks { n: i64 }
impl Ticks { fun next(mut self): Option<i64> = None; }

for t of Ticks { n: 0 } { }
```
```
error[E0411]: `Ticks` can't be looped over
  --> ticks.la:4:10
   |
 4 | for t of Ticks { n: 0 } { }
   |          ^^^^^^^^^^^^^^
   = help: `Ticks` has a method `next`, but a loop needs `impl Iter<T> for Ticks { fun next(mut self): Option<T> … }`
```

## CONTEXT
- **Ledger:** INT-03 (built-in behaviours through implementations, never method names, `for … of` (`Iter`) included); INT-18 (interface values convert along `extends`, with `Iter` the named exception until now); INT-21 (`Iter` reserved).
- **Spec:**
  - §4 "Iterators" (structural today: "anything with that `next`");
  - §5 "for over an iterator";
  - §4's "Still by name" list;
  - §10 "Iterators" (`Count`, `Fib` passed as `Iter<i64>` with no `impl`, a finding of the spec review).
- **Code:**
  - `check/iterators.rs` `next_of`/`iterator_element`, which accept any `offered(ty, "next")`;
  - `check/iter_helpers.rs` (the `.map`/`.filter`/… helpers on any iterator);
  - `check/impls.rs` `by_block`/`nominal`, which leave `Iter` out;
  - `check/upcast.rs` (`Iter` converting by items);
  - `codegen/iterators.rs` (which `next` a loop calls);
  - the prelude's `iter.la` (`ArrayIter`, `Mapped`, `Filter`, …, each with its own `next`) and `more.la`.
- **Decisions:** none open.

## DESIGN
1. **`Iter` is nominal.**
   - It joins `by_block` and `nominal`: a type meets it only through `impl Iter<T> for X { fun next(mut self): Option<T> … }`.
   - A type's own `next` doesn't count.
   - An interface value converts to `Iter<T>` only from an interface extending it (INT-18 with no exception left).
2. **What a loop accepts** (`for x of e`, and the helpers):
   - an array, a string or a view (unchanged);
   - a channel's receiving end (unchanged);
   - a type with a block `impl Iter<T> for X`, whose `next` the loop calls;
   - an `Iter<T>` value, or one of an interface that extends it;
   - a type parameter bounded by `Iter<T>` or by an interface extending it.

   Anything else is E0411. When the type has a `next` of the right shape but no block, the help names the missing `impl`.
3. **Calls.**
   - `x.next()` on such a type reaches the block's method, as any interface item does (a type's own `next` would win, INT-15).
   - The loop and the helpers always call the block's, so a type with both loops through its `impl`.
4. **The prelude.** `ArrayIter`, `Mapped`, `Filter`, `Take`, `Skip`, `Enumerate`, `Zip`, `Chain`, `TakeWhile`, `SkipWhile`, `FlatMap` and the rest move their `next` into `impl<…> Iter<T> for …` blocks. Std's iterators, if any, do the same.
5. **Migration:** every iterator in the tests, the error pages and the spec (§10 "Iterators" included) gets its `impl Iter<T> for …` line.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **No new code:** E0411 ("can't be looped over") already says it; its help gains the `impl` line when a `next` of the right shape exists, the case people will meet first.
2. **A type with both its own `next` and an `impl Iter`:** `x.next()` calls its own (INT-15), and the loop calls the block's. That is the existing rule for any method an interface also supplies, and the loop's behaviour stays visible at the block.
3. **No derive and no default `next`:** an iterator's `next` is always written.

## REQUIREMENTS
### Must Do
- Design 1–5; E0411's help and page.
- Spec §4 "Iterators" and the "Still by name" list (now empty of `Iter`), §5 "for over an iterator", §10 "Iterators", §11 row 225, "Changes since v17" (**Breaking**), and the ledger note that INT-03 is complete.
- CHANGELOG.
### Must NOT Do
- Change arrays', strings', views' or channels' loops.
- Change `for … of mut`'s rules, or the helpers' names and results.

## TESTS
- [ ] `--lib`:
  - [ ] a loop over a type with a block (concrete, generic, through a bound, as an interface value);
  - [ ] E0411 for a type with only its own `next`, with the help;
  - [ ] the helpers on a block-only type;
  - [ ] own `next` beside a block (the call and the loop reach different methods);
  - [ ] no conversion to `Iter<T>` without a block.
- [ ] `tests/programs/iterators/by_impl/`: a stdout case and an E0411 case.
- [ ] Targeted: `--lib`, `programs`, `spec`, `explain`, `comments`, and every `tests/*.rs` whose inline programs define an iterator (found by grep).

## ACCEPTANCE
- [ ] No type becomes loopable by having a method named `next`.
- [ ] Every built-in behaviour is reached through an `impl`.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
