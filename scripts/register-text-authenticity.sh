#!/usr/bin/env bash
set -euo pipefail

# Registers the text-authenticity hybrid scorer as a candidate Canonical Script
# for TEXT_AUTHENTICITY_CHECK. Dry run unless EXECUTE_ONCHAIN=YES.
#
# Why this intent: it is the only intent with a low champion separation bar
# (0.6586) that also has zero scored traffic rows, so the node's real-traffic
# agreement stage cannot run and the fixture gate is the whole decision.

DIAMOND='0x5a2324aA18613FAD4e44bDF0d6c73Ec1f6D87ff8'
RPC_URL="${RPC_URL:-https://sepolia.base.org}"
INTENT='TEXT_AUTHENTICITY_CHECK'
WASM_URL="${WASM_URL:?set WASM_URL to the immutable raw.githubusercontent URL of dist/text_authenticity_slot_d01ad85d11d8.wasm}"
EXPECTED_SHA256='6de1c2d0edbcb3616c939927ded8b1a0a640f841576c790abd3a6313633ab423'
EXPECTED_KECCAK='0xa2019c31adbcd6cc74beae42f553aa153658a781cba66ad2c41febf2a9a1f5f5'
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
