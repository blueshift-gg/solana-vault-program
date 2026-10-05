/-
What is proved about packages/vault-core/src/math/formula.rs, as full statements.

`formula.f` is the Lean function Aeneas extracted from the Rust `f` (VaultFormula/Funs.lean).
Its result is `ok r` when the Rust returns `r` and anything else when the Rust panics.
`Spec.f` is the mathematical value over ℕ (VaultFormula/Spec.lean). `↑x` is the value of a
machine integer `x`.

Part 1, correctness: each function returns exactly its specified value, for all inputs, and
never panics. Part 2, properties: what follows for the Rust from the properties of the
specification (VaultFormula/Properties.lean), with the rounding error bounded on both sides.

Reading order: Spec, Funs (generated), Std (trust base), Refinement, Properties, this file.
-/
import VaultFormula.Refinement
import VaultFormula.Sanity

open Aeneas Aeneas.Std Result
open vault_formula

namespace VaultFormula
open Spec

/-! # Reading a refinement as iffs -/

theorem iff_of_refines {α β : Type} {g : α → β} (hg : Function.Injective g)
    {m : Result (Option α)} {t : Option β} (h : ∃ r, m = ok r ∧ r.map g = t) :
    (∃ r, m = ok r) ∧ (∀ r, m = ok (some r) ↔ t = some (g r)) ∧ (m = ok none ↔ t = none) := by
  obtain ⟨r, rfl, rfl⟩ := h
  refine ⟨⟨r, rfl⟩, fun v => ?_, ?_⟩ <;> simp only [Result.ok.injEq]
  · cases r with
    | none => simp
    | some w => simp only [Option.map_some, Option.some.injEq]; exact ⟨fun h => by rw [h], fun h => hg h⟩
  · cases r <;> simp

theorem val_injective : Function.Injective (fun x : U64 => x.val) := fun _ _ h => UScalar.eq_of_val_eq h

theorem pair_injective : Function.Injective (fun p : U64 × U64 => (p.1.val, p.2.val)) := by
  rintro ⟨a, b⟩ ⟨c, d⟩ h
  simp only [Prod.mk.injEq] at h
  rw [UScalar.eq_of_val_eq h.1, UScalar.eq_of_val_eq h.2]

theorem fit_eq_some (n : Nat) (r : U64) : fit n = some r.val ↔ n = r.val := by
  have := u64_le_max r
  unfold fit; split <;> first | (simp; done) | (simp; omega)

theorem fit_eq_none (n : Nat) : fit n = none ↔ U64_MAX < n := by
  unfold fit; split <;> simp <;> omega

/-- A function refining `fit n`: it returns, returns `Some r` exactly when `n = r`, and returns
`None` exactly when `n` exceeds `u64::MAX`. -/
theorem iff_of_fit {m : Result (Option U64)} {n : Nat} (h : ∃ r, m = ok r ∧ r.map (·.val) = fit n) :
    (∃ r, m = ok r) ∧ (∀ r : U64, m = ok (some r) ↔ n = r.val) ∧ (m = ok none ↔ U64_MAX < n) := by
  have := iff_of_refines val_injective h
  simpa only [fit_eq_some, fit_eq_none] using this

/-! # Part 1. Correctness of each function

For a function returning `Option<u64>` the three conjuncts are: it does not panic; it returns
`Some r` exactly when the specified value is `r`; it returns `None` exactly when the specified
value does not fit a `u64` (for `fee_shares`: or the divisor is zero). -/

theorem to_shares_iff (assets A S : U64) :
    (∃ r, formula.to_shares assets A S = ok r) ∧
    (∀ r : U64, formula.to_shares assets A S = ok (some r) ↔ toShares ↑assets ↑A ↑S = ↑r) ∧
    (formula.to_shares assets A S = ok none ↔ U64_MAX < toShares ↑assets ↑A ↑S) :=
  iff_of_fit (to_shares_correct assets A S)

theorem to_assets_iff (shares A S : U64) :
    (∃ r, formula.to_assets shares A S = ok r) ∧
    (∀ r : U64, formula.to_assets shares A S = ok (some r) ↔ toAssets ↑shares ↑A ↑S = ↑r) ∧
    (formula.to_assets shares A S = ok none ↔ U64_MAX < toAssets ↑shares ↑A ↑S) :=
  iff_of_fit (to_assets_correct shares A S)

theorem performance_fee_iff (gain : U64) (bps : U16) :
    (∃ r, formula.performance_fee gain bps = ok r) ∧
    (∀ r : U64, formula.performance_fee gain bps = ok (some r) ↔ performanceFee ↑gain ↑bps = ↑r) ∧
    (formula.performance_fee gain bps = ok none ↔ U64_MAX < performanceFee ↑gain ↑bps) :=
  iff_of_fit (performance_fee_correct gain bps)

theorem management_fee_iff (A : U64) (bps : U16) (elapsed : U64) :
    (∃ r, formula.management_fee A bps elapsed = ok r) ∧
    (∀ r : U64, formula.management_fee A bps elapsed = ok (some r) ↔ managementFee ↑A ↑bps ↑elapsed = ↑r) ∧
    (formula.management_fee A bps elapsed = ok none ↔ U64_MAX < managementFee ↑A ↑bps ↑elapsed) :=
  iff_of_fit (management_fee_correct A bps elapsed)

theorem fee_shares_iff (fee A S : U64) :
    (∃ r, formula.fee_shares fee A S = ok r) ∧
    (∀ r : U64, formula.fee_shares fee A S = ok (some r) ↔
      fee.val ≤ A.val ∧ feeShares ↑fee ↑A ↑S = ↑r) ∧
    (formula.fee_shares fee A S = ok none ↔
      A.val < fee.val ∨ U64_MAX < feeShares ↑fee ↑A ↑S) := by
  have := iff_of_refines val_injective (fee_shares_correct fee A S)
  refine ⟨this.1, fun r => (this.2.1 r).trans ?_, this.2.2.trans ?_⟩
  · split <;> simp [fit_eq_some, *]
  · split
    · simp [fit_eq_none]; omega
    · simp; omega

theorem fulfil_iff (shares idle A S : U64) :
    (∃ r, formula.fulfil shares idle A S = ok r) ∧
    (∀ paid burned : U64, formula.fulfil shares idle A S = ok (some (paid, burned)) ↔
      toAssets ↑shares ↑A ↑S ≤ U64_MAX ∧ fulfil ↑shares ↑idle ↑A ↑S = (↑paid, ↑burned)) ∧
    (formula.fulfil shares idle A S = ok none ↔ U64_MAX < toAssets ↑shares ↑A ↑S) := by
  have := iff_of_refines pair_injective (fulfil_correct shares idle A S)
  refine ⟨this.1, fun p b => (this.2.1 (p, b)).trans ?_, this.2.2.trans ?_⟩
  · split <;> simp [*]
  · split <;> first | (simp [*]; done) | (simp [*]; omega)

/-- `unlocked` never panics (also for `window = 0`) and returns `locked` once
`window ≤ elapsed`, else `⌊locked·elapsed/window⌋`. -/
theorem unlocked_eq (locked elapsed window : U64) :
    ∃ v, formula.unlocked locked elapsed window = ok v ∧ v.val = unlocked ↑locked ↑elapsed ↑window :=
  unlocked_correct locked elapsed window

/-- `blend` returns exactly when `locked·time_left + gain·period` fits a `u128`, and then
returns `⌊(locked·time_left + gain·period)/(locked + gain)⌋`, or `period` if `locked + gain = 0`.
Otherwise the `u128` addition overflows and it panics: `blend` is NOT total on `u64`. -/
theorem blend_iff (locked time_left gain period : U64) :
    (∀ v : U64, formula.blend locked time_left gain period = ok v ↔
      locked.val * time_left.val + gain.val * period.val ≤ 2 ^ 128 - 1 ∧
      v.val = blend ↑locked ↑time_left ↑gain ↑period) ∧
    (formula.blend locked time_left gain period = fail .integerOverflow ↔
      2 ^ 128 - 1 < locked.val * time_left.val + gain.val * period.val) := by
  rw [← u128_max_eq]
  by_cases hfit : locked.val * time_left.val + gain.val * period.val ≤ U128.max
  · obtain ⟨w, hw, hv⟩ := blend_correct locked time_left gain period hfit
    refine ⟨fun v => ?_, ?_⟩
    · rw [hw, Result.ok.injEq]
      exact ⟨fun h => ⟨hfit, by rw [← h, hv]⟩, fun h => UScalar.eq_of_val_eq (by rw [hv, h.2])⟩
    · rw [hw]; exact ⟨fun h => absurd h ok_not_fail, fun h => absurd hfit (by omega)⟩
  · have hf := blend_overflow locked time_left gain period (by omega)
    refine ⟨fun v => ?_, ?_⟩
    · rw [hf]; exact ⟨fun h => absurd h fail_not_ok, fun h => absurd h.1 hfit⟩
    · rw [hf]; exact ⟨fun _ => by omega, fun _ => rfl⟩

/-- A sufficient condition for `blend` to return: the two times sum to at most `2^64`. Any
pair of durations below 292 billion years qualifies. -/
theorem blend_fits (locked time_left gain period : U64) (h : time_left.val + period.val ≤ 2 ^ 64) :
    locked.val * time_left.val + gain.val * period.val ≤ 2 ^ 128 - 1 := by
  have h1 : locked.val * time_left.val ≤ U64_MAX * time_left.val := Nat.mul_le_mul_right _ (u64_le_max _)
  have h2 : gain.val * period.val ≤ U64_MAX * period.val := Nat.mul_le_mul_right _ (u64_le_max _)
  have h3 : U64_MAX * (time_left.val + period.val) ≤ U64_MAX * 2 ^ 64 := Nat.mul_le_mul_left _ h
  have h4 : U64_MAX * 2 ^ 64 ≤ 2 ^ 128 - 1 := by decide
  rw [Nat.mul_add] at h3; omega

/-- `recover` never panics and returns `(gain − min(gain, loss), loss − min(gain, loss))`. -/
theorem recover_eq (gain loss : U64) :
    ∃ p, formula.recover gain loss = ok p ∧ (p.1.val, p.2.val) = recover ↑gain ↑loss :=
  recover_correct gain loss

/-- `carry_elapsed` never panics and returns `⌊elapsed·min(before, after)/after⌋`, `0` if `after = 0`. -/
theorem carry_elapsed_eq (elapsed before after : U64) :
    ∃ v, formula.carry_elapsed elapsed before after = ok v ∧
      v.val = carryElapsed ↑elapsed ↑before ↑after :=
  carry_elapsed_correct elapsed before after

/-- Totality: no public function panics, on any input, except `blend`, which returns exactly
when its numerator fits a `u128` (`blend_iff`). -/
theorem no_panic (a b c d : U64) (k : U16) :
    (∃ r, formula.to_shares a b c = ok r) ∧ (∃ r, formula.to_assets a b c = ok r) ∧
    (∃ r, formula.unlocked a b c = ok r) ∧
    ((∃ r, formula.blend a b c d = ok r) ↔ a.val * b.val + c.val * d.val ≤ 2 ^ 128 - 1) ∧
    (∃ r, formula.performance_fee a k = ok r) ∧ (∃ r, formula.management_fee a k b = ok r) ∧
    (∃ r, formula.recover a b = ok r) ∧ (∃ r, formula.carry_elapsed a b c = ok r) ∧
    (∃ r, formula.fee_shares a b c = ok r) ∧ (∃ r, formula.fulfil a b c d = ok r) :=
  ⟨(to_shares_iff a b c).1, (to_assets_iff a b c).1, (unlocked_eq a b c).imp fun _ h => h.1,
    ⟨fun ⟨r, h⟩ => (((blend_iff a b c d).1 r).mp h).1, fun h => by
      rw [← u128_max_eq] at h
      exact (blend_correct a b c d h).imp fun _ h => h.1⟩, (performance_fee_iff a k).1, (management_fee_iff a k b).1,
    (recover_eq a b).imp fun _ h => h.1, (carry_elapsed_eq a b c).imp fun _ h => h.1,
    (fee_shares_iff a b c).1, (fulfil_iff a b c d).1⟩

/-! # Part 2. Properties of the Rust

Each is the corresponding theorem of Properties.lean read through Part 1. -/

/-- Shares minted are the exact amount `assets·(S+1)/(A+1)` rounded down: the depositor is
short by less than one share. -/
theorem to_shares_rounding {assets A S sh : U64} (h : formula.to_shares assets A S = ok (some sh)) :
    sh.val * (A.val + 1) ≤ assets.val * (S.val + 1) ∧
    assets.val * (S.val + 1) < (sh.val + 1) * (A.val + 1) := by
  rw [← ((to_shares_iff assets A S).2.1 sh).mp h]; exact toShares_bounds _ _ _

/-- Assets paid are the exact amount `shares·(A+1)/(S+1)` rounded down: the redeemer is short
by less than one unit. -/
theorem to_assets_rounding {shares A S out : U64} (h : formula.to_assets shares A S = ok (some out)) :
    out.val * (S.val + 1) ≤ shares.val * (A.val + 1) ∧
    shares.val * (A.val + 1) < (out.val + 1) * (S.val + 1) := by
  rw [← ((to_assets_iff shares A S).2.1 out).mp h]; exact toAssets_bounds _ _ _

/-- 1. Round trip never profits. Deposit `assets` at `(A, S)` for `sh` shares, then redeem them
at the state the deposit left, `(A', S') = (A + assets, S + sh)`: what comes back is at most
`assets`, and short of it by less than one unit plus the price of one share, `(A'+1)/(S'+1)`. -/
theorem round_trip {assets A S sh A' S' back : U64}
    (h₁ : formula.to_shares assets A S = ok (some sh))
    (hA : A'.val = A.val + assets.val) (hS : S'.val = S.val + sh.val)
    (h₂ : formula.to_assets sh A' S' = ok (some back)) :
    back.val ≤ assets.val ∧
    assets.val * (S'.val + 1) < (back.val + 1) * (S'.val + 1) + (A'.val + 1) := by
  have e₁ := ((to_shares_iff assets A S).2.1 sh).mp h₁
  have e₂ := ((to_assets_iff sh A' S').2.1 back).mp h₂
  rw [hA, hS, ← e₁] at e₂
  rw [hA, hS, ← e₂, ← e₁]
  exact ⟨round_trip_le _ _ _, round_trip_loss_lt _ _ _⟩

/-- 2. `to_shares` is monotonic: if the larger amount converts, so does the smaller, to no more. -/
theorem to_shares_monotonic {x y A S q : U64} (hxy : x.val ≤ y.val)
    (h : formula.to_shares y A S = ok (some q)) :
    ∃ p, formula.to_shares x A S = ok (some p) ∧ p.val ≤ q.val := by
  have e := ((to_shares_iff y A S).2.1 q).mp h
  have hm := toShares_mono hxy A.val S.val
  obtain ⟨r, hr⟩ := (to_shares_iff x A S).1
  cases r with
  | none => have := (to_shares_iff x A S).2.2.mp hr; have := u64_le_max q; omega
  | some p => exact ⟨p, hr, by rw [← ((to_shares_iff x A S).2.1 p).mp hr]; omega⟩

/-- 2. `to_assets` is monotonic: if the larger amount converts, so does the smaller, to no more. -/
theorem to_assets_monotonic {x y A S q : U64} (hxy : x.val ≤ y.val)
    (h : formula.to_assets y A S = ok (some q)) :
    ∃ p, formula.to_assets x A S = ok (some p) ∧ p.val ≤ q.val := by
  have e := ((to_assets_iff y A S).2.1 q).mp h
  have hm := toAssets_mono hxy A.val S.val
  obtain ⟨r, hr⟩ := (to_assets_iff x A S).1
  cases r with
  | none => have := (to_assets_iff x A S).2.2.mp hr; have := u64_le_max q; omega
  | some p => exact ⟨p, hr, by rw [← ((to_assets_iff x A S).2.1 p).mp hr]; omega⟩

/-- 3. A deposit never lowers the price: `(A+1)/(S+1) ≤ (A+assets+1)/(S+sh+1)`, cross-multiplied. -/
theorem deposit_never_dilutes {assets A S sh : U64} (h : formula.to_shares assets A S = ok (some sh)) :
    (A.val + 1) * (S.val + sh.val + 1) ≤ (A.val + assets.val + 1) * (S.val + 1) := by
  rw [← ((to_shares_iff assets A S).2.1 sh).mp h]; exact deposit_price_ge _ _ _

/-- 4. Unlocking never releases more than is locked; releases all of it exactly when the window
is over or nothing is locked; and inside the window is the exact amount
`locked·elapsed/window` rounded down, short by less than one unit. -/
theorem unlocked_bounded {locked elapsed window v : U64}
    (h : formula.unlocked locked elapsed window = ok v) :
    v.val ≤ locked.val ∧
    (v = locked ↔ window.val ≤ elapsed.val ∨ locked.val = 0) ∧
    (elapsed.val < window.val →
      v.val * window.val ≤ locked.val * elapsed.val ∧
      locked.val * elapsed.val < (v.val + 1) * window.val) := by
  obtain ⟨w, hw, hv⟩ := unlocked_eq locked elapsed window
  rw [h, Result.ok.injEq] at hw; subst hw
  refine ⟨by rw [hv]; exact unlocked_le _ _ _, ?_, fun hlt => by rw [hv]; exact unlocked_bounds _ hlt⟩
  rw [← unlocked_eq_locked_iff, ← hv]
  exact ⟨fun h => by rw [h], fun h => UScalar.eq_of_val_eq h⟩

/-- 4. More elapsed time never unlocks less. -/
theorem unlocked_monotonic {locked e e' window v v' : U64} (hle : e.val ≤ e'.val)
    (h : formula.unlocked locked e window = ok v) (h' : formula.unlocked locked e' window = ok v') :
    v.val ≤ v'.val := by
  obtain ⟨w, hw, hv⟩ := unlocked_eq locked e window
  obtain ⟨w', hw', hv'⟩ := unlocked_eq locked e' window
  rw [h, Result.ok.injEq] at hw; rw [h', Result.ok.injEq] at hw'; subst hw hw'
  rw [hv, hv']; exact unlocked_mono _ hle _

/-- 4. Path independence up to rounding. Unlock `a` after `e₁` seconds, restart the line from
the `locked − a` remaining over the `window − e₁` remaining, and unlock `b` after `e₂` more.
Against `c`, a single step of `e₁ + e₂`: the two steps never release more, and release at
most one unit less. Calling more often cannot unlock faster. The subtractions are on ℕ
(truncating at zero), so the statement also covers steps that run past the window. -/
theorem unlocked_two_steps {locked e₁ e₂ e window rest window' a b c : U64}
    (he : e.val = e₁.val + e₂.val)
    (h₁ : formula.unlocked locked e₁ window = ok a)
    (hrest : rest.val = locked.val - a.val) (hwindow' : window'.val = window.val - e₁.val)
    (h₂ : formula.unlocked rest e₂ window' = ok b)
    (h₃ : formula.unlocked locked e window = ok c) :
    a.val + b.val ≤ c.val ∧ c.val ≤ a.val + b.val + 1 := by
  obtain ⟨_, ha, hav⟩ := unlocked_eq locked e₁ window
  obtain ⟨_, hb, hbv⟩ := unlocked_eq rest e₂ window'
  obtain ⟨_, hc, hcv⟩ := unlocked_eq locked e window
  rw [h₁, Result.ok.injEq] at ha; rw [h₂, Result.ok.injEq] at hb; rw [h₃, Result.ok.injEq] at hc
  subst ha hb hc
  rw [hbv, hrest, hwindow', hcv, he, hav]
  exact Spec.unlocked_two_steps _ _ _ _

/-- 4b. Whenever `blend` returns with something to lock, the new window lies between the
shorter and the longer of `time_left` and `period`; it is the weighted average rounded down,
short by less than one second; with nothing locked before it is `period`; with no gain it is
`time_left`. -/
theorem blend_window {locked time_left gain period v : U64}
    (h : formula.blend locked time_left gain period = ok v) :
    (0 < locked.val + gain.val →
      min time_left.val period.val ≤ v.val ∧ v.val ≤ max time_left.val period.val ∧
      v.val * (locked.val + gain.val) ≤ locked.val * time_left.val + gain.val * period.val ∧
      locked.val * time_left.val + gain.val * period.val < (v.val + 1) * (locked.val + gain.val)) ∧
    (locked.val = 0 → v = period) ∧
    (gain.val = 0 → 0 < locked.val → v = time_left) := by
  obtain ⟨_, hv⟩ := ((blend_iff locked time_left gain period).1 v).mp h
  refine ⟨fun hpos => ?_, fun h0 => UScalar.eq_of_val_eq ?_, fun h0 hpos => UScalar.eq_of_val_eq ?_⟩
  · rw [hv]
    exact ⟨(blend_between _ _ hpos).1, (blend_between _ _ hpos).2, blend_bounds _ _ hpos⟩
  · rw [hv, h0]; exact blend_of_no_locked _ _ _
  · rw [hv, h0]; exact blend_of_no_gain _ _ hpos

/-- 5. The performance fee is the exact fee `gain·bps/BPS` rounded down: never more, and less
by under one unit. -/
theorem performance_fee_rounding {gain : U64} {bps : U16} {fee : U64}
    (h : formula.performance_fee gain bps = ok (some fee)) :
    fee.val * BPS ≤ gain.val * bps.val ∧ gain.val * bps.val < (fee.val + 1) * BPS := by
  rw [← ((performance_fee_iff gain bps).2.1 fee).mp h]; exact performanceFee_bounds _ _

/-- 5. The management fee is the exact fee `A·bps·elapsed/(BPS·YEAR)` rounded down: never more,
and less by under one unit. -/
theorem management_fee_rounding {A : U64} {bps : U16} {elapsed fee : U64}
    (h : formula.management_fee A bps elapsed = ok (some fee)) :
    fee.val * (BPS * YEAR) ≤ A.val * bps.val * elapsed.val ∧
    A.val * bps.val * elapsed.val < (fee.val + 1) * (BPS * YEAR) := by
  rw [← ((management_fee_iff A bps elapsed).2.1 fee).mp h]; exact managementFee_bounds _ _ _

/-- 6. Fee shares, valued after they are minted (supply `S' = S + minted`), are worth at most
the fee, and their exact worth `minted·(A+1)/(S'+1)` falls short of the fee by less than the
price of one share: `fee·(S'+1) < (minted+1)·(A+1)`. -/
theorem fee_shares_worth {fee A S minted S' worth : U64}
    (h₁ : formula.fee_shares fee A S = ok (some minted)) (hS : S'.val = S.val + minted.val)
    (h₂ : formula.to_assets minted A S' = ok (some worth)) :
    worth.val ≤ fee.val ∧ fee.val * (S'.val + 1) < (minted.val + 1) * (A.val + 1) := by
  obtain ⟨hle, e₁⟩ := ((fee_shares_iff fee A S).2.1 minted).mp h₁
  have e₂ := ((to_assets_iff minted A S').2.1 worth).mp h₂
  rw [hS, ← e₁] at e₂
  rw [hS, ← e₂, ← e₁]
  exact ⟨feeShares_worth_le _ _ _, feeShares_worth_gt _ hle⟩

/-- 7. A fulfilment pays at most idle, burns at most the ticket, and pays no more than the
burned shares are worth. Either the whole ticket is burned and paid within one unit of its
worth, or all of idle is paid and less than one share more than its exact worth is burned. -/
theorem fulfil_conserves_value {shares idle A S paid burned : U64}
    (h : formula.fulfil shares idle A S = ok (some (paid, burned))) :
    paid.val ≤ idle.val ∧ burned.val ≤ shares.val ∧
    paid.val * (S.val + 1) ≤ burned.val * (A.val + 1) ∧
    ((burned = shares ∧ shares.val * (A.val + 1) < (paid.val + 1) * (S.val + 1)) ∨
     (paid = idle ∧ burned.val * (A.val + 1) < idle.val * (S.val + 1) + (A.val + 1))) := by
  obtain ⟨_, e⟩ := ((fulfil_iff shares idle A S).2.1 paid burned).mp h
  have hc := Spec.fulfil_conserves shares.val idle.val A.val S.val
  rw [e] at hc
  refine ⟨hc.1, hc.2.1, hc.2.2, ?_⟩
  by_cases hfull : toAssets shares.val A.val S.val ≤ idle.val
  · have hf := fulfil_full hfull
    rw [e] at hf
    exact .inl ⟨UScalar.eq_of_val_eq (congrArg Prod.snd hf.1), hf.2⟩
  · have hp := fulfil_partial (Nat.lt_of_not_le hfull)
    rw [e] at hp
    exact .inr ⟨UScalar.eq_of_val_eq (congrArg Prod.fst hp.1), hp.2⟩

/-- 8. The carried fee clock is never longer than the elapsed time, and the management fee
accruing on the new total over the carried time exists and is at most the fee the old total
had accrued, whenever that one is representable. No assumption relates `before` and `after`. -/
theorem carried_fee_never_grows {elapsed before after carried : U64} {bps : U16} {fee : U64}
    (h : formula.carry_elapsed elapsed before after = ok carried) :
    carried.val ≤ elapsed.val ∧
    (formula.management_fee before bps elapsed = ok (some fee) →
      ∃ fee', formula.management_fee after bps carried = ok (some fee') ∧ fee'.val ≤ fee.val) := by
  obtain ⟨w, hw, hv⟩ := carry_elapsed_eq elapsed before after
  rw [h, Result.ok.injEq] at hw; subst hw
  refine ⟨by rw [hv]; exact carryElapsed_le _ _ _, fun hf => ?_⟩
  have e := ((management_fee_iff before bps elapsed).2.1 fee).mp hf
  have hle := carried_fee_le elapsed.val before.val after.val bps.val
  rw [← hv] at hle
  obtain ⟨r, hr⟩ := (management_fee_iff after bps carried).1
  cases r with
  | none => have := (management_fee_iff after bps carried).2.2.mp hr; have := u64_le_max fee; omega
  | some f' => exact ⟨f', hr, by rw [← ((management_fee_iff after bps carried).2.1 f').mp hr]; omega⟩

/-- 9. A gain is split, never created: earned plus recovered is the gain, where recovered is
`min(gain, loss)` and comes off the loss; and nothing is earned while a loss is left. -/
theorem recover_splits_the_gain {gain loss earned left : U64}
    (h : formula.recover gain loss = ok (earned, left)) :
    earned.val + min gain.val loss.val = gain.val ∧ left.val + min gain.val loss.val = loss.val ∧
    ¬ (0 < earned.val ∧ 0 < left.val) := by
  obtain ⟨p, hp, hv⟩ := recover_eq gain loss
  rw [h, Result.ok.injEq] at hp; subst hp
  have := Spec.recover_splits gain.val loss.val
  rw [← hv] at this; exact this

/-! # Witnesses

The hypotheses of Part 2 are satisfiable on the Rust itself, and the `None` cases occur. -/

example : formula.to_shares 5#u64 3#u64 1#u64 = ok (some 2#u64) :=
  ((to_shares_iff _ _ _).2.1 _).mpr (by decide)
example : formula.to_assets 2#u64 8#u64 3#u64 = ok (some 4#u64) :=
  ((to_assets_iff _ _ _).2.1 _).mpr (by decide)
example : formula.to_shares 18446744073709551615#u64 0#u64 1#u64 = ok none :=
  (to_shares_iff _ _ _).2.2.mpr (by decide)
example : formula.performance_fee 1000#u64 2000#u16 = ok (some 200#u64) :=
  ((performance_fee_iff _ _).2.1 _).mpr (by decide)
example : formula.management_fee 1000000#u64 200#u16 31536000#u64 = ok (some 20000#u64) :=
  ((management_fee_iff _ _ _).2.1 _).mpr (by decide)
example : formula.fee_shares 10#u64 1000#u64 999#u64 = ok (some 10#u64) :=
  ((fee_shares_iff _ _ _).2.1 _).mpr (by decide)
/-- A fee of everything, virtual asset included: the divisor is zero and the result is `None`. -/
example : formula.fee_shares 1#u64 0#u64 0#u64 = ok none :=
  (fee_shares_iff _ _ _).2.2.mpr (by decide)
example : formula.fulfil 100#u64 50#u64 1999#u64 999#u64 = ok (some (50#u64, 25#u64)) :=
  ((fulfil_iff _ _ _ _).2.1 _ _).mpr (by decide)
example : formula.fulfil 100#u64 500#u64 1999#u64 999#u64 = ok (some (200#u64, 100#u64)) :=
  ((fulfil_iff _ _ _ _).2.1 _ _).mpr (by decide)

example : formula.unlocked 7#u64 2#u64 5#u64 = ok 2#u64 := by
  obtain ⟨v, h, hv⟩ := unlocked_eq 7#u64 2#u64 5#u64
  rw [h]; congr; exact UScalar.eq_of_val_eq (by rw [hv]; decide)
example : formula.blend 300#u64 100#u64 100#u64 500#u64 = ok 200#u64 :=
  ((blend_iff _ _ _ _).1 _).mpr (by decide)
/-- The panic: all four inputs at `u64::MAX`. Replayed on the compiled Rust, it panics with
"attempt to add with overflow". -/
example : formula.blend 18446744073709551615#u64 18446744073709551615#u64
    18446744073709551615#u64 18446744073709551615#u64 = fail .integerOverflow :=
  (blend_iff _ _ _ _).2.mpr (by decide)

end VaultFormula
