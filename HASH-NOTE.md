# `registerWasm` uses Keccak-256

The `bytes32 wasm_hash` committed by `registerWasm` is the Keccak-256 digest of
the raw hosted WASM bytes. It is not the SHA-256 digest used by Telegraph miner
YAML manifests.

## Reproducible control

Registration 1686 declared:

`b665fa3d7310ebff4b885377c85a8304fa98a2ae82aa184c681ec9fd684966e8`

For the exact 24,199,967-byte artifact served by its registered URL:

```text
SHA-256   805708d8f22ac1a9d904a7c089bb7f29d2f95c0c110d33464d1e43c49e881979
Keccak-256 b665fa3d7310ebff4b885377c85a8304fa98a2ae82aa184c681ec9fd684966e8
```

The Keccak-256 value equals the registry entry byte-for-byte. It can be
reproduced with Foundry without putting binary data into a shell argument:

```bash
cast keccak < scorer.wasm
```

## Current threshold candidate

The candidate built from commit `4d2eb67` and hosted from PREFLIGHT commit
`33f2402489b1281f9104bd10ed1077396e000c6f` has:

```text
bytes       24,200,062
SHA-256     a06a9f98ee607e85e3b6922cc114407de01c647f24a1a122959651657beb10be
Keccak-256  18c53b2e4438020eb0d5287a18fc257095fb4a0f2417dda32be05ec7eda72eba
```

Immutable URL:

`https://raw.githubusercontent.com/shreshth006/Preflight/33f2402489b1281f9104bd10ed1077396e000c6f/public/wasm/hs-a06a9f98ee60.wasm`

Always download the hosted URL and hash those downloaded bytes immediately
before registration. Reusing a URL while deployment is in flight can make the
node fetch different bytes from the ones hashed locally. `registerWasm` also
rejects a digest already registered, so every candidate must have distinct
bytes and a distinct Keccak-256 digest.
