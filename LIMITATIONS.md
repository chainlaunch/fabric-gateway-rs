# Limitations & provenance

This document is deliberately blunt about what this crate does **not** do yet,
and how it was built and verified. Naming the gaps is part of earning trust:
please read it before adopting.

## What is not implemented (vs fabric-gateway-go)

- **Off-line signing is proposal-only.** `Gateway::new_proposal_from_bytes` /
  `new_signed_proposal` re-hydrate a proposal for external signing. The Go
  library's `NewSignedTransaction`, `NewSignedCommit`, and
  `NewSignedChaincodeEventsRequest` are **not** yet provided. The building blocks
  (`bytes()` / `digest()` on every object) exist, so the flow can be completed,
  but the ready-made re-hydration helpers do not.
- **Block-and-private-data events are not exposed.** Chaincode events, block
  events, and filtered block events are implemented; the Go
  `BlockAndPrivateDataEvents` stream is not.
- **No persistent checkpointer.** You can resume events with
  `with_start_block` + `with_after_transaction_id`, but there is no equivalent of
  Go's file/in-memory `Checkpointer` types.
- **Built-in signer is ECDSA P-256 only.** The Go library also ships P-384 and
  Ed25519 signers. Those (and HSM/PKCS#11 or cloud KMS) are reachable through the
  `Signer` trait, but no built-in implementation is provided.
- **Hash options are SHA-256 / SHA-384 / NONE.** Go's SHA3-256 / SHA3-384 hash
  options are not exposed.
- **You own the gRPC connection.** Like fabric-gateway-go, this crate does not
  build the `tonic::transport::Channel` for you (TLS, SNI, keepalive are yours to
  configure). There is no connection-profile or discovery-config convenience.
- **Async only.** The API is Tokio-based; there is no synchronous facade.
- **Not yet on crates.io.** Depend on it by git or path for now.

If any of these blocks you, open an issue — several are small and are on the
[roadmap](README.md#roadmap).

## Provenance — how this was built and verified

This crate was written with AI assistance (Anthropic's Claude). That is exactly
why the verification is designed to be **reproducible by someone who does not
trust the author or the tooling**, rather than asserted:

- **Differential conformance in CI** — every run builds hundreds of randomized
  proposals with *both* the real fabric-gateway-go library and this crate and
  asserts the serialized bytes are identical (`src/conformance.rs`).
- **Robustness tests** — tens of thousands of malformed/truncated/bit-flipped
  inputs are fed to every network-facing decoder, asserting no panics
  (`src/robustness.rs`), with deeper `cargo-fuzz` targets under `fuzz/`.
- **Live integration in CI** — the integration workflow stands up the official
  Hyperledger `fabric-samples` network and runs the full submit → commit →
  read-back lifecycle, plus cross-SDK interop with the Go client.
- **Soak-tested** — 550 consecutive live operations completed with zero failures
  and flat (~10 MB) memory.

See [PARITY.md](PARITY.md) for the full methodology and results. All of it runs
in public CI, so you can watch it pass rather than take our word for it.

Nothing here should be read as a claim of production-hardening beyond what the
tests demonstrate. Review the code and run the suite yourself.
