#!/usr/bin/env bash
set -euo pipefail

DIAMOND='0x5a2324aA18613FAD4e44bDF0d6c73Ec1f6D87ff8'
RPC_URL="${RPC_URL:-https://sepolia.base.org}"
INTENT='IP_GEOLOCATION'
WASM_URL='https://raw.githubusercontent.com/shreshth006/telegraph-factscore/5eafd73ce8052f144216b5a7681c2e29014c9c5b/dist/hybrid_threshold_robust_f906081e0df9.wasm'
EXPECTED_SHA256='f906081e0df92f1e9c4e7ff318cc2cd25cd809815336e393491a4c6af07878de'
EXPECTED_KECCAK='0x3543fcb80073425d99eb4329135214361d466dbc0deeda02b5eedf7da7e83407'
CAST_BIN="${CAST_BIN:-${HOME}/.foundry/bin/cast}"

if [[ ! -x "$CAST_BIN" ]]; then
  echo "cast not found at $CAST_BIN; set CAST_BIN to the Foundry cast executable" >&2
  exit 1
fi

artifact="$(mktemp)"
trap 'rm -f "$artifact"' EXIT
curl --fail --silent --show-error --location "$WASM_URL" --output "$artifact"

actual_sha256="$(sha256sum "$artifact" | awk '{print $1}')"
actual_keccak="$($CAST_BIN keccak < "$artifact")"
if [[ "$actual_sha256" != "$EXPECTED_SHA256" ]]; then
  echo "SHA-256 mismatch: expected $EXPECTED_SHA256, got $actual_sha256" >&2
  exit 1
fi
if [[ "${actual_keccak,,}" != "${EXPECTED_KECCAK,,}" ]]; then
  echo "Keccak-256 mismatch: expected $EXPECTED_KECCAK, got $actual_keccak" >&2
  exit 1
fi

echo "Verified immutable candidate bytes:"
echo "  URL:        $WASM_URL"
echo "  SHA-256:    $actual_sha256"
echo "  Keccak-256: $actual_keccak"
echo "  Intent:     $INTENT"

if [[ "${EXECUTE_ONCHAIN:-NO}" != 'YES' ]]; then
  echo
  echo 'Dry run only. This would submit a wallet-spending Base Sepolia transaction:'
  echo "$CAST_BIN send $DIAMOND 'registerWasm(bytes32,string,string)(uint256)' $EXPECTED_KECCAK '$WASM_URL' '$INTENT' --rpc-url '$RPC_URL' --private-key '<redacted>'"
  echo
  echo 'Run with EXECUTE_ONCHAIN=YES only after explicitly approving that transaction.'
  exit 0
fi

key="${MINER_PRIVATE_KEY:-}"
if [[ -z "$key" ]]; then
  key_file="${MINER_KEY_FILE:-${HOME}/.preflight-miner-key}"
  if [[ ! -f "$key_file" ]]; then
    echo "No MINER_PRIVATE_KEY and key file not found: $key_file" >&2
    exit 1
  fi
  key="$(tr -d '[:space:]' < "$key_file")"
fi
if [[ ! "$key" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
  echo 'Private key is not a 0x-prefixed 32-byte hex value' >&2
  exit 1
fi

echo 'Submitting registerWasm transaction...'
"$CAST_BIN" send "$DIAMOND" \
  'registerWasm(bytes32,string,string)(uint256)' \
  "$EXPECTED_KECCAK" "$WASM_URL" "$INTENT" \
  --rpc-url "$RPC_URL" \
  --private-key "$key"
