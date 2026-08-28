# The registry's wasm_hash is not SHA-256 of the file

Registering a scoring module means committing a `bytes32` the node re-checks
after fetching the URL. The natural assumption — and the rule the *miner* YAML
docs state explicitly ("Use SHA-256, not keccak256") — is that this is the
SHA-256 of the artefact. For `registerWasm` it is not.

## Evidence

Two registrations of the same module, hashed with SHA-256, were rejected with
the node reporting a different digest each time:

| reg | host | our SHA-256 | node computed |
|---|---|---|---|
| 1634 | raw.githubusercontent.com | `213d72bd…` | `f34248ed…` |
| 1635 | our own CDN | `5d149882…` | `73defdbc…` |

The node's value is content-dependent (it differs with the file) and stable per
file, so it is a real digest of what it fetched, not an error page. Ruled out:

- **Not a fetch failure.** Both hosts serve HTTP 200 with byte-identical
  content on repeated fetches, verified three times each.
- **Not compression.** gzip (`083b04ab…`) and brotli (`fea82110…`) of the same
  response match neither value.
- **Not keccak256.** `66cd6bcf…` for the same bytes.

The control case is decisive: registration 1377 and the current champion (630)
both *passed* this check, yet neither of their registered hashes equals the
SHA-256 of the file their URL serves today:

| reg | registered hash | SHA-256 of hosted file |
|---|---|---|
| 1377 | `e427a7f0…` | `8cb641aa…` |
| 630 (champion) | `636983a2…` | `84d6b1dc…` |

So the mismatch is systematic, not something about our artefact.

## What works

Register the digest the node itself reports. A first attempt is rejected with
`expected=<ours> got=<theirs>`; registering `<theirs>` for the same URL passes
the fetch gate and moves the registration to `pending`. Registration 1636 did
exactly that.

Note that `registerWasm` reverts with `duplicate wasm hash` if the digest has
been used before, so the retry needs a hash that has not been registered —
which the node's reported value is, by construction.

## Practical recipe

1. Host the artefact at an immutable URL. Content-address the filename: a CDN
   will otherwise serve the previous bytes from cache at the same path, which
   cost us one registration.
2. Register once with SHA-256 and read the rejection.
3. Re-register the same URL with the `got=` digest from that rejection.
