# Usage guide

How to use `fabric-gateway` (Rust) to connect to a Fabric network and transact.
For internals see [architecture.md](architecture.md); to move an existing app
over from another Fabric gateway see [migration.md](migration.md).

- [Prerequisites](#prerequisites)
- [Adding the dependency](#adding-the-dependency)
- [Loading credentials](#loading-credentials)
- [Connecting](#connecting)
- [Evaluating (querying)](#evaluating-querying)
- [Submitting (updating)](#submitting-updating)
- [Advanced proposals](#advanced-proposals-transient-data-endorsing-orgs)
- [Chaincode events](#chaincode-events)
- [Block events](#block-events)
- [Off-line signing](#off-line-signing)
- [Custom signers (HSM / KMS)](#custom-signers-hsm--kms)
- [Timeouts](#timeouts)
- [Error handling](#error-handling)
- [API reference](#api-reference)

## Prerequisites

- Rust 1.75+ and `protoc` on `PATH` (proto code is generated at build time).
- A reachable Fabric peer with the Gateway service enabled (Fabric 2.4+).
- A client identity: an X.509 certificate issued by an MSP CA on the channel,
  and its P-256 private key. The certificate's role/OU must satisfy the
  chaincode's endorsement policy for submits.
- The peer's TLS CA certificate (to establish TLS to the gateway).

## Adding the dependency

This crate is not published to crates.io; depend on it by path or git:

```toml
[dependencies]
fabric-gateway = { path = "../fabric-gateway-rs" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tonic = { version = "0.12", features = ["tls", "tls-roots"] }
```

## Loading credentials

The identity is an MSP ID + certificate; the signer holds the private key. Both
can be loaded from the filesystem, with PEM/DER and SEC1/PKCS#8 auto-detected.

```rust
use fabric_gateway::{Identity, identity::EcdsaP256Signer};

// From individual files
let identity = Identity::from_cert_file("Org1MSP", "msp/signcerts/cert.pem")?;
let signer   = EcdsaP256Signer::from_file("msp/keystore/key.pem")?;

// From a Fabric MSP keystore directory (finds the single <hex>_sk file)
let signer = EcdsaP256Signer::from_keystore_dir("msp/keystore")?;

// From in-memory bytes
let identity = Identity::new("Org1MSP", cert_pem_bytes);
let signer   = EcdsaP256Signer::from_pem(key_pem_str)?; // or from_der(&der)
```

## Connecting

You create and configure the gRPC channel, then hand it to the gateway. This
keeps TLS and connection policy under your control.

```rust
use fabric_gateway::Gateway;
use tonic::transport::{Channel, ClientTlsConfig, Certificate};
use std::sync::Arc;

let tls = ClientTlsConfig::new()
    .ca_certificate(Certificate::from_pem(std::fs::read("tlsca.pem")?))
    // Must match a SAN on the peer's TLS certificate:
    .domain_name("peer0.org1.example.com");

let channel = Channel::from_static("https://peer0.org1.example.com:7051")
    .tls_config(tls)?
    .connect()
    .await?;

let gateway = Gateway::connect(identity, signer, channel)?;
```

`Gateway::connect` is shorthand; use the builder for options:

```rust
use std::time::Duration;

let gateway = Gateway::builder(identity, channel)
    .with_sign(signer)
    .with_endorse_timeout(Duration::from_secs(30))
    .with_submit_timeout(Duration::from_secs(15))
    .with_commit_status_timeout(Duration::from_secs(60))
    .connect()?;
```

Then obtain a channel (network) and chaincode (contract):

```rust
let network  = gateway.get_network("mychannel");
let contract = network.get_contract("basic");
// For a specific contract within a chaincode:
let contract = network.get_contract_with_name("basic", "AssetTransfer");
```

## Evaluating (querying)

```rust
let result = contract.evaluate_transaction("GetAllAssets", &[]).await?;
println!("{}", String::from_utf8_lossy(&result));

let asset = contract.evaluate_transaction("ReadAsset", &["asset1"]).await?;
```

## Submitting (updating)

`submit_transaction` runs endorse → order → commit and only returns once the
transaction commits with status `VALID`:

```rust
let result = contract
    .submit_transaction("CreateAsset", &["asset1", "blue", "20", "alice", "100"])
    .await?;
```

If you want the result as soon as the orderer accepts it, without waiting for
commit, use `submit_async` and await the commit separately:

```rust
let (result, commit) = contract.submit_async("CreateAsset", &["asset1", "blue"]).await?;
// ... use `result` now ...
let status = commit.status().await?;
if !status.successful {
    eprintln!("tx {} failed: {:?}", status.transaction_id, status.code);
}
```

## Advanced proposals (transient data, endorsing orgs)

For anything beyond positional string arguments, build the proposal explicitly
with `new_proposal`, then `evaluate()` or `endorse()`:

```rust
use std::collections::HashMap;

let mut transient = HashMap::new();
transient.insert("asset_properties".to_string(), br#"{"secret":42}"#.to_vec());

let proposal = contract
    .new_proposal("CreateAsset")
    .with_arguments(["asset1", "blue"])
    .with_transient(transient)
    .with_endorsing_organizations(["Org1MSP", "Org2MSP"])
    .build()?;

// Query with transient data:
let result = proposal.evaluate().await?;

// Or run the submit flow step by step:
// let tx     = proposal.endorse().await?;   // -> Transaction
// let result = tx.result().to_vec();        // result before commit
// let commit = tx.submit().await?;          // -> Commit
// let status = commit.status().await?;
```

`with_arguments` takes anything iterable of `Into<String>`; `with_bytes_arguments`
takes raw `Vec<u8>` arguments.

## Chaincode events

```rust
use tokio_stream::StreamExt;

let request = network
    .new_chaincode_events_request("basic")
    .with_start_block(0)          // optional; default is next committed block
    .build();

let mut events = request.events().await?;
while let Some(event) = events.next().await {
    println!("{} {}#{}: {}", event.block_number, event.chaincode_name,
             event.transaction_id, String::from_utf8_lossy(&event.payload));
}
```

To resume from a saved position, combine `with_start_block` and
`with_after_transaction_id`.

## Block events

```rust
// Full blocks:
let mut blocks = network.new_block_events_request().build_block().events().await?;
// Filtered blocks:
let mut filtered = network.new_block_events_request().build_filtered().events().await?;
```

## Off-line signing

When the private key lives elsewhere (an air-gapped signer, a wallet service),
connect a gateway *without* a signer, serialise the message, sign it remotely,
then reattach the signature:

```rust
let proposal = contract.new_proposal("CreateAsset").with_arguments(["asset1"]).build()?;

let bytes  = proposal.bytes();     // serialized ProposedTransaction
let digest = proposal.digest();    // what must be signed

let signature = remote_sign(&digest)?;   // your out-of-band signer

let signed = gateway.new_signed_proposal(&bytes, signature)?;
let result = signed.evaluate().await?;    // or .endorse().await?
```

`Transaction`, `Commit` and the event requests expose the same `bytes()` /
`digest()` pair for the same purpose.

> If your remote signer needs the full message rather than a digest, connect
> with `GatewayBuilder::with_hash(fabric_gateway::hash::none)` so the digest step
> is a no-op.

## Custom signers (HSM / KMS)

Implement `Signer` to delegate signing to any backend. The digest arrives
pre-hashed; return a low-S DER ECDSA signature.

```rust
use fabric_gateway::{Signer, GatewayError};

struct KmsSigner { /* client, key id, ... */ }

impl Signer for KmsSigner {
    fn sign(&self, digest: &[u8]) -> Result<Vec<u8>, GatewayError> {
        my_kms_sign(digest).map_err(|e| GatewayError::Signing(e.to_string()))
    }
}

let gateway = Gateway::builder(identity, channel)
    .with_sign(std::sync::Arc::new(KmsSigner { /* ... */ }))
    .connect()?;
```

A plain closure works too, thanks to a blanket impl:

```rust
let signer = std::sync::Arc::new(|digest: &[u8]| Ok(my_sign(digest)));
```

## Timeouts

Set per-stage defaults on the builder (`with_evaluate_timeout`,
`with_endorse_timeout`, `with_submit_timeout`, `with_commit_status_timeout`).
`with_commit_status_timeout` bounds how long a submit waits for commit — set it
generously, since it depends on block cutting and network latency.

## Error handling

Match on `GatewayError` to react to the failure stage. `TransactionError` gives
you the transaction ID and any per-node `ErrorDetail`s:

```rust
use fabric_gateway::GatewayError;

match contract.submit_transaction("CreateAsset", &["asset1"]).await {
    Ok(result) => { /* ... */ }
    Err(GatewayError::Endorse(e)) => {
        eprintln!("endorsement failed for tx {}", e.transaction_id);
        for d in &e.details {
            eprintln!("  {} ({}): {}", d.address, d.msp_id, d.message);
        }
    }
    Err(GatewayError::Commit { transaction_id, code }) => {
        eprintln!("tx {transaction_id} committed invalid: {code:?}");
    }
    Err(other) => eprintln!("{other}"),
}
```

## API reference

Handles and the objects they produce:

| Type | Key methods |
|---|---|
| `Gateway` | `connect`, `builder`, `get_network`, `identity`, `new_proposal_from_bytes`, `new_signed_proposal` |
| `GatewayBuilder` | `with_sign`, `with_hash`, `with_*_timeout`, `with_tls_client_certificate_hash`, `connect` |
| `Network` | `name`, `get_contract`, `get_contract_with_name`, `new_chaincode_events_request`, `new_block_events_request` |
| `Contract` | `chaincode_name`, `contract_name`, `evaluate_transaction`, `submit_transaction`, `submit_async`, `new_proposal` |
| `ProposalBuilder` | `with_arguments`, `with_bytes_arguments`, `with_transient`, `with_endorsing_organizations`, `build` |
| `Proposal` | `bytes`, `digest`, `transaction_id`, `evaluate`, `endorse` |
| `Transaction` | `result`, `bytes`, `digest`, `transaction_id`, `submit` |
| `Commit` | `bytes`, `digest`, `transaction_id`, `status` |
| `Status` | fields: `code`, `successful`, `transaction_id`, `block_number` |
| `Identity` | `new`, `from_cert_file`, `msp_id`, `credentials` |
| `EcdsaP256Signer` | `from_pem`, `from_der`, `from_file`, `from_keystore_dir`, `from_signing_key` |
| `Signer` (trait) | `sign(&self, digest) -> Result<Vec<u8>, GatewayError>` |
| `hash` (module) | `sha256`, `sha384`, `none` |
| Events | `ChaincodeEventsBuilder`, `ChaincodeEventsRequest`, `ChaincodeEvent`, `BlockEventsBuilder`, `BlockEventsRequest`, `FilteredBlockEventsRequest` |
