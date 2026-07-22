# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-07-22

No public API or behaviour changes — this release adds test infrastructure,
documentation, and continuous verification.

### Added

- Differential byte-equality conformance testing against fabric-gateway-go
  (200 randomized proposals per run asserted byte-identical).
- Robustness / fuzz-lite decoder tests, plus `cargo-fuzz` targets under `fuzz/`.
- Live integration workflow against the official `fabric-samples` network, with
  cross-SDK interop checks against the Go client.
- Mutation-testing (`cargo-mutants`) config and workflow; added focused unit
  tests for proposal digest/transaction-id/signing (97.6% mutation kill on the
  crypto/wire core).
- Soak-test example (`examples/soak.rs`); `hello_world` and `chaincode_events`
  examples; FAQ and LIMITATIONS documentation.

### Changed

- Internal: proposal building refactored to a reproducible `build_with_context`
  path (output is unchanged and remains byte-identical to fabric-gateway-go).

## [0.1.0] - 2026-07-21

Initial release. A Rust client for the Hyperledger Fabric Gateway, ported from
the official `fabric-gateway` Go client and wire/crypto-compatible with it.

### Added

- Gateway connection API: `Gateway` / `GatewayBuilder`, `Network`, `Contract`.
- Transaction lifecycle: evaluate (query), submit (endorse → order → commit),
  `submit_async`, and a fine-grained `ProposalBuilder` with transient data and
  endorsing-organization selection.
- Chaincode events, and block / filtered-block event streams.
- Off-line signing: `Proposal`/`Transaction`/`Commit` `bytes()`/`digest()` plus
  `Gateway::new_signed_proposal`.
- Pluggable `Signer` trait; `EcdsaP256Signer` with low-S DER ECDSA(P-256)
  signing. Load keys from PEM, DER, a file, or a Fabric MSP `keystore` directory.
- Staged error model (`GatewayError`) with decoded `gateway.ErrorDetail`s.
- Cross-implementation parity tests against fabric-gateway-go and a live-network
  validation (see [PARITY.md](PARITY.md)).

[Unreleased]: https://github.com/chainlaunch/fabric-gateway-rs/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/chainlaunch/fabric-gateway-rs/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/chainlaunch/fabric-gateway-rs/releases/tag/v0.1.0
