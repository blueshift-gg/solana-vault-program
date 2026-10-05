/-
The trust base: Lean definitions of the Rust `core` functions that formula.rs calls and that
Aeneas does not translate (it leaves them as holes in FunsExternal_Template.lean).

Each definition states the documented behaviour of the Rust function; the quoted sentence is
from the Rust standard library documentation. Nothing else in this project is assumed: these
are definitions, not axioms, so they cannot introduce an inconsistency, only a mismatch with
Rust, and each is short enough to compare by eye.

The names and types are dictated by the generated Funs.lean.
-/
module
public import Aeneas
public import VaultFormula.Types
@[expose] public section
open Aeneas Aeneas.Std Result ControlFlow Error
set_option linter.dupNamespace false
open vault_formula

/-- `impl TryFrom<u128> for u64 :: try_from`. "Tries to create the target number type from a
source number type. This returns an error if the source value is outside of the range of the
target type." -/
def U64.Insts.CoreConvertTryFromU128TryFromIntError.try_from (x : Std.U128) :
    Result (core.result.Result Std.U64 core.num.error.TryFromIntError) :=
  if x.val ≤ U64.max then ok (.Ok (UScalar.cast .U64 x)) else ok (.Err ())

/-- `u128::div_ceil`. "Calculates the quotient of `self` and `rhs`, rounding the result towards
positive infinity. Panics: this function will panic if `rhs` is zero." Written as in `core`:
`let d = self / rhs; let r = self % rhs; if r > 0 { d + 1 } else { d }`. -/
def core.num.U128.div_ceil (x y : Std.U128) : Result Std.U128 := do
  let d ← x / y
  let r ← x % y
  if r > 0#u128 then d + 1#u128 else ok d

/-- `Option::filter`. "Returns `None` if the option is `None`, otherwise calls `predicate` with
the wrapped value and returns: `Some(t)` if `predicate` returns `true` (where `t` is the wrapped
value), and `None` if `predicate` returns `false`." -/
def core.option.Option.filter
    {T : Type} {P : Type} (opsfunctionFnOncePTupleSharedTBoolInst : core.ops.function.FnOnce P T Bool)
    (self : Option T) (predicate : P) : Result (Option T) :=
  match self with
  | some t => do
    let keep ← opsfunctionFnOncePTupleSharedTBoolInst.call_once predicate t
    if keep then ok (some t) else ok none
  | none => ok none

/-- `impl Try for Option<T> :: branch`, the test the `?` operator makes. In `core`:
`match self { Some(v) => ControlFlow::Continue(v), None => ControlFlow::Break(None) }`. -/
def core.option.Option.Insts.CoreOpsTry_traitTry.branch {T : Type} (self : Option T) :
    Result (core.ops.control_flow.ControlFlow (Option Never) T) :=
  match self with
  | some v => ok (core.ops.control_flow.ControlFlow.Continue v)
  | none => ok (core.ops.control_flow.ControlFlow.Break none)

/-- `impl FromResidual<Option<Infallible>> for Option<T> :: from_residual`, the early return the
`?` operator makes. In `core`: `match residual { None => None }`. -/
def core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual
    (T : Type) (_residual : Option Never) : Result (Option T) :=
  ok none

/-- `Result::ok`. "Converts from `Result<T, E>` to `Option<T>`. Converts `self` into an
`Option<T>`, consuming `self`, and discarding the error, if any." -/
def core.result.Result.ok {T : Type} {E : Type} (self : core.result.Result T E) : Result (Option T) :=
  match self with
  | .Ok t => Aeneas.Std.Result.ok (some t)
  | .Err _ => Aeneas.Std.Result.ok none
