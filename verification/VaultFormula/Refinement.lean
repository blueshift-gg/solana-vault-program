/-
Refinement: every function Aeneas extracted from formula.rs (Funs.lean) computes the value
`Spec.lean` gives it, for all inputs, and never panics.

Statements have the form `∃ r, f args = ok r ∧ <r is the specified value>`. `ok` means the
Rust returned without panicking; for `Option<u64>` results, `r.map val = fit n` says the
function returns `Some` of the specified value `n` exactly when `n` fits a `u64`, else `None`.
`VaultFormula.lean` restates these as iffs.
-/
import VaultFormula.Funs
import VaultFormula.Properties
open Aeneas Aeneas.Std Result

namespace VaultFormula
open Spec vault_formula

/-- The `Option<u64>` a specified value becomes: `Some` exactly when it fits. -/
def fit (n : Nat) : Option Nat := if n ≤ U64_MAX then some n else none

theorem u64_le_max (x : U64) : x.val ≤ U64_MAX := by unfold U64_MAX; scalar_tac
theorem u16_lt (x : U16) : x.val < 2 ^ 16 := by scalar_tac
theorem u128_max_eq : U128.max = 2 ^ 128 - 1 := by simp [U128.max_eq]

/-- Reading a correctness statement in `fit` form as the two iffs of the headline theorems. -/
theorem fit_iff {m : Result (Option U64)} {n : Nat}
    (h : ∃ r, m = ok r ∧ r.map (·.val) = fit n) :
    (∀ r : U64, m = ok (some r) ↔ n = r.val) ∧ (m = ok none ↔ U64_MAX < n) := by
  obtain ⟨r, rfl, hr⟩ := h
  unfold fit at hr
  constructor
  · intro v
    simp only [Result.ok.injEq]
    constructor
    · rintro rfl; split at hr <;> simp at hr; exact hr.symm
    · intro hn
      have := u64_le_max v
      rw [if_pos (by omega)] at hr
      cases r with
      | none => simp at hr
      | some w => simp at hr; congr; exact UScalar.eq_of_val_eq (by omega)
  · simp only [Result.ok.injEq]
    constructor
    · rintro rfl; split at hr <;> simp at hr; omega
    · intro hn; rw [if_neg (by omega)] at hr; cases r <;> simp_all

theorem cast_u64 (x : U64) : (UScalar.cast .U128 x).val = x.val := by
  simp [UScalar.cast_val_eq]; scalar_tac

theorem cast_u16 (x : U16) : (UScalar.cast .U128 x).val = x.val := by
  simp [UScalar.cast_val_eq]; scalar_tac

theorem succ_u128 (x : U64) : ∃ i, UScalar.cast .U128 x + 1#u128 = ok i ∧ i.val = x.val + 1 := by
  obtain ⟨i, hi, hv⟩ := WP.spec_imp_exists (UScalar.add_spec (x := UScalar.cast .U128 x) (y := 1#u128)
    (by have := cast_u64 x; scalar_tac))
  exact ⟨i, hi, by rw [hv, cast_u64]; rfl⟩

theorem mul_le_u128 {x y : Nat} (hx : x ≤ U64_MAX) (hy : y ≤ U64_MAX + 1) : x * y ≤ U128.max := by
  have h : x * y ≤ U64_MAX * (U64_MAX + 1) := Nat.mul_le_mul hx hy
  have : U64_MAX * (U64_MAX + 1) ≤ 2 ^ 128 - 1 := by decide
  rw [u128_max_eq]; omega

/-- A product too wide for a `u128`, divided by `BPS · YEAR`, is still too wide for a `u64`. -/
theorem wide_quotient {n : Nat} (h : U128.max < n) : U64_MAX < n / (BPS * YEAR) := by
  have h1 : 2 ^ 128 / (BPS * YEAR) ≤ n / (BPS * YEAR) := Nat.div_le_div_right (by rw [u128_max_eq] at h; omega)
  have : U64_MAX < 2 ^ 128 / (BPS * YEAR) := by decide
  omega

theorem ceilDiv_eq (n : Nat) {d : Nat} (hd : 0 < d) :
    ceilDiv n d = if n % d = 0 then n / d else n / d + 1 := by
  have h1 := Nat.div_add_mod n d
  have h2 := Nat.mod_lt n hd
  unfold ceilDiv
  split
  · apply Nat.div_eq_of_lt_le
    · rw [Nat.mul_comm]; omega
    · rw [Nat.add_mul, Nat.mul_comm]; omega
  · apply Nat.div_eq_of_lt_le
    · rw [Nat.add_mul, Nat.mul_comm]; omega
    · rw [Nat.add_mul, Nat.add_mul, Nat.mul_comm]; omega

/-! ## Constants -/

theorem bps_val : formula.BPS.val = BPS := by unfold formula.BPS; rfl

theorem year_ok : ∃ y, formula.YEAR = ok y ∧ y.val = YEAR := by
  unfold formula.YEAR
  obtain ⟨i, hi, hiv⟩ := WP.spec_imp_exists (UScalar.mul_spec (x := 365#u128) (y := 24#u128) (by scalar_tac))
  obtain ⟨i1, hi1, hi1v⟩ := WP.spec_imp_exists (UScalar.mul_spec (x := i) (y := 60#u128) (by scalar_tac))
  obtain ⟨i2, hi2, hi2v⟩ := WP.spec_imp_exists (UScalar.mul_spec (x := i1) (y := 60#u128) (by scalar_tac))
  refine ⟨i2, by simp [hi, hi1, hi2], ?_⟩
  rw [hi2v, hi1v, hiv]; rfl

theorem bps_year_ok {y : U128} (hy : y.val = YEAR) : ∃ d, formula.BPS * y = ok d ∧ d.val = BPS * YEAR := by
  obtain ⟨d, hd, hdv⟩ := WP.spec_imp_exists (UScalar.mul_spec (x := formula.BPS) (y := y)
    (by rw [bps_val, hy, UScalar.max_UScalarTy_U128_eq, u128_max_eq]; decide))
  exact ⟨d, hd, by rw [hdv, bps_val, hy]⟩

/-! ## The two helpers -/

theorem try_from_ok_spec (q : U128) :
    ∃ t, U64.Insts.CoreConvertTryFromU128TryFromIntError.try_from q = ok t ∧
      ∃ r, core.result.Result.ok t = ok r ∧ r.map (·.val) = fit q.val := by
  unfold U64.Insts.CoreConvertTryFromU128TryFromIntError.try_from fit
  have hmax : U64.max = U64_MAX := by simp [U64.max_eq, U64_MAX]
  rw [hmax]
  split
  · rename_i h
    refine ⟨_, rfl, some (UScalar.cast .U64 q), by simp [core.result.Result.ok], ?_⟩
    simp [UScalar.cast_val_eq]
    unfold U64_MAX at h; omega
  · exact ⟨_, rfl, none, by simp [core.result.Result.ok], rfl⟩

/-- `mul_div a b d`: `⌊a·b/d⌋` when `a·b` fits a `u128` and the quotient a `u64`. -/
theorem mul_div_spec (a : U64) (b d : U128) (hd : d.val ≠ 0) :
    ∃ r, formula.mul_div a b d = ok r ∧
      r.map (·.val) = if a.val * b.val ≤ U128.max then fit (a.val * b.val / d.val) else none := by
  unfold formula.mul_div
  have hcast := cast_u64 a
  have hmul := U128.checked_mul_bv_spec (UScalar.cast .U128 a) b
  cases hm : U128.checked_mul (UScalar.cast .U128 a) b with
  | none =>
    rw [hm] at hmul
    have : ¬ a.val * b.val ≤ U128.max := by rw [hcast] at hmul; omega
    exact ⟨none, by simp [lift, hm, core.option.Option.Insts.CoreOpsTry_traitTry.branch,
      core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual], by simp [this]⟩
  | some p =>
    rw [hm] at hmul
    obtain ⟨hle, hp, _⟩ := hmul
    rw [hcast] at hle hp
    obtain ⟨q, hq, hqv⟩ := UScalar.div_spec p hd
    obtain ⟨t, ht, r, hr, hrv⟩ := try_from_ok_spec q
    exact ⟨r, by simp [lift, hm, core.option.Option.Insts.CoreOpsTry_traitTry.branch, hq, ht, hr],
      by rw [hrv, hqv, hp, if_pos hle]⟩

theorem div_ceil_spec (x : U128) {y : U128} (hy : y.val ≠ 0) :
    ∃ z, core.num.U128.div_ceil x y = ok z ∧ z.val = ceilDiv x.val y.val := by
  unfold core.num.U128.div_ceil
  obtain ⟨d, hd, hdv⟩ := UScalar.div_spec x hy
  obtain ⟨r, hr, hrv⟩ := WP.spec_imp_exists (UScalar.rem_spec x hy)
  rw [ceilDiv_eq _ (by omega)]
  by_cases h0 : x.val % y.val = 0
  · refine ⟨d, ?_, by rw [hdv, if_pos h0]⟩
    simp [hd, hr, hrv, h0]
  · have hlt : x.val / y.val < x.val := by
      apply Nat.div_lt_self
      · have := Nat.mod_le x.val y.val; omega
      · by_contra hc
        have : y.val = 1 := by omega
        rw [this, Nat.mod_one] at h0; exact h0 rfl
    obtain ⟨z, hz, hzv⟩ := WP.spec_imp_exists (UScalar.add_spec (x := d) (y := 1#u128) (by scalar_tac))
    refine ⟨z, ?_, by rw [hzv, hdv, if_neg h0]; rfl⟩
    simp [hd, hr, hrv, h0, hz]

/-- `mul_div_ceil a b d`: `⌈a·b/d⌉` when `a·b` fits a `u128` and the quotient a `u64`. -/
theorem mul_div_ceil_spec (a : U64) (b d : U128) (hd : d.val ≠ 0) :
    ∃ r, formula.mul_div_ceil a b d = ok r ∧
      r.map (·.val) = if a.val * b.val ≤ U128.max then fit (ceilDiv (a.val * b.val) d.val) else none := by
  unfold formula.mul_div_ceil
  have hcast := cast_u64 a
  have hmul := U128.checked_mul_bv_spec (UScalar.cast .U128 a) b
  cases hm : U128.checked_mul (UScalar.cast .U128 a) b with
  | none =>
    rw [hm] at hmul
    have : ¬ a.val * b.val ≤ U128.max := by rw [hcast] at hmul; omega
    exact ⟨none, by simp [lift, hm, core.option.Option.Insts.CoreOpsTry_traitTry.branch,
      core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual], by simp [this]⟩
  | some p =>
    rw [hm] at hmul
    obtain ⟨hle, hp, _⟩ := hmul
    rw [hcast] at hle hp
    obtain ⟨q, hq, hqv⟩ := div_ceil_spec p hd
    obtain ⟨t, ht, r, hr, hrv⟩ := try_from_ok_spec q
    exact ⟨r, by simp [lift, hm, core.option.Option.Insts.CoreOpsTry_traitTry.branch, hq, ht, hr],
      by rw [hrv, hqv, hp, if_pos hle]⟩

/-! ## Conversions -/

/-- `to_shares` returns `Some ⌊assets·(S+1)/(A+1)⌋` when that fits a `u64`, else `None`. -/
theorem to_shares_correct (a A S : U64) :
    ∃ r, formula.to_shares a A S = ok r ∧ r.map (·.val) = fit (toShares a.val A.val S.val) := by
  unfold formula.to_shares toShares
  obtain ⟨i1, h1, h1v⟩ := succ_u128 S
  obtain ⟨i3, h3, h3v⟩ := succ_u128 A
  obtain ⟨r, hr, hrv⟩ := mul_div_spec a i1 i3 (by omega)
  have := mul_le_u128 (u64_le_max a) (show S.val + 1 ≤ U64_MAX + 1 by have := u64_le_max S; omega)
  refine ⟨r, by simp [lift, h1, h3, hr], by rw [hrv, h1v, h3v, if_pos this]⟩

theorem to_shares_ceil_correct (a A S : U64) :
    ∃ r, formula.to_shares_ceil a A S = ok r ∧ r.map (·.val) = fit (toSharesCeil a.val A.val S.val) := by
  unfold formula.to_shares_ceil toSharesCeil
  obtain ⟨i1, h1, h1v⟩ := succ_u128 S
  obtain ⟨i3, h3, h3v⟩ := succ_u128 A
  obtain ⟨r, hr, hrv⟩ := mul_div_ceil_spec a i1 i3 (by omega)
  have := mul_le_u128 (u64_le_max a) (show S.val + 1 ≤ U64_MAX + 1 by have := u64_le_max S; omega)
  refine ⟨r, by simp [lift, h1, h3, hr], by rw [hrv, h1v, h3v, if_pos this]⟩

/-- `to_assets` returns `Some ⌊shares·(A+1)/(S+1)⌋` when that fits a `u64`, else `None`. -/
theorem to_assets_correct (s A S : U64) :
    ∃ r, formula.to_assets s A S = ok r ∧ r.map (·.val) = fit (toAssets s.val A.val S.val) := by
  unfold formula.to_assets toAssets
  obtain ⟨i1, h1, h1v⟩ := succ_u128 A
  obtain ⟨i3, h3, h3v⟩ := succ_u128 S
  obtain ⟨r, hr, hrv⟩ := mul_div_spec s i1 i3 (by omega)
  have := mul_le_u128 (u64_le_max s) (show A.val + 1 ≤ U64_MAX + 1 by have := u64_le_max A; omega)
  refine ⟨r, by simp [lift, h1, h3, hr], by rw [hrv, h1v, h3v, if_pos this]⟩

/-! ## Fees -/

/-- `performance_fee` returns `Some ⌊gain·bps/BPS⌋` when that fits a `u64`, else `None`. -/
theorem performance_fee_correct (g : U64) (bps : U16) :
    ∃ r, formula.performance_fee g bps = ok r ∧ r.map (·.val) = fit (performanceFee g.val bps.val) := by
  unfold formula.performance_fee performanceFee
  obtain ⟨r, hr, hrv⟩ := mul_div_spec g (UScalar.cast .U128 bps) formula.BPS (by rw [bps_val]; decide)
  have := mul_le_u128 (u64_le_max g) (show bps.val ≤ U64_MAX + 1 by have := u16_lt bps; unfold U64_MAX; omega)
  refine ⟨r, by simp [lift, hr], by rw [hrv, cast_u16, bps_val, if_pos this]⟩

/-- `management_fee` returns `Some ⌊A·bps·elapsed/(BPS·YEAR)⌋` when that fits a `u64`, else `None`. -/
theorem management_fee_correct (A : U64) (bps : U16) (e : U64) :
    ∃ r, formula.management_fee A bps e = ok r ∧
      r.map (·.val) = fit (managementFee A.val bps.val e.val) := by
  unfold formula.management_fee managementFee
  have hmul := U128.checked_mul_bv_spec (UScalar.cast .U128 bps) (UScalar.cast .U128 e)
  rw [cast_u16, cast_u64] at hmul
  have hfit : bps.val * e.val ≤ U128.max := by
    rw [Nat.mul_comm]
    exact mul_le_u128 (u64_le_max e) (by have := u16_lt bps; unfold U64_MAX; omega)
  cases hm : U128.checked_mul (UScalar.cast .U128 bps) (UScalar.cast .U128 e) with
  | none => rw [hm] at hmul; omega
  | some p =>
    rw [hm] at hmul
    obtain ⟨_, hp, _⟩ := hmul
    obtain ⟨y, hy, hyv⟩ := year_ok
    obtain ⟨d, hd, hdv⟩ := bps_year_ok hyv
    obtain ⟨r, hr, hrv⟩ := mul_div_spec A p d (by rw [hdv]; decide)
    refine ⟨r, by simp [lift, hm, core.option.Option.Insts.CoreOpsTry_traitTry.branch, hy, hd, hr], ?_⟩
    rw [hrv, hp, hdv, Nat.mul_assoc]
    split
    · rfl
    · rename_i h
      have := wide_quotient (Nat.lt_of_not_le h)
      unfold fit; rw [if_neg (by omega)]

/-! ## Unlocking -/

/-- `unlocked` returns `locked` once `window ≤ elapsed`, else `⌊locked·elapsed/window⌋`.
It cannot panic (`window = 0` takes the first branch) and the final `as u64` never truncates. -/
theorem unlocked_correct (L e w : U64) :
    ∃ v, formula.unlocked L e w = ok v ∧ v.val = unlocked L.val e.val w.val := by
  unfold formula.unlocked
  by_cases hw : w.val ≤ e.val
  · exact ⟨L, by simp [hw], by unfold Spec.unlocked; rw [if_pos hw]⟩
  · obtain ⟨i2, h2, h2v⟩ := WP.spec_imp_exists (UScalar.mul_spec (x := UScalar.cast .U128 L)
      (y := UScalar.cast .U128 e) (by
        rw [cast_u64, cast_u64, UScalar.max_UScalarTy_U128_eq]
        exact mul_le_u128 (u64_le_max L) (by have := u64_le_max e; omega)))
    rw [cast_u64, cast_u64] at h2v
    obtain ⟨i4, h4, h4v⟩ := UScalar.div_spec i2 (y := UScalar.cast .U128 w) (by rw [cast_u64]; omega)
    rw [cast_u64, h2v] at h4v
    refine ⟨UScalar.cast .U64 i4, by simp [hw, lift, h2, h4], ?_⟩
    have hle := unlocked_le L.val e.val w.val
    unfold Spec.unlocked at hle ⊢
    rw [if_neg hw] at hle ⊢
    rw [UScalar.cast_val_eq, h4v]
    apply Nat.mod_eq_of_lt
    have := u64_le_max L
    unfold U64_MAX at this; simp; omega

/-! ## Blending the unlock window -/

theorem blend_total (L g : U64) :
    ∃ tot, UScalar.cast .U128 L + UScalar.cast .U128 g = ok tot ∧ tot.val = L.val + g.val := by
  obtain ⟨tot, h, hv⟩ := WP.spec_imp_exists (UScalar.add_spec (x := UScalar.cast .U128 L)
    (y := UScalar.cast .U128 g) (by
      rw [cast_u64, cast_u64, UScalar.max_UScalarTy_U128_eq, u128_max_eq]
      have := u64_le_max L; have := u64_le_max g; unfold U64_MAX at *; omega))
  exact ⟨tot, h, by rw [hv, cast_u64, cast_u64]⟩

theorem blend_products (L t g p : U64) :
    ∃ i4 i7, UScalar.cast .U128 L * UScalar.cast .U128 t = ok i4 ∧ i4.val = L.val * t.val ∧
      UScalar.cast .U128 g * UScalar.cast .U128 p = ok i7 ∧ i7.val = g.val * p.val := by
  obtain ⟨i4, h4, h4v⟩ := WP.spec_imp_exists (UScalar.mul_spec (x := UScalar.cast .U128 L)
    (y := UScalar.cast .U128 t) (by
      rw [cast_u64, cast_u64, UScalar.max_UScalarTy_U128_eq]
      exact mul_le_u128 (u64_le_max L) (by have := u64_le_max t; omega)))
  obtain ⟨i7, h7, h7v⟩ := WP.spec_imp_exists (UScalar.mul_spec (x := UScalar.cast .U128 g)
    (y := UScalar.cast .U128 p) (by
      rw [cast_u64, cast_u64, UScalar.max_UScalarTy_U128_eq]
      exact mul_le_u128 (u64_le_max g) (by have := u64_le_max p; omega)))
  exact ⟨i4, i7, h4, by rw [h4v, cast_u64, cast_u64], h7, by rw [h7v, cast_u64, cast_u64]⟩

/-- `blend` returns `⌊(locked·time_left + gain·period)/(locked + gain)⌋`, and `period` when
`locked + gain = 0`, PROVIDED the numerator fits a `u128`. The final `as u64` never truncates.
The proviso is necessary: see `blend_overflow`. -/
theorem blend_correct (L t g p : U64) (hfit : L.val * t.val + g.val * p.val ≤ U128.max) :
    ∃ v, formula.blend L t g p = ok v ∧ v.val = blend L.val t.val g.val p.val := by
  unfold formula.blend
  obtain ⟨tot, htot, htotv⟩ := blend_total L g
  by_cases h0 : L.val + g.val = 0
  · have : tot = 0#u128 := UScalar.eq_of_val_eq (by rw [htotv, h0]; rfl)
    exact ⟨p, by simp [lift, htot, this], by unfold Spec.blend; rw [if_pos h0]⟩
  · have hne : tot ≠ 0#u128 := fun h => h0 (by rw [← htotv, h]; rfl)
    obtain ⟨i4, i7, h4, h4v, h7, h7v⟩ := blend_products L t g p
    obtain ⟨i8, h8, h8v⟩ := WP.spec_imp_exists (UScalar.add_spec (x := i4) (y := i7)
      (by rw [h4v, h7v, UScalar.max_UScalarTy_U128_eq]; exact hfit))
    rw [h4v, h7v] at h8v
    obtain ⟨i9, h9, h9v⟩ := UScalar.div_spec i8 (y := tot) (by rw [htotv]; exact h0)
    rw [h8v, htotv] at h9v
    refine ⟨UScalar.cast .U64 i9, by simp [lift, htot, hne, h4, h7, h8, h9], ?_⟩
    have hb := (blend_between t.val p.val (Nat.pos_of_ne_zero h0)).2
    unfold Spec.blend at hb ⊢
    rw [if_neg h0] at hb ⊢
    rw [UScalar.cast_val_eq, h9v]
    apply Nat.mod_eq_of_lt
    have := u64_le_max t; have := u64_le_max p
    unfold U64_MAX at *; simp; omega

/-- `blend` PANICS (arithmetic overflow in the `u128` sum) exactly when the numerator
`locked·time_left + gain·period` exceeds `u128::MAX`. This direction: it then does not return. -/
theorem blend_overflow (L t g p : U64) (hbig : U128.max < L.val * t.val + g.val * p.val) :
    formula.blend L t g p = fail .integerOverflow := by
  unfold formula.blend
  obtain ⟨tot, htot, htotv⟩ := blend_total L g
  have h0 : L.val + g.val ≠ 0 := by
    intro h
    have hL : L.val = 0 := by omega
    have hg : g.val = 0 := by omega
    rw [hL, hg] at hbig; simp at hbig
  have hne : tot ≠ 0#u128 := fun h => h0 (by rw [← htotv, h]; rfl)
  obtain ⟨i4, i7, h4, h4v, h7, h7v⟩ := blend_products L t g p
  have hadd : i4 + i7 = fail .integerOverflow := by
    have hnb : ¬ i4.val + i7.val < 340282366920938463463374607431768211456 := by
      rw [h4v, h7v]; rw [u128_max_eq] at hbig; omega
    show UScalar.add i4 i7 = _
    simp [UScalar.add, UScalar.tryMk, UScalar.tryMkOpt, UScalar.check_bounds, Result.ofOption]
    rw [dif_neg hnb]
  simp [lift, htot, hne, h4, h7, hadd]

/-! ## Loss recovery and the fee clock -/

/-- `recover` returns `(gain − min(gain, loss), loss − min(gain, loss))`. It cannot panic. -/
theorem recover_correct (g l : U64) :
    ∃ p, formula.recover g l = ok p ∧ (p.1.val, p.2.val) = recover g.val l.val := by
  unfold formula.recover Spec.recover
  have hmin : (core.cmp.impls.OrdU64.min g l).val = min g.val l.val := by simp
  obtain ⟨i, hi, hiv, _⟩ := WP.spec_imp_exists (UScalar.sub_spec (x := g)
    (y := core.cmp.impls.OrdU64.min g l) (by rw [hmin]; omega))
  obtain ⟨i1, hi1, hi1v, _⟩ := WP.spec_imp_exists (UScalar.sub_spec (x := l)
    (y := core.cmp.impls.OrdU64.min g l) (by rw [hmin]; omega))
  exact ⟨(i, i1), by simp [lift, hi, hi1], by rw [hiv, hi1v, hmin]⟩

/-- `carry_elapsed` returns `⌊elapsed·min(before, after)/after⌋`, and `0` when `after = 0`.
It cannot panic, and the final `as u64` never truncates. -/
theorem carry_elapsed_correct (e b a : U64) :
    ∃ v, formula.carry_elapsed e b a = ok v ∧ v.val = carryElapsed e.val b.val a.val := by
  unfold formula.carry_elapsed Spec.carryElapsed
  by_cases ha : a = 0#u64
  · subst ha; exact ⟨0#u64, by simp, by simp⟩
  · have ha' : a.val ≠ 0 := by scalar_tac
    have hmin : (core.cmp.impls.OrdU64.min b a).val = min b.val a.val := by simp
    obtain ⟨i3, h3, h3v⟩ := WP.spec_imp_exists (UScalar.mul_spec (x := UScalar.cast .U128 e)
      (y := UScalar.cast .U128 (core.cmp.impls.OrdU64.min b a)) (by
        rw [cast_u64, cast_u64, UScalar.max_UScalarTy_U128_eq]
        exact mul_le_u128 (u64_le_max e) (by have := u64_le_max (core.cmp.impls.OrdU64.min b a); omega)))
    rw [cast_u64, cast_u64, hmin] at h3v
    obtain ⟨i5, h5, h5v⟩ := UScalar.div_spec i3 (y := UScalar.cast .U128 a) (by rw [cast_u64]; exact ha')
    rw [cast_u64, h3v] at h5v
    refine ⟨UScalar.cast .U64 i5, by simp [ha, lift, h3, h5], ?_⟩
    have hle := carryElapsed_le e.val b.val a.val
    unfold Spec.carryElapsed at hle
    rw [if_neg ha'] at hle ⊢
    rw [UScalar.cast_val_eq, h5v]
    apply Nat.mod_eq_of_lt
    have := u64_le_max e
    unfold U64_MAX at this; simp; omega

/-! ## Fee shares and fulfilment -/

theorem fit_some {n : Nat} {v : U64} (h : (some v).map (·.val) = fit n) : n = v.val ∧ n ≤ U64_MAX := by
  unfold fit at h; split at h <;> simp at h; exact ⟨h.symm, by assumption⟩

theorem fit_none {n : Nat} (h : (none : Option U64).map (·.val) = fit n) : U64_MAX < n := by
  unfold fit at h; split at h <;> simp at h; omega

/-- `fee_shares` returns `Some ⌊fee·(S+1)/(A+1−fee)⌋` when `fee ≤ A` and that fits a `u64`,
else `None`. It cannot panic: a zero divisor is `None`. -/
theorem fee_shares_correct (fee A S : U64) :
    ∃ r, formula.fee_shares fee A S = ok r ∧
      r.map (·.val) = if fee.val ≤ A.val then fit (feeShares fee.val A.val S.val) else none := by
  unfold formula.fee_shares Spec.feeShares
  obtain ⟨i1, h1, h1v⟩ := succ_u128 A
  have hsub := U128.checked_sub_bv_spec i1 (UScalar.cast .U128 fee)
  rw [cast_u64, h1v] at hsub
  cases hs : U128.checked_sub i1 (UScalar.cast .U128 fee) with
  | none =>
    rw [hs] at hsub
    exact ⟨none, by simp [lift, h1, hs, core.option.Option.filter, core.option.Option.Insts.CoreOpsTry_traitTry.branch,
      core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual],
      by rw [if_neg (by omega)]; rfl⟩
  | some v =>
    rw [hs] at hsub
    obtain ⟨hle, hv, _⟩ := hsub
    by_cases h0 : v.val = 0
    · exact ⟨none, by simp [lift, h1, hs, core.option.Option.filter,
        formula.fee_shares.closure.Insts.CoreOpsFunctionFnOnceTupleShared0U128Bool.call_once, h0,
        core.option.Option.Insts.CoreOpsTry_traitTry.branch,
        core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual],
        by rw [if_neg (by omega)]; rfl⟩
    · obtain ⟨i4, h4, h4v⟩ := succ_u128 S
      obtain ⟨r, hr, hrv⟩ := mul_div_spec fee i4 v h0
      have := mul_le_u128 (u64_le_max fee) (show S.val + 1 ≤ U64_MAX + 1 by have := u64_le_max S; omega)
      have hpos : 0 < v.val := by omega
      refine ⟨r, by simp [lift, h1, hs, core.option.Option.filter,
        formula.fee_shares.closure.Insts.CoreOpsFunctionFnOnceTupleShared0U128Bool.call_once, hpos,
        core.option.Option.Insts.CoreOpsTry_traitTry.branch, h4, hr], ?_⟩
      rw [hrv, h4v, hv, if_pos this, if_pos (by omega)]

/-- `fulfil` returns `Some` of the specified `(paid, burned)` when the ticket's worth fits a
`u64`, else `None`. It cannot panic, and the rounded-up burn never overflows. -/
theorem fulfil_correct (sh idle A S : U64) :
    ∃ r, formula.fulfil sh idle A S = ok r ∧
      r.map (fun p => (p.1.val, p.2.val)) =
        if toAssets sh.val A.val S.val ≤ U64_MAX then some (fulfil sh.val idle.val A.val S.val) else none := by
  unfold formula.fulfil
  obtain ⟨o, ho, hov⟩ := to_assets_correct sh A S
  cases o with
  | none =>
    have := fit_none hov
    exact ⟨none, by simp [ho, core.option.Option.Insts.CoreOpsTry_traitTry.branch,
      core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual],
      by rw [if_neg (by omega)]; rfl⟩
  | some owed =>
    obtain ⟨hval, hfit⟩ := fit_some hov
    by_cases hc : owed.val ≤ idle.val
    · have hc' : toAssets sh.val A.val S.val ≤ idle.val := by rw [hval]; exact hc
      have hn : ¬ idle.val < owed.val := by omega
      refine ⟨some (owed, sh), by simp [ho, core.option.Option.Insts.CoreOpsTry_traitTry.branch, hn], ?_⟩
      rw [if_pos hfit, (fulfil_full hc').1, hval]; rfl
    · have hc' : idle.val < toAssets sh.val A.val S.val := by rw [hval]; omega
      have hn : idle.val < owed.val := by omega
      obtain ⟨o1, ho1, ho1v⟩ := to_shares_ceil_correct idle A S
      have hle := toSharesCeil_le_of_short hc'
      cases o1 with
      | none => have := fit_none ho1v; have := u64_le_max sh; omega
      | some b =>
        obtain ⟨hb, _⟩ := fit_some ho1v
        refine ⟨some (idle, core.cmp.impls.OrdU64.min b sh),
          by simp [ho, core.option.Option.Insts.CoreOpsTry_traitTry.branch, hn, ho1, lift], ?_⟩
        rw [if_pos hfit, (fulfil_partial hc').1, hb]
        simp; omega

end VaultFormula
