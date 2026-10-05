/-
Specification of packages/vault-core/src/math/formula.rs.

Each definition is the mathematical value a Rust function of the same name is meant to
compute, over the natural numbers: no machine widths, no overflow, no `Option`.
`n / d` is the floor of the quotient. `A` is total assets and `S` total shares; every
conversion adds one virtual asset and one virtual share.

`Refinement.lean` proves the Rust computes exactly these values. This file has no proofs.
-/
namespace VaultFormula.Spec

/-- One hundred percent, in basis points. -/
def BPS : Nat := 10000

/-- 365 days, in seconds. -/
def YEAR : Nat := 365 * 24 * 60 * 60

/-- The largest `u64`. A function returning `Option<u64>` returns `Some` exactly when its
specified value is at most this. -/
def U64_MAX : Nat := 2 ^ 64 - 1

/-- `⌈n / d⌉`. -/
def ceilDiv (n d : Nat) : Nat := (n + d - 1) / d

/-- `to_shares`: shares minted for `assets`, `⌊assets · (S + 1) / (A + 1)⌋`. -/
def toShares (assets A S : Nat) : Nat := assets * (S + 1) / (A + 1)

/-- `to_shares_ceil`: shares burned to pay out `assets`, `⌈assets · (S + 1) / (A + 1)⌉`. -/
def toSharesCeil (assets A S : Nat) : Nat := ceilDiv (assets * (S + 1)) (A + 1)

/-- `to_assets`: assets paid for `shares`, `⌊shares · (A + 1) / (S + 1)⌋`. -/
def toAssets (shares A S : Nat) : Nat := shares * (A + 1) / (S + 1)

/-- `unlocked`: how much of `locked` has unlocked `elapsed` seconds into a `window`-second
straight line: all of it once `window ≤ elapsed`, else `⌊locked · elapsed / window⌋`. -/
def unlocked (locked elapsed window : Nat) : Nat :=
  if window ≤ elapsed then locked else locked * elapsed / window

/-- `blend`: the size-weighted average of `timeLeft` (for `locked`) and `period` (for `gain`),
`⌊(locked · timeLeft + gain · period) / (locked + gain)⌋`, and `period` when there is nothing. -/
def blend (locked timeLeft gain period : Nat) : Nat :=
  if locked + gain = 0 then period else (locked * timeLeft + gain * period) / (locked + gain)

/-- `performance_fee`: `⌊gain · bps / BPS⌋`. -/
def performanceFee (gain bps : Nat) : Nat := gain * bps / BPS

/-- `management_fee`: `⌊A · bps · elapsed / (BPS · YEAR)⌋`. -/
def managementFee (A bps elapsed : Nat) : Nat := A * bps * elapsed / (BPS * YEAR)

/-- `recover`: `(gain beyond the loss, loss left)`; what is recovered is `min gain loss`. -/
def recover (gain loss : Nat) : Nat × Nat := (gain - min gain loss, loss - min gain loss)

/-- `carry_elapsed`: `⌊elapsed · min(before, after) / after⌋`, and `0` when `after = 0`. -/
def carryElapsed (elapsed before after : Nat) : Nat :=
  if after = 0 then 0 else elapsed * min before after / after

/-- `fee_shares`: `⌊fee · (S + 1) / (A + 1 − fee)⌋`. Meaningful only for `fee ≤ A`; the Rust
returns `None` otherwise. -/
def feeShares (fee A S : Nat) : Nat := fee * (S + 1) / (A + 1 - fee)

/-- `fulfil`: `(assets paid, shares burned)`. A ticket idle covers is paid in full and burned
whole; otherwise idle is paid out and the shares it is worth, rounded up, are burned. -/
def fulfil (shares idle A S : Nat) : Nat × Nat :=
  if toAssets shares A S ≤ idle then (toAssets shares A S, shares)
  else (idle, min (toSharesCeil idle A S) shares)

end VaultFormula.Spec
