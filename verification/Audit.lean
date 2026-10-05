/-
Axiom audit. Not part of the library: `./check.sh` runs this file and fails if any headline
theorem depends on anything but Lean's three standard axioms (propext, Classical.choice,
Quot.sound). The `Std.lean` trust base consists of definitions, so it adds none.
-/
import VaultFormula

#print axioms VaultFormula.to_shares_iff
#print axioms VaultFormula.to_assets_iff
#print axioms VaultFormula.unlocked_eq
#print axioms VaultFormula.blend_iff
#print axioms VaultFormula.blend_fits
#print axioms VaultFormula.performance_fee_iff
#print axioms VaultFormula.management_fee_iff
#print axioms VaultFormula.recover_eq
#print axioms VaultFormula.carry_elapsed_eq
#print axioms VaultFormula.fee_shares_iff
#print axioms VaultFormula.fulfil_iff
#print axioms VaultFormula.no_panic
#print axioms VaultFormula.to_shares_rounding
#print axioms VaultFormula.to_assets_rounding
#print axioms VaultFormula.round_trip
#print axioms VaultFormula.to_shares_monotonic
#print axioms VaultFormula.to_assets_monotonic
#print axioms VaultFormula.deposit_never_dilutes
#print axioms VaultFormula.unlocked_bounded
#print axioms VaultFormula.unlocked_monotonic
#print axioms VaultFormula.unlocked_two_steps
#print axioms VaultFormula.blend_window
#print axioms VaultFormula.performance_fee_rounding
#print axioms VaultFormula.management_fee_rounding
#print axioms VaultFormula.fee_shares_worth
#print axioms VaultFormula.fulfil_conserves_value
#print axioms VaultFormula.carried_fee_never_grows
#print axioms VaultFormula.recover_splits_the_gain
