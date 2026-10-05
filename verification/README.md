# Verification of the value math

Machine-checked proofs, in Lean 4, about `packages/vault-core/src/math/formula.rs`.

The Rust is translated to Lean by [Charon](https://github.com/AeneasVerif/charon) and
[Aeneas](https://github.com/AeneasVerif/aeneas). The proofs are about that translation, not about
a hand-written model. There is no `sorry`, no `admit` and no `axiom`.

## What is proved

### 1. Each function computes its specification

`VaultFormula/Spec.lean` gives every function a value over the natural numbers (68 lines, no
proofs). For all inputs, the translated Rust returns exactly that value and does not panic.

| Rust function | Specified value | Theorem |
|---|---|---|
| `to_shares(a, A, S)` | `⌊a·(S+1)/(A+1)⌋` | `to_shares_iff` |
| `to_assets(s, A, S)` | `⌊s·(A+1)/(S+1)⌋` | `to_assets_iff` |
| `unlocked(L, e, w)` | `L` if `w ≤ e`, else `⌊L·e/w⌋` | `unlocked_eq` |
| `blend(L, t, g, p)` | `⌊(L·t + g·p)/(L+g)⌋`, `p` if `L+g = 0` | `blend_iff` (partial: see below) |
| `performance_fee(g, bps)` | `⌊g·bps/BPS⌋` | `performance_fee_iff` |
| `management_fee(A, bps, t)` | `⌊A·bps·t/(BPS·YEAR)⌋` | `management_fee_iff` |
| `recover(g, l)` | `(g − min(g,l), l − min(g,l))` | `recover_eq` |
| `carry_elapsed(t, b, a)` | `⌊t·min(b,a)/a⌋`, `0` if `a = 0` | `carry_elapsed_eq` |
| `fee_shares(f, A, S)` | `⌊f·(S+1)/(A+1−f)⌋`, defined for `f ≤ A` | `fee_shares_iff` |
| `fulfil(sh, idle, A, S)` | `(owed, sh)` if `owed ≤ idle`, else `(idle, ⌈idle·(S+1)/(A+1)⌉)`, with `owed = ⌊sh·(A+1)/(S+1)⌋` | `fulfil_iff` |

For a function returning `Option<u64>`, the theorem is three statements: it returns; it returns
`Some(r)` if and only if the specified value equals `r`; it returns `None` if and only if the
specified value exceeds `u64::MAX`. Two functions have a further `None` case, stated in their
theorems: `fee_shares` also returns `None` when `f > A` (divisor zero or negative), and `fulfil`
returns `None` exactly when `owed` exceeds `u64::MAX`.

For a function returning `u64` (or a pair), the theorem says it returns and the result's value
equals the specified one.

**One function is not total.** `blend` computes `locked·time_left + gain·period` in `u128`
with overflow checks on, and that sum can exceed `u128::MAX`: `blend(u64::MAX, u64::MAX,
u64::MAX, u64::MAX)` panics with "attempt to add with overflow" (replayed on the compiled Rust;
in Lean, the last example of `VaultFormula.lean`). `blend_iff` is the exact statement: it
returns the specified value if and only if the sum is at most `u128::MAX`, and panics if and
only if it is larger. `blend_fits` gives a sufficient condition that every real input meets:
`time_left + period ≤ 2^64`. The comment in the source ("a weighted average of two u64s is at
most the larger, so it fits") is true of the result, and proved (the `as u64` never
truncates), but does not cover the intermediate sum.

`no_panic` collects totality: no other public function panics on any input, `unlocked` with
`window = 0` included. Along the way the proofs
show that no intermediate `u128` product can overflow except where the code checks for it, that
the `as u64` in `carry_elapsed`, `unlocked` and `blend` never truncates, and that the rounded-up burn in `fulfil` always
fits and never exceeds the ticket (the `.min(shares)` there never cuts).

The private helpers `mul_div`, `mul_div_ceil` and `to_shares_ceil` have the same kind of
theorem in `VaultFormula/Refinement.lean`.

### 2. Properties

`VaultFormula/Properties.lean` proves these from the specification alone, for all natural
numbers. `VaultFormula.lean` restates each for the Rust, using part 1. Names below are the
Rust-level theorems in `VaultFormula.lean`; every inequality is an exact integer statement, with
rational bounds cleared of denominators.

| # | Property | Theorem | Precondition |
|---|---|---|---|
| 1 | Round trip: deposit `a` for `sh` shares, redeem at `(A+a, S+sh)`, get `back ≤ a` | `round_trip` | none |
| 2 | `to_shares`, `to_assets` monotonic in the first argument; if the larger input converts, so does the smaller | `to_shares_monotonic`, `to_assets_monotonic` | none |
| 3 | A deposit never lowers the price: `(A+1)·(S+sh+1) ≤ (A+a+1)·(S+1)` | `deposit_never_dilutes` | none |
| 4 | `unlocked ≤ locked`; equals `locked` iff `window ≤ elapsed` or `locked = 0`; monotone in `elapsed`; two steps release no more than one and at most one unit less: `a + b ≤ c ≤ a + b + 1` | `unlocked_bounded`, `unlocked_monotonic`, `unlocked_two_steps` | none |
| 4b | `blend` lies in `[min(t,p), max(t,p)]` when `L+g > 0`; is `p` when `L = 0`; is `t` when `g = 0 < L` | `blend_window` | that `blend` returned |
| 5 | Fees are the exact fee rounded down | `performance_fee_rounding`, `management_fee_rounding` | none |
| 6 | Fee shares, valued at the post-mint supply, are worth at most the fee; `None` exactly when `f > A` or the result overflows | `fee_shares_worth`, `fee_shares_iff` | none |
| 7 | `fulfil`: `paid ≤ idle`, `burned ≤ shares`, `paid·(S+1) ≤ burned·(A+1)` | `fulfil_conserves_value` | none |
| 8 | `carry_elapsed ≤ elapsed`; the fee on the new total over the carried time exists and is at most the fee the old total had accrued, when that one is representable | `carried_fee_never_grows` | none; in particular not `before ≤ after` |
| 9 | `earned + min(g,l) = g`, `left + min(g,l) = l`, not both `earned > 0` and `left > 0` | `recover_splits_the_gain` | none |
| 10 | No function panics, except `blend`, which returns iff `L·t + g·p ≤ u128::MAX` | `no_panic`, `blend_iff` | none |

"None" means no precondition beyond the hypothesis that the calls in question returned `Some`.
Where a statement mentions a later vault state (`A + a`, `S + sh`), that state is a `u64` whose
value is assumed equal to the sum, so the statement covers exactly the cases where the sum fits.

### 3. Rounding: direction and size

"Rounding favours the vault" means the following inequalities. The second column is the most a
user can lose to it.

| Operation | Direction | Bound on the error | Theorem |
|---|---|---|---|
| `to_shares` | `sh·(A+1) ≤ a·(S+1)` | less than one share: `a·(S+1) < (sh+1)·(A+1)` | `to_shares_rounding` |
| `to_assets` | `out·(S+1) ≤ s·(A+1)` | less than one unit: `s·(A+1) < (out+1)·(S+1)` | `to_assets_rounding` |
| round trip | `back ≤ a` | less than one unit plus the price of one share: `a·(S'+1) < (back+1)·(S'+1) + (A'+1)` | `round_trip` |
| fees | `fee·D ≤ N` | less than one unit: `N < (fee+1)·D` | `performance_fee_rounding`, `management_fee_rounding` |
| `fee_shares` | worth `≤ fee` | exact worth short by less than the price of one share: `fee·(S'+1) < (m+1)·(A+1)` | `fee_shares_worth` |
| `fulfil`, full | `paid·(S+1) ≤ sh·(A+1)` | less than one unit: `sh·(A+1) < (paid+1)·(S+1)` | `fulfil_conserves_value` |
| `fulfil`, partial | `idle·(S+1) ≤ burned·(A+1)` | less than one share extra burned: `burned·(A+1) < idle·(S+1) + (A+1)` | `fulfil_conserves_value` |
| `carry_elapsed` | `c·a ≤ t·min(b,a)` | less than one second: `t·min(b,a) < (c+1)·a` | `Spec.carryElapsed_bounds` |
| `unlocked` | `v·w ≤ L·e` | less than one unit: `L·e < (v+1)·w`; over two steps at most one unit | `unlocked_bounded`, `unlocked_two_steps` |
| `blend` | `v·(L+g) ≤ L·t + g·p` | less than one second: `L·t + g·p < (v+1)·(L+g)` | `blend_window` |

`Spec.carryElapsed_bounds` is stated on the specification; with `carry_elapsed_eq` it holds of
the Rust.

### 4. The statements are not vacuous and the specification discriminates

`VaultFormula/Sanity.lean`, all by `decide` (kernel evaluation):

- an instance for every hypothesis used in `Properties.lean`;
- for every precondition there, an input without it on which the conclusion fails;
- nine mutants, each refuted by a headline property:

| Mutant | Counterexample | Property violated |
|---|---|---|
| `to_shares` rounds up | `a=1, A=2, S=0`: mints 1 share, which redeems for 2 | `round_trip_le`; also `deposit_price_ge` (price falls from 3 to 2) |
| `to_assets` without the virtual share | first deposit of 1 into an empty vault redeems for 2 | `round_trip_le` |
| `fee_shares` divides by `A+1`, not `A+1−fee` | `fee=50, A=99, S=99`: mints 50 shares worth 33 | `feeShares_worth_gt` |
| partial `fulfil` burns the rounded-down count | `sh=1, idle=1, A=3, S=1`: pays 1, burns 0 | `fulfil_conserves` |
| `unlocked` without the `window ≤ elapsed` branch | `L=7, e=9, w=5`: releases 12 | `unlocked_le` |
| `unlocked` rounds up | `L=7, w=5`, steps of 1 and 1: releases 4, one step of 2 releases 3 | `unlocked_two_steps` |
| `blend` weights swapped | `L=300, t=100, g=0, p=500`: gives 500, not 100 | `blend_of_no_gain`, `blend_of_no_locked` |
| `blend` adds the times | `L=300, t=100, g=100, p=500`: gives 600 > 500 | `blend_between` |
| `carry_elapsed` without `min` | `t=100, b=2000, a=1000`: carries 200 | `carryElapsed_le` |

The end of `VaultFormula.lean` has instances on the translated Rust itself, including the
`None` cases (`to_shares(u64::MAX, 0, 1)`, `fee_shares(1, 0, 0)`).

## Trust base

A proof here is about the Lean file `VaultFormula/Funs.lean`. That it means something about
the program rests on the following and nothing else.

1. **Charon and Aeneas** translate the Rust faithfully. Not verified here.
2. **`VaultFormula/Std.lean`**: Lean definitions of the six `core` functions formula.rs calls
   that Aeneas does not translate. They are definitions, not axioms; each restates the Rust
   documentation or source in a few lines.

   | Rust | Definition encodes |
   |---|---|
   | `u64::try_from(u128)` | `Ok` of the same value if it is at most `u64::MAX`, else `Err` |
   | `u128::div_ceil` | `core`'s body: `d = x / y; r = x % y; if r > 0 { d + 1 } else { d }`; panics on `y = 0` |
   | `Option::filter` | `None` on `None`; on `Some(t)`, `Some(t)` if the predicate holds, else `None` |
   | `?` on `Option`, test (`Try::branch`) | `Some(v)` continues with `v`, `None` breaks |
   | `?` on `Option`, early return (`FromResidual::from_residual`) | returns `None` |
   | `Result::ok` | `Ok(t)` to `Some(t)`, `Err` to `None` |

   `check.sh` verifies that every function Aeneas reports as untranslated
   (`VaultFormula/FunsExternal_Template.lean`, generated, not compiled) is defined there.
3. **The Aeneas Lean library** for machine integers and the operations Aeneas does model:
   `checked_mul`, `checked_sub`, `min`, `as` casts, `+ - * / %` with overflow
   and division by zero as failures. These are definitions too.
4. **Lean's kernel** and its three standard axioms. `Audit.lean` prints the axioms of all 28
   headline theorems and `check.sh` fails on anything else. Current output, identical for each:

   ```
   'VaultFormula.to_shares_iff' depends on axioms: [propext, Classical.choice, Quot.sound]
   ```
5. **`Spec.lean` says what you want.** It is the file to review.

## What is not proved

- The `Vault` methods in `packages/vault-core/src/state.rs` that compose these formulas
  (`deposit`, `report`, `accrue`, `charge`, `deploy`, `recall`). Nothing here says a sequence of
  state transitions preserves an invariant.
- The instruction handlers in `program/`, account validation, and access control.
- The token programs, the adapter, and the Solana runtime.
- The compiled program. The proofs are about Rust source as Charon reads it (MIR), not SBF
  bytecode, and not about `rustc`.
- Charon and Aeneas themselves, and the six definitions in `Std.lean`.
- Economic adequacy: that these are the right formulas for a vault.

## How to check

Requires [elan](https://github.com/leanprover/elan). From this directory:

```sh
./check.sh
```

It refuses `sorry`/`admit`/`axiom` in the sources, checks the trust base is complete, runs
`lake build`, then runs the axiom audit. Equivalent by hand:
`lake exe cache get && lake build && lake env lean Audit.lean`.

The first run downloads the Aeneas repository and a prebuilt Mathlib (the Aeneas library
depends on it) into `.lake/`, about 7.7 GB on disk, and compiles the Aeneas library. See
"Timings" below. Nothing is installed outside this directory except the Lean toolchain, which
elan places under `~/.elan`.

## How to regenerate the translation

```sh
./regenerate.sh          # fail if VaultFormula/{Types,Funs,FunsExternal_Template}.lean are stale
./regenerate.sh --write  # rewrite them from the current formula.rs
```

Requires Docker. Inside a `nixos/nix` container it runs, at the pinned commits:

```sh
charon cargo --preset=aeneas                                   # in extract/, gives vault_formula.llbc
aeneas vault_formula.llbc -backend lean -split-files -dest .   # gives the three files
```

`extract/` is a crate whose only module is `formula.rs`, included by path. Run the check
mode in CI or before a release: if formula.rs changes in any way that changes the translation,
it fails, and after `--write` the proofs must pass `./check.sh` again. If a change makes Aeneas
report a new untranslated `core` function, `check.sh` fails until it is defined in `Std.lean`.

## Versions

| | |
|---|---|
| Lean | `leanprover/lean4:v4.31.0` (`lean-toolchain`) |
| Aeneas | `557eff83ecef5083b98a52a94ca7fae63d6c1dab` (`lakefile.toml`, `regenerate.sh`) |
| Charon | `c8f15d7d658c86a95658f71ad99cddd4be002e04` (the commit that Aeneas commit pins; `regenerate.sh`) |
| Mathlib | `v4.31.0`, commit `fabf563a7c95` (required by Aeneas) |
| Other Lean packages | exact commits in `lake-manifest.json` |

## Timings

Apple M-series, 14 cores.

| | |
|---|---|
| `./check.sh`, dependencies already built | 5 s |
| `./check.sh`, clean checkout | about 2.5 min (network-bound: about 7 GB of downloads) |
| `./regenerate.sh`, tools cached | 21 s |
| `./regenerate.sh`, first run (builds Charon and Aeneas) | about 25 min |

## Files, in reading order

| File | Role |
|---|---|
| `VaultFormula/Spec.lean` | The specification. Definitions only. |
| `VaultFormula/Types.lean`, `Funs.lean` | Generated by Aeneas from formula.rs. Do not edit. |
| `VaultFormula/FunsExternal_Template.lean` | Generated: the `core` functions Aeneas left untranslated. Not compiled. |
| `VaultFormula/Std.lean` | Trust base: definitions of those functions. `FunsExternal.lean` re-exports it under the name `Funs.lean` imports. |
| `VaultFormula/Refinement.lean` | Translated Rust computes the specification. |
| `VaultFormula/Properties.lean` | Properties of the specification. |
| `VaultFormula/Sanity.lean` | Instances, necessity of preconditions, mutants. |
| `VaultFormula.lean` | Every headline theorem with its full statement. |
| `Audit.lean`, `check.sh` | Axiom audit and the check command. |
| `extract/`, `regenerate.sh` | The extraction crate and the drift check. |
