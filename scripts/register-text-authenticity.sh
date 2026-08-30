#!/usr/bin/env bash
set -euo pipefail

# Registers the text-authenticity scorer as a candidate Canonical Script
# for TEXT_AUTHENTICITY_CHECK. Dry run unless EXECUTE_ONCHAIN=YES.
#
# Why this intent: it is the only intent with a low champion separation bar
# (0.6586) that also has zero scored traffic rows, so the node's real-traffic
# agreement stage cannot run and the fixture gate is the whole decision.

DIAMOND='0x5a2324aA18613FAD4e44bDF0d6c73Ec1f6D87ff8'
RPC_URL="${RPC_URL:-https://sepolia.base.org}"
INTENT='TEXT_AUTHENTICITY_CHECK'
WASM_URL="${WASM_URL:-https://raw.githubusercontent.com/shreshth006/telegraph-factscore/27d945fc22a95a0e280a6545cd97c86ed1158a38/dist/fork_ta_b0005_241c14f95ba2.wasm}"
EXPECTED_SHA256='241c14f95ba2741189cd7d4e0580f07af4989557cd76a1609afd6968ab3fb4b6'
EXPECTED_KECCAK='0x94e0130863f5430ef20896012f7ae958685f21a05521f1a4f1c8cd496dc8a6fb'
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
# The key file may hold the 32 bytes with or without the 0x prefix; normalise.
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
