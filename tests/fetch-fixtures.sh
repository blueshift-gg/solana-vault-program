#!/usr/bin/env bash
# Snapshot Jupiter Lend's USDC market from mainnet for the end-to-end tests:
# the lending and liquidity programs into target/deploy, and every account a
# deposit or withdrawal touches into tests/fixtures. Re-run to refresh; the
# tests read the snapshot's clock from fixtures/clock.json.
set -euo pipefail
cd "$(dirname "$0")"

LENDING=jup3YeL8QhtSx1e253b2FDvsMNC87fDrgQZivbrndc9
LIQUIDITY=jupeiUmn818Jg1ekPURTpr4mFo29p46vygyykFJ3wZC
USDC=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v
TOKEN=TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
ATA=ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL
DEPLOY=../target/deploy

pda() { solana find-program-derived-address "$@" --output json | python3 -c "import sys,json;print(json.load(sys.stdin)['address'])"; }
fetch() { solana account -u m "$2" --output json > "fixtures/$1.json"; }

mkdir -p fixtures "$DEPLOY"
solana program dump -u m "$LENDING" "$DEPLOY/jupiter_lending.so"
solana program dump -u m "$LIQUIDITY" "$DEPLOY/jupiter_liquidity.so"

F_TOKEN_MINT=$(pda "$LENDING" string:f_token_mint pubkey:"$USDC")
LENDING_STATE=$(pda "$LENDING" string:lending pubkey:"$USDC" pubkey:"$F_TOKEN_MINT")
LIQUIDITY_STATE=$(pda "$LIQUIDITY" string:liquidity)

fetch mint "$USDC"
fetch f_token_mint "$F_TOKEN_MINT"
fetch lending "$LENDING_STATE"
fetch lending_admin "$(pda "$LENDING" string:lending_admin)"
fetch liquidity "$LIQUIDITY_STATE"
fetch rate_model "$(pda "$LIQUIDITY" string:rate_model pubkey:"$USDC")"
fetch token_reserve "$(pda "$LIQUIDITY" string:reserve pubkey:"$USDC")"
fetch supply_position "$(pda "$LIQUIDITY" string:user_supply_position pubkey:"$USDC" pubkey:"$LENDING_STATE")"
fetch liquidity_vault "$(pda "$ATA" pubkey:"$LIQUIDITY_STATE" pubkey:"$TOKEN" pubkey:"$USDC")"
# The rewards rate model's address is recorded in the lending account, after
# its discriminator, two mints, id and decimals.
fetch rewards_rate_model "$(python3 - <<'PY'
import base64, json
A = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
data = base64.b64decode(json.load(open('fixtures/lending.json'))['account']['data'][0])
n, out = int.from_bytes(data[75:107], 'big'), ''
while n:
    n, r = divmod(n, 58)
    out = A[r] + out
print(out)
PY
)"
solana -u m epoch-info --output json | python3 -c "
import sys, json, time
info = json.load(sys.stdin)
json.dump({'slot': info['absoluteSlot'], 'epoch': info['epoch'], 'unix_timestamp': int(time.time())}, open('fixtures/clock.json', 'w'))"
ls -la fixtures
