/-
Properties of the specification. Everything here is proved from `Spec.lean` alone, over
all natural numbers; `VaultFormula.lean` carries them to the Rust through `Refinement.lean`.

"Rounding favours the vault" is stated as an inequality in each direction: which side of the
exact rational value the result falls on, and by how much at most.
-/
import VaultFormula.Spec

namespace VaultFormula.Spec

/-! ## Division -/

theorem floor_mul_le (n d : Nat) : n / d * d ≤ n := Nat.div_mul_le_self n d

theorem lt_floor_succ_mul (n : Nat) {d : Nat} (hd : 0 < d) : n < (n / d + 1) * d := by
  rw [Nat.mul_comm]; exact Nat.lt_mul_div_succ n hd

theorem le_ceilDiv_mul (n : Nat) {d : Nat} (hd : 0 < d) : n ≤ ceilDiv n d * d := by
  have := lt_floor_succ_mul (n + d - 1) hd
  unfold ceilDiv; rw [Nat.add_mul] at this; omega

theorem ceilDiv_mul_lt (n : Nat) {d : Nat} (hd : 0 < d) : ceilDiv n d * d < n + d := by
  have := floor_mul_le (n + d - 1) d
  unfold ceilDiv; omega

theorem ceilDiv_le {n d k : Nat} (h : n ≤ k * d) : ceilDiv n d ≤ k := by
  rcases Nat.eq_zero_or_pos d with rfl | hd
  · simp [ceilDiv]
  · have h1 := ceilDiv_mul_lt n hd
    have h2 : ceilDiv n d * d < (k + 1) * d := by rw [Nat.add_mul]; omega
    exact Nat.le_of_lt_succ (Nat.lt_of_mul_lt_mul_right h2)

/-! ## Conversions: within one unit below the exact value -/

/-- Shares minted are the exact rational amount rounded down: less than one share is withheld. -/
theorem toShares_bounds (a A S : Nat) :
    toShares a A S * (A + 1) ≤ a * (S + 1) ∧ a * (S + 1) < (toShares a A S + 1) * (A + 1) :=
  ⟨floor_mul_le _ _, lt_floor_succ_mul _ (by omega)⟩

/-- Assets paid are the exact rational amount rounded down: less than one unit is withheld. -/
theorem toAssets_bounds (s A S : Nat) :
    toAssets s A S * (S + 1) ≤ s * (A + 1) ∧ s * (A + 1) < (toAssets s A S + 1) * (S + 1) :=
  ⟨floor_mul_le _ _, lt_floor_succ_mul _ (by omega)⟩

/-- Shares burned for a payout are the exact rational amount rounded up: less than one share extra. -/
theorem toSharesCeil_bounds (a A S : Nat) :
    a * (S + 1) ≤ toSharesCeil a A S * (A + 1) ∧ toSharesCeil a A S * (A + 1) < a * (S + 1) + (A + 1) :=
  ⟨le_ceilDiv_mul _ (by omega), ceilDiv_mul_lt _ (by omega)⟩

/-- More assets never mint fewer shares. -/
theorem toShares_mono {a b : Nat} (h : a ≤ b) (A S : Nat) : toShares a A S ≤ toShares b A S :=
  Nat.div_le_div_right (Nat.mul_le_mul_right _ h)

/-- More shares never pay fewer assets. -/
theorem toAssets_mono {a b : Nat} (h : a ≤ b) (A S : Nat) : toAssets a A S ≤ toAssets b A S :=
  Nat.div_le_div_right (Nat.mul_le_mul_right _ h)

/-- Round trip never profits: redeeming the shares a deposit minted, at the state the deposit
left, returns at most the deposit. -/
theorem round_trip_le (a A S : Nat) :
    toAssets (toShares a A S) (A + a) (S + toShares a A S) ≤ a := by
  have h := (toShares_bounds a A S).1
  generalize toShares a A S = sh at *
  unfold toAssets
  apply Nat.div_le_of_le_mul
  grind

/-- Round trip loses less than one unit of assets plus the post-deposit price of one share,
`(A + a + 1) / (S + sh + 1)`: with `sh` the shares minted and `back` the assets returned,
`a < back + 1 + (A + a + 1) / (S + sh + 1)`, cleared of denominators. -/
theorem round_trip_loss_lt (a A S : Nat) :
    a * (S + toShares a A S + 1) <
      (toAssets (toShares a A S) (A + a) (S + toShares a A S) + 1) * (S + toShares a A S + 1)
        + (A + a + 1) := by
  have h := (toShares_bounds a A S).2
  have hb := (toAssets_bounds (toShares a A S) (A + a) (S + toShares a A S)).2
  generalize toShares a A S = sh at *
  generalize toAssets sh (A + a) (S + sh) = back at *
  have e1 : a * (S + sh + 1) + (sh + 1) * (A + 1) + a = a * (S + 1) + (sh + 1) * (A + a + 1) := by grind
  have e2 : (sh + 1) * (A + a + 1) = sh * (A + a + 1) + (A + a + 1) := by grind
  omega

/-- A deposit never lowers the price `(A + 1) / (S + 1)`: the price after is at least the
price before, cross-multiplied. -/
theorem deposit_price_ge (a A S : Nat) :
    (A + 1) * (S + toShares a A S + 1) ≤ (A + a + 1) * (S + 1) := by
  have h := (toShares_bounds a A S).1
  generalize toShares a A S = sh at *
  grind

/-! ## Unlocking -/

/-- Never more than is locked. -/
theorem unlocked_le (L e w : Nat) : unlocked L e w ≤ L := by
  unfold unlocked
  split
  · exact Nat.le_refl _
  · rename_i h
    exact Nat.div_le_of_le_mul (by rw [Nat.mul_comm w L]; exact Nat.mul_le_mul_left _ (by omega))

/-- Inside the window the amount is `locked · elapsed / window` rounded down: short of the
exact value by less than one unit. -/
theorem unlocked_bounds (L : Nat) {e w : Nat} (h : e < w) :
    unlocked L e w * w ≤ L * e ∧ L * e < (unlocked L e w + 1) * w := by
  unfold unlocked; rw [if_neg (by omega)]
  exact ⟨floor_mul_le _ _, lt_floor_succ_mul _ (by omega)⟩

/-- Everything is unlocked exactly when the window is over or nothing was locked. -/
theorem unlocked_eq_locked_iff (L e w : Nat) : unlocked L e w = L ↔ w ≤ e ∨ L = 0 := by
  constructor
  · intro h
    by_cases hw : w ≤ e
    · exact .inl hw
    · have hb := (unlocked_bounds L (Nat.lt_of_not_le hw)).1
      rw [h] at hb
      rcases Nat.eq_zero_or_pos L with h0 | hpos
      · exact .inr h0
      · exact absurd (Nat.le_of_mul_le_mul_left hb hpos) hw
  · rintro (h | rfl)
    · unfold unlocked; rw [if_pos h]
    · exact Nat.le_zero.mp (unlocked_le 0 e w)

/-- More time never unlocks less. -/
theorem unlocked_mono (L : Nat) {e e' : Nat} (h : e ≤ e') (w : Nat) :
    unlocked L e w ≤ unlocked L e' w := by
  by_cases hw : w ≤ e'
  · have : unlocked L e' w = L := (unlocked_eq_locked_iff L e' w).mpr (.inl hw)
    rw [this]; exact unlocked_le _ _ _
  · unfold unlocked; rw [if_neg hw, if_neg (by omega)]
    exact Nat.div_le_div_right (Nat.mul_le_mul_left _ h)

/-- Path independence up to rounding. Unlock for `e₁` seconds, restart the line from what
is left over the rest of the window, and unlock for `e₂` more: the two steps together never
release more than one step of `e₁ + e₂` would, and fall short of it by at most one unit.
No hypothesis: past the window (`w - e₁` truncating at zero) both sides are `L`. -/
theorem unlocked_two_steps (L e₁ e₂ w : Nat) :
    unlocked L e₁ w + unlocked (L - unlocked L e₁ w) e₂ (w - e₁) ≤ unlocked L (e₁ + e₂) w ∧
    unlocked L (e₁ + e₂) w ≤ unlocked L e₁ w + unlocked (L - unlocked L e₁ w) e₂ (w - e₁) + 1 := by
  have ha := unlocked_le L e₁ w
  by_cases hfull : w - e₁ ≤ e₂
  · -- the second step ends the window: both ways release everything
    have h2 : unlocked (L - unlocked L e₁ w) e₂ (w - e₁) = L - unlocked L e₁ w := by
      unfold unlocked; rw [if_pos hfull]
    have h1 : unlocked L (e₁ + e₂) w = L := (unlocked_eq_locked_iff _ _ _).mpr (.inl (by omega))
    omega
  · obtain ⟨k, hk⟩ : ∃ k, w = e₁ + e₂ + k + 1 := ⟨w - e₁ - e₂ - 1, by omega⟩
    subst hk
    have hd : e₁ + e₂ + k + 1 - e₁ = e₂ + k + 1 := by omega
    rw [hd]
    obtain ⟨a1, a2⟩ := unlocked_bounds L (show e₁ < e₁ + e₂ + k + 1 by omega)
    obtain ⟨b1, b2⟩ := unlocked_bounds (L - unlocked L e₁ (e₁ + e₂ + k + 1)) (show e₂ < e₂ + k + 1 by omega)
    obtain ⟨c1, c2⟩ := unlocked_bounds L (show e₁ + e₂ < e₁ + e₂ + k + 1 by omega)
    generalize unlocked L (e₁ + e₂) (e₁ + e₂ + k + 1) = c at *
    generalize unlocked (L - unlocked L e₁ (e₁ + e₂ + k + 1)) e₂ (e₂ + k + 1) = b at *
    generalize unlocked L e₁ (e₁ + e₂ + k + 1) = a at *
    obtain ⟨R, rfl⟩ : ∃ R, L = a + R := ⟨L - a, by omega⟩
    rw [show a + R - a = R by omega] at b1 b2
    have hw : 0 < (e₁ + e₂ + k + 1) * (e₂ + k + 1) := Nat.mul_pos (by omega) (by omega)
    constructor
    · -- (a + b)·w ≤ L·(e₁ + e₂), shown after multiplying by the second window
      have A := Nat.mul_le_mul_right (k + 1) a1
      have B := Nat.mul_le_mul_right (e₁ + e₂ + k + 1) b1
      have key : (a + b) * (e₁ + e₂ + k + 1) * (e₂ + k + 1) ≤ (a + R) * (e₁ + e₂) * (e₂ + k + 1) := by grind
      have key' := Nat.le_of_mul_le_mul_right key (by omega)
      have : (a + b) * (e₁ + e₂ + k + 1) < (c + 1) * (e₁ + e₂ + k + 1) := by omega
      have := Nat.lt_of_mul_lt_mul_right this
      omega
    · -- L·(e₁ + e₂) < (a + b + 2)·w
      have A := Nat.mul_le_mul_right (k + 1) (Nat.succ_le_of_lt a2)
      have B := Nat.mul_le_mul_right (e₁ + e₂ + k + 1) (Nat.succ_le_of_lt b2)
      have key : (a + R) * (e₁ + e₂) * (e₂ + k + 1) < (a + b + 2) * (e₁ + e₂ + k + 1) * (e₂ + k + 1) := by grind
      have key' := Nat.lt_of_mul_lt_mul_right key
      have : c * (e₁ + e₂ + k + 1) < (a + b + 2) * (e₁ + e₂ + k + 1) := by omega
      have := Nat.lt_of_mul_lt_mul_right this
      omega

/-! ## Blending the unlock window -/

/-- The blended window is the weighted average rounded down: short by less than one second. -/
theorem blend_bounds {L g : Nat} (t p : Nat) (h : 0 < L + g) :
    blend L t g p * (L + g) ≤ L * t + g * p ∧ L * t + g * p < (blend L t g p + 1) * (L + g) := by
  unfold blend; rw [if_neg (by omega)]
  exact ⟨floor_mul_le _ _, lt_floor_succ_mul _ h⟩

/-- A new gain never stretches the window beyond the longer of the two times, nor shrinks it
below the shorter. -/
theorem blend_between {L g : Nat} (t p : Nat) (h : 0 < L + g) :
    min t p ≤ blend L t g p ∧ blend L t g p ≤ max t p := by
  unfold blend; rw [if_neg (by omega)]
  constructor
  · rw [Nat.le_div_iff_mul_le h]
    have h1 : L * min t p ≤ L * t := Nat.mul_le_mul_left _ (Nat.min_le_left _ _)
    have h2 : g * min t p ≤ g * p := Nat.mul_le_mul_left _ (Nat.min_le_right _ _)
    rw [Nat.mul_comm, Nat.add_mul]; omega
  · apply Nat.div_le_of_le_mul
    have h1 : L * t ≤ L * max t p := Nat.mul_le_mul_left _ (Nat.le_max_left _ _)
    have h2 : g * p ≤ g * max t p := Nat.mul_le_mul_left _ (Nat.le_max_right _ _)
    rw [Nat.add_mul]; omega

/-- With nothing locked before, the window is a full period. -/
theorem blend_of_no_locked (t g p : Nat) : blend 0 t g p = p := by
  unfold blend
  split
  · rfl
  · rename_i h; simp only [Nat.zero_mul, Nat.zero_add] at *
    exact Nat.mul_div_cancel_left p (by omega)

/-- With no gain, the window is the time that was left. -/
theorem blend_of_no_gain {L : Nat} (t p : Nat) (h : 0 < L) : blend L t 0 p = t := by
  unfold blend; rw [if_neg (by omega)]
  simp only [Nat.zero_mul, Nat.add_zero]
  exact Nat.mul_div_cancel_left t h

/-! ## Fees: the exact rational fee rounded down -/

theorem performanceFee_bounds (g bps : Nat) :
    performanceFee g bps * BPS ≤ g * bps ∧ g * bps < (performanceFee g bps + 1) * BPS :=
  ⟨floor_mul_le _ _, lt_floor_succ_mul _ (by decide)⟩

theorem managementFee_bounds (A bps e : Nat) :
    managementFee A bps e * (BPS * YEAR) ≤ A * bps * e ∧
      A * bps * e < (managementFee A bps e + 1) * (BPS * YEAR) :=
  ⟨floor_mul_le _ _, lt_floor_succ_mul _ (by decide)⟩

/-- A larger total, a higher rate or a longer time never lowers the management fee. -/
theorem managementFee_mono {A A' bps e e' : Nat} (h : A * e ≤ A' * e') :
    managementFee A bps e ≤ managementFee A' bps e' := by
  unfold managementFee
  apply Nat.div_le_div_right
  have := Nat.mul_le_mul_right bps h
  grind

/-! ## Fee shares -/

/-- Fee shares are `fee · (S + 1) / (A + 1 − fee)` rounded down. Needs `fee ≤ A`. -/
theorem feeShares_bounds {fee A : Nat} (S : Nat) (h : fee ≤ A) :
    feeShares fee A S * (A + 1 - fee) ≤ fee * (S + 1) ∧
      fee * (S + 1) < (feeShares fee A S + 1) * (A + 1 - fee) :=
  ⟨floor_mul_le _ _, lt_floor_succ_mul _ (by omega)⟩

/-- Fee shares, valued after they are minted, are worth at most the fee. -/
theorem feeShares_worth_le (fee A S : Nat) :
    toAssets (feeShares fee A S) A (S + feeShares fee A S) ≤ fee := by
  have h : feeShares fee A S * (A + 1 - fee) ≤ fee * (S + 1) := floor_mul_le _ _
  generalize feeShares fee A S = m at *
  unfold toAssets
  apply Nat.div_le_of_le_mul
  by_cases hf : fee ≤ A
  · obtain ⟨k, hk⟩ : ∃ k, A + 1 = k + fee := ⟨A + 1 - fee, by omega⟩
    rw [show A + 1 - fee = k by omega] at h
    rw [hk]; grind
  · have : m * (A + 1) ≤ m * fee := Nat.mul_le_mul_left _ (by omega)
    grind

/-- Fee shares fall short of the fee by less than the post-mint price of one share,
`(A + 1) / (S + m + 1)`: `fee · (S + m + 1) < (m + 1) · (A + 1)`. Needs `fee ≤ A`. -/
theorem feeShares_worth_gt {fee A : Nat} (S : Nat) (h : fee ≤ A) :
    fee * (S + feeShares fee A S + 1) < (feeShares fee A S + 1) * (A + 1) := by
  have h2 := (feeShares_bounds S h).2
  generalize feeShares fee A S = m at *
  obtain ⟨k, hk⟩ : ∃ k, A + 1 = k + fee := ⟨A + 1 - fee, by omega⟩
  rw [show A + 1 - fee = k by omega] at h2
  rw [hk]
  have e : (m + 1) * (k + fee) = (m + 1) * k + m * fee + fee := by grind
  have e2 : fee * (S + m + 1) = fee * (S + 1) + m * fee := by grind
  omega

/-! ## Fulfilment -/

/-- When idle does not cover the ticket, the rounded-up burn is at most the ticket, so the
`min` in `fulfil` never cuts it. -/
theorem toSharesCeil_le_of_short {sh idle A S : Nat} (h : idle < toAssets sh A S) :
    toSharesCeil idle A S ≤ sh := by
  apply ceilDiv_le
  have h1 := (toAssets_bounds sh A S).1
  have h2 : idle * (S + 1) ≤ toAssets sh A S * (S + 1) := Nat.mul_le_mul_right _ (by omega)
  omega

/-- A fulfilment pays at most idle, burns at most the ticket, and pays no more than the burned
shares are worth at the price `(A + 1) / (S + 1)`. -/
theorem fulfil_conserves (sh idle A S : Nat) :
    (fulfil sh idle A S).1 ≤ idle ∧ (fulfil sh idle A S).2 ≤ sh ∧
      (fulfil sh idle A S).1 * (S + 1) ≤ (fulfil sh idle A S).2 * (A + 1) := by
  unfold fulfil
  split
  · rename_i h; exact ⟨h, Nat.le_refl _, (toAssets_bounds sh A S).1⟩
  · rename_i h
    rw [Nat.min_eq_left (toSharesCeil_le_of_short (by omega))]
    exact ⟨Nat.le_refl _, toSharesCeil_le_of_short (by omega), (toSharesCeil_bounds idle A S).1⟩

/-- A ticket idle covers is burned whole and paid within one unit of its worth. -/
theorem fulfil_full {sh idle A S : Nat} (h : toAssets sh A S ≤ idle) :
    fulfil sh idle A S = (toAssets sh A S, sh) ∧
      sh * (A + 1) < ((fulfil sh idle A S).1 + 1) * (S + 1) := by
  unfold fulfil; simp only [h, if_true]
  exact ⟨trivial, (toAssets_bounds sh A S).2⟩

/-- A ticket idle does not cover pays all of idle and burns less than one share more than
that payment is exactly worth. -/
theorem fulfil_partial {sh idle A S : Nat} (h : idle < toAssets sh A S) :
    fulfil sh idle A S = (idle, toSharesCeil idle A S) ∧
      (fulfil sh idle A S).2 * (A + 1) < idle * (S + 1) + (A + 1) := by
  have hm := Nat.min_eq_left (toSharesCeil_le_of_short h)
  unfold fulfil; simp only [show ¬ toAssets sh A S ≤ idle by omega, if_false, hm]
  exact ⟨trivial, (toSharesCeil_bounds idle A S).2⟩

/-! ## The fee clock -/

/-- The carried time is never longer than the elapsed time. -/
theorem carryElapsed_le (e b a : Nat) : carryElapsed e b a ≤ e := by
  unfold carryElapsed
  split
  · omega
  · exact Nat.div_le_of_le_mul (by rw [Nat.mul_comm a e]; exact Nat.mul_le_mul_left _ (Nat.min_le_right _ _))

/-- The carried time is `elapsed · min(before, after) / after` rounded down: short by less than a second. -/
theorem carryElapsed_bounds (e b : Nat) {a : Nat} (ha : a ≠ 0) :
    carryElapsed e b a * a ≤ e * min b a ∧ e * min b a < (carryElapsed e b a + 1) * a := by
  unfold carryElapsed; simp only [ha, if_false]
  exact ⟨floor_mul_le _ _, lt_floor_succ_mul _ (by omega)⟩

/-- The fee accruing on the new total over the carried time is at most the fee the old total
had accrued over the elapsed time. -/
theorem carried_fee_le (e b a bps : Nat) :
    managementFee a bps (carryElapsed e b a) ≤ managementFee b bps e := by
  apply managementFee_mono
  by_cases ha : a = 0
  · simp [ha]
  · have h1 := (carryElapsed_bounds e b ha).1
    have h2 : e * min b a ≤ e * b := Nat.mul_le_mul_left _ (Nat.min_le_left _ _)
    rw [Nat.mul_comm a, Nat.mul_comm b]; omega

/-! ## Loss recovery -/

/-- A gain is split, never created: what is earned plus what is recovered is the gain, what is
recovered comes off the loss, and a gain is earned only once no loss is left. -/
theorem recover_splits (g l : Nat) :
    (recover g l).1 + min g l = g ∧ (recover g l).2 + min g l = l ∧
      ¬ (0 < (recover g l).1 ∧ 0 < (recover g l).2) := by
  unfold recover; simp only; omega

end VaultFormula.Spec
