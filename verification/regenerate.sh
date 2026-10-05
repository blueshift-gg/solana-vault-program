#!/bin/sh
# Re-translate packages/vault-core/src/math/formula.rs to Lean with Charon + Aeneas and
# compare with the checked-in translation under VaultFormula/.
#
#   ./regenerate.sh          fail if the checked-in translation differs from a fresh one
#   ./regenerate.sh --write  replace the checked-in translation with the fresh one
#
# Needs Docker: Charon and Aeneas are built with Nix inside a nixos/nix container at the
# pinned commits below. The first run builds both (about 25 minutes, cached afterwards in
# the Docker volume `vault-fv-nix`). `lake build` does not need any of this.
set -eu

AENEAS_REV=557eff83ecef5083b98a52a94ca7fae63d6c1dab
CHARON_REV=c8f15d7d658c86a95658f71ad99cddd4be002e04
GENERATED="Types.lean Funs.lean FunsExternal_Template.lean"

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

docker run --rm \
  -v vault-fv-nix:/nix \
  -v "$repo/packages/vault-core/src/math":/in/math:ro \
  -v "$here/extract":/in/extract:ro \
  -v "$out":/out \
  nixos/nix sh -euc '
    NIX="nix --extra-experimental-features nix-command --extra-experimental-features flakes"
    mkdir -p /w/packages/vault-core/src/math /w/verification
    cp /in/math/formula.rs /w/packages/vault-core/src/math/formula.rs
    cp -r /in/extract /w/verification/extract
    cd /w/verification/extract
    $NIX run github:AeneasVerif/charon/'"$CHARON_REV"' -- cargo --preset=aeneas
    $NIX run github:AeneasVerif/aeneas/'"$AENEAS_REV"' -- vault_formula.llbc -backend lean -split-files -dest /out
  '

status=0
for f in $GENERATED; do
  fresh=$(find "$out" -name "$f" | head -n 1)
  [ -n "$fresh" ] || { echo "regenerate: Aeneas did not produce $f" >&2; exit 1; }
  if [ "${1:-}" = "--write" ]; then
    cp "$fresh" "$here/VaultFormula/$f"
  elif ! diff -u "$here/VaultFormula/$f" "$fresh"; then
    status=1
  fi
done
[ $status -eq 0 ] || { echo "regenerate: VaultFormula/ is stale against formula.rs; run ./regenerate.sh --write and re-check the proofs" >&2; exit 1; }
[ "${1:-}" = "--write" ] && echo "regenerate: VaultFormula/ rewritten from formula.rs" || echo "regenerate: translation matches formula.rs"
