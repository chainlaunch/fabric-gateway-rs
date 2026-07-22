# fabric-gateway (Rust)

[![CI](https://github.com/chainlaunch/fabric-gateway-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/chainlaunch/fabric-gateway-rs/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

A Rust client for the Hyperledger Fabric **Gateway**, ported from the official
[`fabric-gateway`](https://github.com/hyperledger/fabric-gateway) Go client
(`pkg/client`). It lets applications evaluate (query) and submit (update)
transactions against chaincode deployed to a Fabric network, connecting through
a peer's embedded Gateway gRPC service.

It is **wire- and crypto-compatible** with fabric-gateway-go: proposals,
transactions and commit-status requests are serialised with the same protobuf
layout, the same transaction-ID derivation (`hex(sha256(nonce || creator))`),
SHA-256 digesting, and low-S ASN.1-DER ECDSA(P-256) signatures — so signatures
produced here verify on a real Fabric peer, and each library can parse the
other's messages. See [PARITY.md](PARITY.md) for the parity test methodology and
results.

## Feature parity with fabric-gateway-go

| Capability | Go (`pkg/client`) | This crate |
|---|---|---|
| Connect with identity + signer | `client.Connect` | `Gateway::connect` / `Gateway::builder` |
| Evaluate (query) | `Contract.EvaluateTransaction` | `Contract::evaluate_transaction` |
| Submit (endorse → order → commit) | `Contract.SubmitTransaction` | `Contract::submit_transaction` |
| Async submit (result before commit) | `Contract.SubmitAsync` | `Contract::submit_async` |
| Fine-grained proposals | `Contract.NewProposal` | `Contract::new_proposal` (builder) |
| Transient data | `WithTransient` | `ProposalBuilder::with_transient` |
| Endorsing orgs | `WithEndorsingOrganizations` | `ProposalBuilder::with_endorsing_organizations` |
| Chaincode events | `Network.ChaincodeEvents` | `Network::new_chaincode_events_request` |
| Block / filtered block events | `Network.BlockEvents` / `FilteredBlockEvents` | `Network::new_block_events_request` → `build_block` / `build_filtered` |
| Off-line signing | `Gateway.NewSignedProposal` etc. | `Proposal::bytes`/`digest`, `Gateway::new_signed_proposal` |
| Per-call timeouts | `WithEvaluateTimeout` etc. | `GatewayBuilder::with_*_timeout` |
| mTLS cert hash | `WithTLSClientCertificateHash` | `GatewayBuilder::with_tls_client_certificate_hash` |
| Staged errors + `ErrorDetail` | `EndorseError`/`SubmitError`/… | `GatewayError::{Endorse,Submit,CommitStatus,Evaluate,Commit}` |

The main difference is idiom: this crate is **async** (Tokio/tonic) where the Go
library is synchronous, and it uses a pluggable [`Signer`] trait plus a builder
API instead of functional options.

## Install

**Prerequisite:** the Protocol Buffers compiler (`protoc`) must be on `PATH` —
`build.rs` uses it to generate the gateway wire types.

```bash
# macOS
brew install protobuf
# Debian / Ubuntu (needs libprotobuf-dev too, for the well-known types)
sudo apt-get install -y protobuf-compiler libprotobuf-dev
```

Then add the dependency (until the crate is published to crates.io, depend on it
by git or path):

```toml
[dependencies]
fabric-gateway = { git = "https://github.com/chainlaunch/fabric-gateway-rs" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tonic = { version = "0.12", features = ["tls", "tls-roots"] }
```

## Example

```rust
use fabric_gateway::{Gateway, Identity, identity::EcdsaP256Signer};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tls = tonic::transport::ClientTlsConfig::new()
        .ca_certificate(tonic::transport::Certificate::from_pem(std::fs::read("tlsca.pem")?))
        .domain_name("peer0.org1.example.com");
    let channel = tonic::transport::Channel::from_static("https://peer0.org1.example.com:7051")
        .tls_config(tls)?
        .connect()
        .await?;

    let identity = Identity::new("Org1MSP", std::fs::read("client-cert.pem")?);
    let signer = Arc::new(EcdsaP256Signer::from_pem(&std::fs::read_to_string("client-key.pem")?)?);
    let gateway = Gateway::connect(identity, signer, channel)?;

    let contract = gateway.get_network("mychannel").get_contract("basic");

    // Query
    let assets = contract.evaluate_transaction("GetAllAssets", &[]).await?;
    println!("{}", String::from_utf8_lossy(&assets));

    // Submit (waits for commit)
    contract.submit_transaction("CreateAsset", &["asset1", "blue", "20", "alice", "100"]).await?;
    Ok(())
}
```

Runnable examples live in [`examples/`](examples/):

- [`hello_world.rs`](examples/hello_world.rs) — the smallest connect-and-query.
- [`query.rs`](examples/query.rs) — a full CLI: TLS, evaluate, and submit.
- [`chaincode_events.rs`](examples/chaincode_events.rs) — stream chaincode events.

## Documentation

- [docs/usage.md](docs/usage.md) — full usage guide and API reference.
- [docs/architecture.md](docs/architecture.md) — internal design and wire format.
- [docs/migration.md](docs/migration.md) — migrating from the Go / Node.js / Java
  Gateway SDKs or the legacy `fabric-network` / `fabric-sdk-go` SDKs.
- [docs/faq.md](docs/faq.md) — troubleshooting common errors.
- [PARITY.md](PARITY.md) — parity methodology and live-network results.
- [LIMITATIONS.md](LIMITATIONS.md) — what's not implemented yet, and how this
  was built and verified (provenance).

## Loading keys and certificates from the filesystem

`EcdsaP256Signer` and `Identity` can read credentials directly off disk, so you
don't have to slurp files yourself. Encoding (PEM vs DER) and format (SEC1 vs
PKCS#8) are auto-detected.

```rust
use fabric_gateway::{Identity, identity::EcdsaP256Signer};

// Certificate + private key from individual files.
let identity = Identity::from_cert_file("Org1MSP", "msp/signcerts/cert.pem")?;
let signer   = EcdsaP256Signer::from_file("msp/keystore/key.pem")?; // PEM or DER

// Or point at a Fabric MSP keystore directory (the `<hex>_sk` file is found
// automatically; an ambiguous directory with multiple keys is a hard error).
let signer = EcdsaP256Signer::from_keystore_dir("msp/keystore")?;
```

`from_der(&[u8])` and `from_pem(&str)` remain available for in-memory material.

## Building

```bash
cargo build           # requires protoc on PATH for codegen
cargo test            # unit + parity tests
```

Proto definitions live under `proto/` (from
[hyperledger/fabric-protos](https://github.com/hyperledger/fabric-protos)) and
are compiled by `build.rs` via `tonic-build`.

## Roadmap

- [ ] Publish to [crates.io](https://crates.io) (enables `cargo add` + docs.rs).
- [ ] Event checkpointing helpers for resumable event streams.
- [ ] Optional built-in gRPC connection helper (TLS + SNI) behind a feature flag.
- [ ] Broader signer implementations (PKCS#11 / HSM, cloud KMS) as opt-in crates.
- [ ] Fuzz + differential testing of the wire codec against fabric-gateway-go.

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0, matching the upstream fabric-gateway project. See
[LICENSE](LICENSE) and [NOTICE](NOTICE).
