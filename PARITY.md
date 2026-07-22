# Parity report: fabric-gateway-rs vs fabric-gateway-go

This document records how wire- and crypto-compatibility with the official
Go client (`github.com/hyperledger/fabric-gateway` v1.11.0) was verified, and
the results of a live run against a real Hyperledger Fabric network.

## Why parity matters

A Fabric peer validates the client's signature over the exact serialised
proposal bytes and checks the transaction ID, creator identity, and low-S
signature encoding. If the Rust client's byte layout, hashing, ID derivation, or
signature encoding differed from what the peer expects, endorsement would fail.
So "parity" here means: **the bytes and signatures this crate produces are
accepted by the same peers, and each library can consume the other's messages.**

## Method

A Go harness (`parity-harness/main.go`) links the real `fabric-gateway` library
and exchanges artefacts with the Rust client using a shared identity (a P-256
key + X.509 cert). Four cross-checks:

1. **Rust parses Go's proposal.** Go builds a `ProposedTransaction`
   (`Contract.NewProposal`) and writes its bytes. Rust parses them with
   `Gateway::new_proposal_from_bytes` and re-derives the transaction ID and
   signing digest. Both **match the values Go reported** exactly.

2. **Go parses Rust's proposal.** Rust builds a proposal for the same
   contract/args and writes its bytes. The Go library's `Gateway.NewProposal`
   consumes them and extracts a valid transaction ID + digest — proving Rust's
   `ProposedTransaction` is structurally valid Fabric wire data.

3. **Rust verifies Go's signature; Go verifies Rust's.** Each library signs a
   fixed digest (`sha256("fabric-gateway-parity-test")`); the other verifies the
   low-S DER ECDSA signature against the shared public key. Both directions pass.

4. **Signatures are canonical low-S DER.** A unit test signs 50 messages and
   asserts every S value is ≤ N/2 for P-256 (Fabric rejects high-S). This caught
   a real bug: the `ecdsa` crate does **not** normalise S by default, so the
   signer now calls `normalize_s()` explicitly — matching Go's
   `canonicalECDSASignatureSValue`.

Self-contained unit tests (`tests/unit.rs`) additionally pin: transaction-ID
derivation `hex(sha256(nonce || creator))`, sign/verify round-trips, DER
structure, and acceptance of both SEC1 and PKCS#8 private-key PEM encodings.

### Reproduce

See [`parity-harness/README.md`](parity-harness/README.md) for generating the
shared test identity. Then:

```bash
# Go side (in parity-harness/):
go build -o parity . && ./parity build-proposal && \
  ./parity sign $(printf 'fabric-gateway-parity-test' | shasum -a 256 | cut -d' ' -f1) && \
  ./parity pubkey

# Rust side (PARITY_DIR points at the harness dir holding the fixtures):
PARITY_DIR=./parity-harness cargo test         # unit + parity tests

# Reverse direction:
./parity parse-proposal rust_proposal.bin      # Go consumes Rust's proposal
./parity verify <digest> rust_sig.bin          # Go verifies Rust's signature
```

Result: **all parity checks pass in both directions.**

## Live network test

The client was exercised against a running Hyperledger Fabric network with a
peer Gateway endpoint, an organization MSP, and a chaincode whose endorsement
policy required an `admin`-role identity.

The client identity was created **without exposing any private key**: a P-256
keypair was generated locally, and the organization's Fabric CA issued an
`OU=admin` leaf certificate for the *public* key. The private key never left the
test host, and no existing key material was extracted.

Results, all through this Rust client over TLS to the peer's Gateway service,
against the standard `asset-transfer` sample chaincode:

| Operation | Function | Outcome |
|---|---|---|
| Connect (TLS + gRPC) | — | channel established |
| Identity validation | — | accepted by peer MSP |
| Evaluate | `GetAllAssets` | returned the seeded assets |
| Evaluate | `GetMetadata` | returned contract metadata |
| **Submit** (endorse → order → commit) | `CreateAsset` | committed `VALID`, asset persisted |
| Evaluate (read-back) | `ReadAsset` | returned the newly-created asset |

The submit path exercised every signing operation in the lifecycle — proposal,
transaction envelope, and commit-status request — each accepted by the real peer
and orderer. The structured `gateway.ErrorDetail` decoding was also validated
against a genuine peer error response (an untrusted identity produced
`creator org unknown, creator is malformed`, correctly surfaced with address +
MSP ID + message).

## Continuous, reproducible verification

The checks above are wired into CI so a skeptic can watch them pass on neutral
infrastructure rather than take any single run on trust:

- **Differential byte-equality conformance** (`src/conformance.rs`, runs in the
  main CI): each run builds **200 randomized proposals** (random channel,
  chaincode, function, args, transient data, and endorsing orgs) with *both* the
  real fabric-gateway-go library and this crate — injecting Go's nonce and
  timestamp into the Rust build — and asserts the serialized `ProposedTransaction`
  bytes are **identical**. This turns "compatible on one example" into "byte-for-
  byte identical across thousands of randomized inputs." (Locally verified across
  1,000+ cases; it also caught a genuine proto edge case — empty transient map
  values encode differently between proto libraries, unrelated to gateway logic.)

- **Live integration against the official `fabric-samples` network**
  (`.github/workflows/integration.yml` + `tests/fabric_samples.rs`): CI stands up
  the upstream Hyperledger test network in Docker, deploys `asset-transfer-basic`,
  and runs the full submit → commit → read-back lifecycle through this client —
  reproducible by anyone, on GitHub's runners, not a private network.

- **Cross-SDK interop on that live ledger**: the same workflow has this Rust
  client write an asset that the Go client reads back, and vice versa — proving
  the two implementations agree on a real ledger, not just in a byte comparison.

## Conclusion

`fabric-gateway-rs` is functionally equivalent to fabric-gateway-go for the
transaction and eventing APIs, produces byte-identical wire structures and
Fabric-valid signatures, interoperates bidirectionally with the Go library, and
successfully drives the full query + submit lifecycle against a live Hyperledger
Fabric network — all verified continuously and reproducibly in public CI.
