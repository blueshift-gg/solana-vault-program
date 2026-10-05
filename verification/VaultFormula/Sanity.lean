/-
Checks that the statements are not vacuous and that the specification has teeth.

1. Instances: each hypothesis used in `Properties.lean` is satisfiable and the conclusion
   says something there. The numbers are those of the unit tests formula.rs once carried.
2. Necessity: each precondition is needed; without it the conclusion fails.
3. Mutants: plausible wrong formulas, each refuted by a headline property.

Everything is closed by `decide`: kernel computation, no extra axioms.
-/
import VaultFormula.Properties

namespace VaultFormula.Sanity
open VaultFormula.Spec

/-! ## Instances -/

example : toShares 1000 0 0 = 1000 ∧ toAssets 1000 1000 1000 = 1000 := by decide
example : fulfil 100 50 1999 999 = (50, 25) ∧ fulfil 100 500 1999 999 = (200, 100) ∧
    fulfil 100 0 1999 999 = (0, 0) := by decide
/-- `fulfil_full` applies: idle 500 covers the 200 owed. -/
example : toAssets 100 1999 999 ≤ 500 := by decide
/-- `fulfil_partial` and `toSharesCeil_le_of_short` apply: idle 50 does not cover the 200 owed. -/
example : 50 < toAssets 100 1999 999 := by decide
/-- `feeShares_bounds` and `feeShares_worth_gt` apply, and rounding is live: a fee of 10 on
(A, S) = (1000, 999) mints 10 shares worth 9. -/
example : (10 : Nat) ≤ 1000 ∧ feeShares 10 1000 999 = 10 ∧ toAssets 10 1000 (999 + 10) = 9 := by decide
/-- Rounding is live in the round trip: 5 assets at (A, S) = (3, 1) mint 2 shares worth 4. -/
example : toShares 5 3 1 = 2 ∧ toAssets 2 (3 + 5) (1 + 2) = 4 := by decide
/-- `carried_fee_le` is exercised: a deposit doubling 1000 halves a 100 second clock. -/
example : carryElapsed 100 1000 2000 = 50 := by decide
example : recover 70 30 = (40, 0) ∧ recover 30 70 = (0, 40) := by decide

/-- `toShares_mono`, `toAssets_mono` and `managementFee_mono` apply. -/
example : toShares 1 0 0 ≤ toShares 2 0 0 ∧ toAssets 1 0 0 ≤ toAssets 2 0 0 ∧
    managementFee (BPS * YEAR) 1 1 ≤ managementFee (BPS * YEAR) 1 2 := by decide
/-- `ceilDiv_le` applies and is tight: `⌈5/2⌉ = 3`. -/
example : 5 ≤ 3 * 2 ∧ ceilDiv 5 2 = 3 := by decide

/-- `unlocked_bounds` applies and rounds: 7 locked, 2 of 5 seconds in, unlocks 2 (exactly 2.8). -/
example : (2 : Nat) < 5 ∧ unlocked 7 2 5 = 2 := by decide
/-- `unlocked_two_steps` (no hypothesis) is exercised and its slack is reached: 7 locked over 5 seconds, steps of 2
then 1 release 2 + 1 = 3, a single step of 3 releases 4. -/
example : unlocked 7 2 5 + unlocked (7 - unlocked 7 2 5) 1 (5 - 2) = 3 ∧
    unlocked 7 (2 + 1) 5 = 4 := by decide
/-- `blend_between` and `blend_bounds` apply: 300 with 100 seconds left and a gain of 100 with
a 500 second period blend to 200. `blend_of_no_gain` applies. -/
example : (0 : Nat) < 300 + 100 ∧ blend 300 100 100 500 = 200 ∧ blend 300 100 0 500 = 100 := by decide

/-! ## Necessity of the preconditions -/

/-- `unlocked_bounds` needs `elapsed < window`. -/
example : ¬ 7 * 9 < (unlocked 7 9 5 + 1) * 5 := by decide
/-- `unlocked_mono` needs `e ≤ e'`. -/
example : ¬ unlocked 7 3 5 ≤ unlocked 7 2 5 := by decide
/-- `blend_between` and `blend_bounds` need something to lock; `blend_of_no_gain` needs `locked > 0`. -/
example : ¬ blend 0 7 0 3 ≤ max 0 0 ∧ ¬ 0 * 7 + 0 * 3 < (blend 0 7 0 3 + 1) * (0 + 0) ∧ blend 0 7 0 3 ≠ 7 := by decide

/-- The division bounds need a positive divisor. -/
example : ¬ 1 < (1 / 0 + 1) * 0 ∧ ¬ 1 ≤ ceilDiv 1 0 * 0 ∧ ¬ ceilDiv 0 0 * 0 < 0 + 0 := by decide
/-- `ceilDiv_le` needs `n ≤ k · d`. -/
example : ¬ ceilDiv 5 2 ≤ 2 := by decide

/-- `toShares_mono`, `toAssets_mono` need `a ≤ b`. -/
example : ¬ toShares 2 0 0 ≤ toShares 1 0 0 ∧ ¬ toAssets 2 0 0 ≤ toAssets 1 0 0 := by decide
/-- `managementFee_mono` needs `A · e ≤ A' · e'`. -/
example : ¬ managementFee (BPS * YEAR) 1 2 ≤ managementFee (BPS * YEAR) 1 1 := by decide
/-- `feeShares_bounds` and `feeShares_worth_gt` need `fee ≤ A`. -/
example : ¬ 5 * (0 + 1) < (feeShares 5 0 0 + 1) * (0 + 1 - 5) ∧
    ¬ 5 * (0 + feeShares 5 0 0 + 1) < (feeShares 5 0 0 + 1) * (0 + 1) := by decide
/-- `fulfil_full` needs idle to cover the ticket; `fulfil_partial` needs it not to. -/
example : fulfil 100 50 1999 999 ≠ (toAssets 100 1999 999, 100) ∧
    fulfil 100 500 1999 999 ≠ (500, toSharesCeil 500 1999 999) := by decide
/-- `toSharesCeil_le_of_short` needs idle short of the ticket. -/
example : ¬ toSharesCeil 500 1999 999 ≤ 100 := by decide
/-- `carryElapsed_bounds` needs `after ≠ 0`. -/
example : ¬ 7 * min 3 0 < (carryElapsed 7 3 0 + 1) * 0 := by decide

/-! ## Mutants -/

/-- Mutant: `to_shares` rounds up. -/
def toSharesUp (a A S : Nat) : Nat := ceilDiv (a * (S + 1)) (A + 1)
/-- Refuted by `round_trip_le`: 1 asset at (A, S) = (2, 0) mints 1 share, which redeems for 2. -/
example : ¬ toAssets (toSharesUp 1 2 0) (2 + 1) (0 + toSharesUp 1 2 0) ≤ 1 := by decide
/-- Refuted by `deposit_price_ge`: the same deposit lowers the price from 3 to 2. -/
example : ¬ (2 + 1) * (0 + toSharesUp 1 2 0 + 1) ≤ (2 + 1 + 1) * (0 + 1) := by decide

/-- Mutant: `to_assets` without the virtual share. -/
def toAssetsNoVirtualShare (s A S : Nat) : Nat := s * (A + 1) / S
/-- Refuted by `round_trip_le`: the first deposit of 1 into an empty vault redeems for 2. -/
example : ¬ toAssetsNoVirtualShare (toShares 1 0 0) (0 + 1) (0 + toShares 1 0 0) ≤ 1 := by decide

/-- Mutant: `fee_shares` divides by `A + 1` instead of `A + 1 − fee`. -/
def feeSharesNoDilution (fee A S : Nat) : Nat := fee * (S + 1) / (A + 1)
/-- Refuted by `feeShares_worth_gt`: a fee of 50 on (A, S) = (99, 99) mints 50 shares worth 33. -/
example : ¬ 50 * (99 + feeSharesNoDilution 50 99 99 + 1) < (feeSharesNoDilution 50 99 99 + 1) * (99 + 1) ∧
    toAssets (feeSharesNoDilution 50 99 99) 99 (99 + feeSharesNoDilution 50 99 99) = 33 := by decide

/-- Mutant: a partial `fulfil` burns the rounded-down share count. -/
def fulfilBurnFloor (sh idle A S : Nat) : Nat × Nat :=
  if toAssets sh A S ≤ idle then (toAssets sh A S, sh) else (idle, min (toShares idle A S) sh)
/-- Refuted by `fulfil_conserves`: at price 2, idle 1 is paid out and no share is burned. -/
example : ¬ (fulfilBurnFloor 1 1 3 1).1 * (1 + 1) ≤ (fulfilBurnFloor 1 1 3 1).2 * (3 + 1) := by decide

/-- Mutant: `carry_elapsed` without the `min`, on a total that shrank. -/
def carryElapsedNoMin (e b a : Nat) : Nat := if a = 0 then 0 else e * b / a
/-- Refuted by `carryElapsed_le`: the clock would run ahead. -/
example : ¬ carryElapsedNoMin 100 2000 1000 ≤ 100 := by decide

/-- Mutant: `unlocked` without the `window ≤ elapsed` branch. -/
def unlockedNoCap (L e w : Nat) : Nat := L * e / w
/-- Refuted by `unlocked_le`: past the window it releases more than is locked. (In Rust the
same mutant also divides by zero at `window = 0`.) -/
example : ¬ unlockedNoCap 7 9 5 ≤ 7 := by decide

/-- Mutant: `unlocked` rounds up. -/
def unlockedUp (L e w : Nat) : Nat := if w ≤ e then L else ceilDiv (L * e) w
/-- Refuted by the first half of `unlocked_two_steps`: two steps release more than one. -/
example : ¬ unlockedUp 7 1 5 + unlockedUp (7 - unlockedUp 7 1 5) 1 (5 - 1) ≤ unlockedUp 7 (1 + 1) 5 := by decide

/-- Mutant: `blend` weights the wrong way round. -/
def blendSwapped (L t g p : Nat) : Nat := if L + g = 0 then p else (L * p + g * t) / (L + g)
/-- Refuted by `blend_of_no_gain` and `blend_of_no_locked`. -/
example : blendSwapped 300 100 0 500 ≠ 100 ∧ blendSwapped 0 100 100 500 ≠ 500 := by decide

/-- Mutant: `blend` adds the times instead of averaging. -/
def blendSum (L t g p : Nat) : Nat := if L + g = 0 then p else if g = 0 then t else t + p
/-- Refuted by `blend_between`: the window would exceed the longer of the two. -/
example : ¬ blendSum 300 100 100 500 ≤ max 100 500 := by decide

end VaultFormula.Sanity
