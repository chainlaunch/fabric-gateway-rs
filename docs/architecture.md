# Architecture

Technical reference for `fabric-gateway` (Rust). This explains how the crate is
put together, how a transaction flows through it, and the exact wire and
cryptographic behaviour that makes it interoperable with the official
Hyperledger Fabric Gateway clients.

- [Overview](#overview)
- [Module layout](#module-layout)
- [Object model](#object-model)
- [Transaction lifecycle](#transaction-lifecycle)
- [Wire format](#wire-format)
- [Cryptography](#cryptography)
- [Signing abstraction](#signing-abstraction)
- [gRPC transport](#grpc-transport)
- [Error model](#error-model)
- [Concurrency & runtime](#concurrency--runtime)
- [Protobuf code generation](#protobuf-code-generation)

## Overview

The Fabric Gateway is a service embedded in a Fabric peer (Fabric 2.4+). A
client submits a transaction proposal to *one* gateway peer, and that peer takes
care of collecting endorsements from other organizations, forwarding the
endorsed transaction to the ordering service, and reporting commit status. This
crate is a client for that service: it builds the signed protobuf messages the
gateway expects, sends them over gRPC, and interprets the responses.

It is a direct port of the `pkg/client` package from
[`hyperledger/fabric-gateway`](https://github.com/hyperledger/fabric-gateway)
(the Go client), and is wire- and signature-compatible with it. See
[PARITY.md](../PARITY.md) for the compatibility evidence.

The one deliberate deviation from the Go library is idiom: this crate is
**async** (Tokio + tonic) rather than synchronous, and it uses a builder API and
a `Signer` trait instead of Go's functional options and function-typed signer.

## Module layout

```
src/
├── lib.rs          Crate root, re-exports, crate docs
├── gateway.rs      Gateway + GatewayBuilder (connection & offline signing entry)
├── network.rs      Network (a channel)
├── contract.rs     Contract (a chaincode) — evaluate/submit convenience methods
├── proposal.rs     ProposalBuilder, Proposal, TransactionContext (txid/nonce)
├── transaction.rs  Transaction + endorsed-envelope parsing
├── commit.rs       Commit + Status
├── events.rs       Chaincode events + block/filtered-block events
├── identity.rs     Identity, Signer trait, EcdsaP256Signer
├── signing.rs      SigningIdentity (internal: id + signer + hash + creator)
├── hash.rs         sha256 / sha384 / none digest functions
├── client.rs       GrpcClient — thin async wrapper over generated gRPC stubs
├── error.rs        GatewayError, TransactionError, ErrorDetail decoding
├── util.rs         now_timestamp helper
└── pb.rs           Generated protobuf modules (tonic::include_proto!)
```

Public types are re-exported from the crate root; `client`, `signing` and
`util` are private implementation details.

## Object model

The public API mirrors the Fabric programming model as a hierarchy. Each level
is a cheap, cloneable handle that carries a shared reference to the gRPC client
and the signing identity:

```
Gateway            connection + client identity
  └─ Network       a Fabric channel (get_network("mychannel"))
       └─ Contract a chaincode (get_contract("basic"))
            └─ ProposalBuilder  → Proposal → Transaction → Commit → Status
```

`Gateway`, `Network` and `Contract` hold no exclusive state, so you can freely
create many `Network`/`Contract` handles from one `Gateway`. The per-transaction
objects (`Proposal`, `Transaction`, `Commit`) are consumed by value as they move
through the flow — `proposal.endorse()` takes `self` and yields a `Transaction`,
which `submit()` consumes to yield a `Commit`, and so on. This makes the
state machine explicit in the type system: you cannot submit an unendorsed
proposal or re-submit a consumed transaction.

## Transaction lifecycle

### Evaluate (query)

```
Contract::evaluate_transaction
  → ProposalBuilder::build      (construct + serialise proposal, derive txid)
  → Proposal::evaluate          (sign proposal, gRPC Evaluate)
  → EvaluateResponse.result.payload
```

Evaluate runs the transaction function on a single endorsing peer and returns
the result. Nothing is sent to the orderer and nothing is committed.

### Submit (update)

```
Contract::submit_transaction
  → ProposalBuilder::build
  → Proposal::endorse           (sign proposal, gRPC Endorse → prepared envelope)
  → Transaction::submit         (sign envelope, gRPC Submit → orderer)
  → Commit::status              (sign commit request, gRPC CommitStatus → wait)
  → Status { successful, code, block_number }
```

Three separate signatures are produced over three different messages — the
proposal, the transaction envelope, and the commit-status request — each
generated lazily just before the corresponding RPC. `submit_transaction` runs
the whole chain and returns an error unless the transaction commits with status
`VALID`. `submit_async` stops after Submit and hands back the result plus a
`Commit` you can await later.

## Wire format

The parity-critical byte layout, reproduced exactly from the Go client. A
transaction proposal is a nested set of protobuf messages:

```
ProposedTransaction
├── transaction_id: string                    (hex, see Cryptography)
├── proposal: SignedProposal
│    ├── proposal_bytes: bytes  ── Proposal
│    │    ├── header: bytes ── common.Header
│    │    │    ├── channel_header: bytes ── common.ChannelHeader
│    │    │    │    ├── type = ENDORSER_TRANSACTION (3)
│    │    │    │    ├── timestamp, channel_id, tx_id, epoch = 0
│    │    │    │    └── extension: bytes ── ChaincodeHeaderExtension{ ChaincodeID{ name } }
│    │    │    └── signature_header: bytes ── common.SignatureHeader{ creator, nonce }
│    │    └── payload: bytes ── ChaincodeProposalPayload
│    │         ├── input: bytes ── ChaincodeInvocationSpec{ ChaincodeSpec{ ChaincodeID{name}, Input{args} } }
│    │         └── TransientMap: map<string,bytes>
│    └── signature: bytes                       (ECDSA over sha256(proposal_bytes))
└── endorsing_organizations: repeated string
```

`args[0]` is the (optionally contract-qualified) transaction name; user
arguments follow. The `creator` is a serialised `msp.SerializedIdentity{ mspid,
id_bytes = certificate PEM }`.

The endorsed transaction returned by `Endorse` is a `common.Envelope`; its
`payload` is signed for `Submit`. `transaction.rs` parses that envelope
(`Payload → Transaction → TransactionAction → ChaincodeActionPayload →
ProposalResponsePayload → ChaincodeAction → Response.payload`) to extract the
result value before the transaction has committed.

## Cryptography

Everything here matches fabric-gateway-go so that signatures verify on the peer:

- **Transaction ID** — `hex( sha256( nonce ‖ creator ) )`, where `nonce` is 24
  random bytes and `creator` is the serialised identity. Implemented in
  `TransactionContext::new`.
- **Signing digest** — SHA-256 of the message being signed (proposal bytes,
  envelope payload, or commit-status request bytes). Configurable via
  `GatewayBuilder::with_hash`; default is `hash::sha256`.
- **Signature** — ECDSA on P-256, **low-S normalised**, ASN.1 DER encoded as
  `SEQUENCE { INTEGER r, INTEGER s }`.

> **Low-S is mandatory.** Fabric rejects signatures whose S value is in the
> upper half of the curve order. The `ecdsa` crate does *not* normalise S by
> default, so `EcdsaP256Signer` explicitly calls `normalize_s()` — the analogue
> of Go's `canonicalECDSASignatureSValue`. A regression test signs 50 messages
> and asserts every S ≤ N/2.

## Signing abstraction

Signing is decoupled from the private key through the `Signer` trait:

```rust
pub trait Signer: Send + Sync {
    fn sign(&self, digest: &[u8]) -> Result<Vec<u8>, GatewayError>;
}
```

A `Signer` receives a pre-computed digest and returns a DER-encoded signature.
The bundled `EcdsaP256Signer` covers the common case (a P-256 key held in
memory or on disk), but because the trait is object-safe and blanket-implemented
for closures, you can plug in an HSM, a KMS, or a remote signing service without
touching the rest of the library. This is also the seam for off-line signing:
connect a `Gateway` without a signer and use `Proposal::digest()` /
`Gateway::new_signed_proposal()` to sign elsewhere.

Internally, `SigningIdentity` bundles the `Identity`, the optional `Signer` and
the hash function, and provides `creator()` (serialised `SerializedIdentity`)
used both for the transaction ID and the `SignatureHeader`.

## gRPC transport

`GrpcClient` (in `client.rs`) wraps two generated tonic stubs against one shared
`tonic::transport::Channel`: the `Gateway` service (Endorse/Submit/Evaluate/
CommitStatus/ChaincodeEvents) and the peer `Deliver` service (block events). The
crate never creates the channel for you — you build and configure it (TLS,
authority, keepalive, proxy…) and pass it in. This keeps connection policy in
your hands and lets you share one channel across many `Gateway` instances.

Per-call timeouts (`with_evaluate_timeout`, etc.) are applied by setting a
deadline on each tonic request, mirroring Go's per-call `context` timeouts.

## Error model

`GatewayError` names the stage at which a failure occurred, mirroring the Go
error types:

| Variant | Corresponds to | Meaning |
|---|---|---|
| `Endorse(TransactionError)` | `EndorseError` | Endorse RPC failed |
| `Submit(TransactionError)` | `SubmitError` | Submit RPC failed |
| `CommitStatus(TransactionError)` | `CommitStatusError` | CommitStatus RPC failed |
| `Evaluate(TransactionError)` | (evaluate path) | Evaluate RPC failed |
| `Commit { transaction_id, code }` | `CommitError` | Delivered but validated invalid |
| `Protobuf` / `Identity` / `Signing` / `Connection` / `InvalidArgument` | — | Local failures |

`TransactionError` carries the transaction ID, the underlying `tonic::Status`,
and any structured `gateway.ErrorDetail`s (peer/orderer address, MSP ID,
message). Those details are packed by the gateway into the gRPC status as a
`google.rpc.Status`; `error.rs` decodes them so you get the originating node's
error, not just a generic transport failure. The `TransactionError` is boxed
inside `GatewayError` to keep `Result` values small.

## Concurrency & runtime

The crate is async and runtime-agnostic in principle, but depends on Tokio via
tonic. All network methods are `async`. Event streams
(`ChaincodeEventsRequest::events`, block-event `events`) return a boxed
`Stream`; internally a background Tokio task reads the gRPC stream and forwards
flattened events over an mpsc channel, mirroring the goroutine in the Go client.
Handles are `Clone` and cheap; the underlying `Channel` multiplexes concurrent
RPCs, so you can drive many transactions in parallel from one `Gateway`.

## Protobuf code generation

Message and service types are generated at build time by `build.rs` using
`tonic-build`, from the `.proto` files under `proto/` (copied from
[`hyperledger/fabric-protos`](https://github.com/hyperledger/fabric-protos)).
`build.rs` walks the tree and compiles every `.proto` so the full dependency
graph resolves. Generated modules are surfaced through `pb.rs`; note that
Fabric's `peer/*.proto` files declare `package protos`, so peer messages appear
under `pb::protos`. `proto/peer/chaincode_shim.proto` is intentionally omitted
because its `Connect` RPC would generate a client method colliding with tonic's
`Channel::connect`. `protoc` must be on `PATH` to build.
```
