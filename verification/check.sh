#!/bin/sh
# Check every proof, then audit the axioms the headline theorems rest on.
# Needs only elan/lake. The first run downloads the Aeneas library and a prebuilt Mathlib.
set -eu
cd "$(dirname "$0")"

# No sorry, admit or axiom anywhere, except the holes Aeneas lists in its template, which is
# not compiled: each hole must be a definition in Std.lean.
if grep -rnE '\b(sorry|admit)\b|^[[:space:]]*(private |protected )?axiom\b' --include='*.lean' \
     --exclude=FunsExternal_Template.lean VaultFormula VaultFormula.lean Audit.lean; then
  echo "check: sorry, admit or axiom in the sources" >&2; exit 1
fi
holes=$(tr '\n' ' ' < VaultFormula/FunsExternal_Template.lean | grep -oE 'axiom +[^ ]+' | awk '{print $2}')
for h in $holes; do
  grep -qE "^def +$h( |\$)" VaultFormula/Std.lean || tr '\n' ' ' < VaultFormula/Std.lean | grep -qE "def +$h " \
    || { echo "check: $h is opaque in the translation and not defined in Std.lean" >&2; exit 1; }
done
echo "check: $(echo "$holes" | wc -w | tr -d ' ') opaque std functions, all defined in Std.lean"

[ -d .lake/packages/mathlib/.lake/build ] || lake exe cache get
lake build

out=$(lake env lean Audit.lean)
printf '%s\n' "$out"
rest=$(printf '%s\n' "$out" \
  | sed -e "s/'[^']*' depends on axioms://" -e "s/'[^']*' does not depend on any axioms//" \
        -e 's/propext//' -e 's/Classical\.choice//' -e 's/Quot\.sound//' \
  | tr -d '[], \n')
[ -z "$rest" ] || { echo "check: unexpected axioms: $rest" >&2; exit 1; }
n=$(printf '%s\n' "$out" | grep -c "axioms")
echo "check: all proofs build; $n headline theorems rest on Lean's three standard axioms at most"
