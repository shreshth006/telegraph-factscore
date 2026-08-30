#!/usr/bin/env bash
set -euo pipefail

# Registers the constant-only registration-628 fork for TWITTER_SEARCH.
# Dry run unless EXECUTE_ONCHAIN=YES.

DIAMOND='0x5a2324aA18613FAD4e44bDF0d6c73Ec1f6D87ff8'
RPC_URL="${RPC_URL:-https://sepolia.base.org}"
INTENT='TWITTER_SEARCH'
WASM_URL="${WASM_URL:-https://raw.githubusercontent.com/shreshth006/telegraph-factscore/ec3d0e47fc47481d456acdd7cd4476159b9ef171/dist/fork_tw_b003_ce65dd2cc0e9.wasm}"
EXPECTED_SHA256='ce65dd2cc0e90471d0ae785cb2e5c9d110ecd5130dc5361895acc8c0744c2af7'
EXPECTED_KECCAK='0x2cabd37b5ae7d9b509d0ac40105e57721c2353a66335e58d0c36ce13d30b5d5d'
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
if [[ "$key" =~ ^[0-9a-fA-F]{64}$ ]]; then key="0x${key}"; fi
if [[ ! "$key" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
  echo 'Private key is not a 32-byte hex value' >&2
  exit 1
fi

echo 'Submitting registerWasm transaction...'
"$CAST_BIN" send "$DIAMOND" \
  'registerWasm(bytes32,string,string)(uint256)' \
  "$EXPECTED_KECCAK" "$WASM_URL" "$INTENT" \
  --rpc-url "$RPC_URL" \
  --private-key "$key"
