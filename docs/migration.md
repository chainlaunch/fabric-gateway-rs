# Migrating to fabric-gateway (Rust)

This guide helps you port an application from another Hyperledger Fabric gateway
client to this Rust crate. It covers the three official Fabric Gateway SDKs
(Go, Node.js, Java), which share this crate's programming model, and the older
pre-gateway SDKs (`fabric-network` for Node, `fabric-sdk-go`), which require more
rethinking.

- [Before you start](#before-you-start)
- [Concept mapping](#concept-mapping)
- [From fabric-gateway-go](#from-fabric-gateway-go)
- [From fabric-gateway Node.js](#from-fabric-gateway-nodejs)
- [From fabric-gateway Java](#from-fabric-gateway-java)
- [From the legacy SDKs (fabric-network / fabric-sdk-go)](#from-the-legacy-sdks-fabric-network--fabric-sdk-go)
- [Behavioural differences to watch for](#behavioural-differences-to-watch-for)
- [Migration checklist](#migration-checklist)

## Before you start

The three official Gateway SDKs and this crate are all clients for the **same**
peer-side Gateway service, so they share concepts one-to-one: a gateway
connection bound to a client identity, a network per channel, a contract per
chaincode, and evaluate/submit/commit operations. Porting between them is mostly
mechanical.

Requirements that carry over unchanged:

- **Fabric 2.4+** with the peer Gateway service enabled. If you are on an older
  SDK because you run an older Fabric, upgrade Fabric first.
- The **same identities** — your existing MSP certificates and P-256 keys work
  as-is. No re-enrolment needed.
- The **same endorsement policies** — nothing about signing or policy changes.

The biggest structural change for most teams is that this crate is **async**
(Tokio) and Rust, so error handling, credential loading, and connection setup
are expressed differently even though the Fabric semantics are identical.

## Concept mapping

| Concept | Go | Node.js | Java | This crate |
|---|---|---|---|---|
| Connect | `client.Connect` | `connect()` | `Gateway.newInstance()` | `Gateway::connect` / `Gateway::builder` |
| Identity | `identity.NewX509Identity` | `{ mspId, credentials }` | `X509Identity` | `Identity::new` / `from_cert_file` |
| Signer | `identity.NewPrivateKeySign` | `signers.newPrivateKeySigner` | `Signers.newPrivateKeySigner` | `EcdsaP256Signer::from_*` |
| gRPC connection | `grpc.NewClient` | `@grpc/grpc-js` client | `ManagedChannel` | `tonic::transport::Channel` |
| Network (channel) | `gw.GetNetwork` | `gateway.getNetwork` | `gateway.getNetwork` | `gateway.get_network` |
| Contract | `network.GetContract` | `network.getContract` | `network.getContract` | `network.get_contract` |
| Query | `EvaluateTransaction` | `evaluateTransaction` | `evaluateTransaction` | `evaluate_transaction` |
| Submit | `SubmitTransaction` | `submitTransaction` | `submitTransaction` | `submit_transaction` |
| Async submit | `SubmitAsync` | `submitAsync` | `submitAsync` | `submit_async` |
| Proposal builder | `NewProposal(opts...)` | `newProposal(opts)` | `newProposal()` builder | `new_proposal(...)` builder |
| Transient data | `WithTransient` | `transientData` | `putTransient` | `with_transient` |
| Endorsing orgs | `WithEndorsingOrganizations` | `endorsingOrganizations` | `setEndorsingOrganizations` | `with_endorsing_organizations` |
| Chaincode events | `ChaincodeEvents` | `getChaincodeEvents` | `getChaincodeEvents` | `new_chaincode_events_request` |
| Block events | `BlockEvents` | `getBlockEvents` | `getBlockEvents` | `new_block_events_request` |
| Off-line signing | `NewSignedProposal` | `newSignedProposal` | `newSignedProposal` | `new_signed_proposal` |

## From fabric-gateway-go

This crate is a direct port of `fabric-gateway-go`, so the mapping is nearly
line-for-line. The differences are Rust idiom: a `Signer` trait instead of a
`Sign` function, a builder instead of functional options, and `.await` on the
network calls.

**Go**

```go
clientConn, _ := grpc.NewClient(peerEndpoint, grpc.WithTransportCredentials(creds))

cert, _ := identity.CertificateFromPEM(certPEM)
id, _   := identity.NewX509Identity("Org1MSP", cert)
key, _  := identity.PrivateKeyFromPEM(keyPEM)
sign, _ := identity.NewPrivateKeySign(key)

gw, _ := client.Connect(id, client.WithSign(sign), client.WithClientConnection(clientConn))
defer gw.Close()

contract := gw.GetNetwork("mychannel").GetContract("basic")

result, _ := contract.EvaluateTransaction("GetAllAssets")
_, _      = contract.SubmitTransaction("CreateAsset", "asset1", "blue", "20")
```

**Rust**

```rust
let channel = Channel::from_shared(format!("https://{peer_endpoint}"))?
    .tls_config(tls)?
    .connect()
    .await?;

let id     = Identity::new("Org1MSP", cert_pem);
let signer = Arc::new(EcdsaP256Signer::from_pem(&key_pem)?);

let gateway = Gateway::connect(id, signer, channel)?;

let contract = gateway.get_network("mychannel").get_contract("basic");

let result = contract.evaluate_transaction("GetAllAssets", &[]).await?;
contract.submit_transaction("CreateAsset", &["asset1", "blue", "20"]).await?;
```

Notes:

- Go's variadic `SubmitTransaction(name, args...)` becomes `submit_transaction(name, &["...", ...])`.
- There is no explicit `Close()`; the `Gateway` and its `Channel` drop normally.
- Functional options (`WithEndorseTimeout`, etc.) become builder methods
  (`Gateway::builder(...).with_endorse_timeout(...)`).
- `WithClientConnection` is implicit: you pass the `Channel` to `Gateway::builder`/`connect`.

## From fabric-gateway Node.js

`@hyperledger/fabric-gateway`.

**Node.js**

```js
const client = new grpc.Client(peerEndpoint, tlsCredentials);
const gateway = connect({
    client,
    identity: { mspId: 'Org1MSP', credentials: certPem },
    signer: signers.newPrivateKeySigner(crypto.createPrivateKey(keyPem)),
});

const contract = gateway.getNetwork('mychannel').getContract('basic');

const result = await contract.evaluateTransaction('GetAllAssets');
await contract.submitTransaction('CreateAsset', 'asset1', 'blue', '20');
```

**Rust**

```rust
let gateway = Gateway::connect(
    Identity::new("Org1MSP", cert_pem),
    Arc::new(EcdsaP256Signer::from_pem(&key_pem)?),
    channel,
)?;

let contract = gateway.get_network("mychannel").get_contract("basic");

let result = contract.evaluate_transaction("GetAllAssets", &[]).await?;
contract.submit_transaction("CreateAsset", &["asset1", "blue", "20"]).await?;
```

Notes:

- Node returns strings/`Uint8Array`; Rust returns `Vec<u8>`. Wrap with
  `String::from_utf8_lossy` when you expect text.
- Node's `getChaincodeEvents` async iterator maps to
  `network.new_chaincode_events_request("basic").build().events().await?`,
  consumed with `tokio_stream::StreamExt`.
- Both are async; `await`/`?` replace `await`/exceptions.

## From fabric-gateway Java

`org.hyperledger.fabric.client`.

**Java**

```java
var channel = Grpc.newChannelBuilder(endpoint, creds).build();
var gateway = Gateway.newInstance()
    .identity(new X509Identity("Org1MSP", certificate))
    .signer(Signers.newPrivateKeySigner(privateKey))
    .connection(channel)
    .connect();

var contract = gateway.getNetwork("mychannel").getContract("basic");
byte[] result = contract.evaluateTransaction("GetAllAssets");
contract.submitTransaction("CreateAsset", "asset1", "blue", "20");
```

**Rust**

```rust
let gateway = Gateway::builder(Identity::new("Org1MSP", cert_pem), channel)
    .with_sign(Arc::new(EcdsaP256Signer::from_pem(&key_pem)?))
    .connect()?;

let contract = gateway.get_network("mychannel").get_contract("basic");
let result = contract.evaluate_transaction("GetAllAssets", &[]).await?;
contract.submit_transaction("CreateAsset", &["asset1", "blue", "20"]).await?;
```

The Java builder (`newInstance().identity().signer().connection().connect()`)
maps directly onto `Gateway::builder(identity, channel).with_sign(...).connect()`.

## From the legacy SDKs (fabric-network / fabric-sdk-go)

The pre-2.4 SDKs — `fabric-network` (Node) and `fabric-sdk-go` — predate the
peer Gateway service. Migrating is a genuine re-architecture, not a rename,
because several client-side concepts move into the peer:

| Legacy concept | What happens now |
|---|---|
| Connection profile (CCP) YAML/JSON | Gone. You connect to **one** gateway peer with a `Channel`; the peer handles endorsement layout and orderer routing. |
| Wallet / identity store | Gone. Load the identity + key yourself (`Identity::from_cert_file`, `EcdsaP256Signer::from_file`) or from your own secret store. |
| Client-side service discovery | Moves into the gateway peer. Use `with_endorsing_organizations` only when you need to constrain endorsers (e.g. private data). |
| Event hubs / `addContractListener` | Replaced by `new_chaincode_events_request` / `new_block_events_request` streams. |
| `Gateway.connect(ccp, options)` | `Gateway::connect(identity, signer, channel)`. |

**Legacy fabric-network (Node)**

```js
const wallet = await Wallets.newFileSystemWallet('wallet');
const gateway = new Gateway();
await gateway.connect(connectionProfile, {
    wallet, identity: 'user1', discovery: { enabled: true, asLocalhost: true },
});
const contract = gateway.getNetwork('mychannel').then(n => n.getContract('basic'));
await contract.submitTransaction('CreateAsset', 'asset1', 'blue', '20');
```

**Rust**

```rust
// No CCP, no wallet — build a channel to one gateway peer and load the identity.
let channel = Channel::from_static("https://peer0.org1.example.com:7051")
    .tls_config(tls)?
    .connect()
    .await?;

let gateway = Gateway::connect(
    Identity::from_cert_file("Org1MSP", "wallet/user1/cert.pem")?,
    Arc::new(EcdsaP256Signer::from_keystore_dir("wallet/user1/keystore")?),
    channel,
)?;

let contract = gateway.get_network("mychannel").get_contract("basic");
contract.submit_transaction("CreateAsset", &["asset1", "blue", "20"]).await?;
```

Practical migration steps from a legacy SDK:

1. Ensure the target network is Fabric 2.4+ with the Gateway service enabled.
2. Pick a gateway peer to connect to (any peer of your org works).
3. Extract each identity's certificate and private key out of the wallet into
   files or your secret manager; drop the CCP entirely.
4. Replace discovery-based endorsement with the peer's built-in endorsement,
   adding `with_endorsing_organizations` only where you previously targeted
   specific orgs.
5. Rewrite listeners as event streams.

## Behavioural differences to watch for

- **Async everywhere.** Every network call is `async` and must be `.await`ed
  inside a Tokio runtime. Add `#[tokio::main]` or spawn a runtime.
- **You own the channel.** Unlike the Node/Java SDKs' convenience wrappers, and
  like the Go SDK, you construct and configure the `tonic::transport::Channel`
  (TLS, authority/SNI, keepalive) and pass it in. The TLS `domain_name` must
  match a SAN on the peer's TLS certificate.
- **Low-S signatures are automatic.** `EcdsaP256Signer` normalises S, so you do
  not need the manual low-S handling some custom signers carry over from older
  code. Custom `Signer` implementations **must** return low-S DER, though.
- **Byte results, not strings.** Query/submit return `Vec<u8>`. Decode
  explicitly.
- **Staged errors.** Instead of one exception type, match `GatewayError`
  (`Endorse`/`Submit`/`CommitStatus`/`Commit`) and read `TransactionError.details`
  for per-node diagnostics.
- **Consuming builders.** `Proposal`, `Transaction` and `Commit` are consumed as
  they advance (`endorse` → `submit` → `status`), which the compiler enforces —
  keep the returned value at each step.

## Migration checklist

- [ ] Fabric 2.4+ with the peer Gateway service enabled.
- [ ] A Tokio runtime in place (`#[tokio::main]` or equivalent).
- [ ] `tonic::transport::Channel` built with the peer's TLS CA and correct SNI.
- [ ] Identity loaded via `Identity::from_cert_file` / `Identity::new`.
- [ ] Signer loaded via `EcdsaP256Signer::from_file` / `from_keystore_dir`, or a
      custom `Signer` for HSM/KMS (returning low-S DER).
- [ ] `evaluateTransaction`/`submitTransaction` calls translated, arguments as
      `&[&str]`.
- [ ] Transient data / endorsing-org calls moved to the `new_proposal` builder.
- [ ] Event listeners rewritten as `events()` streams.
- [ ] Error handling switched to matching `GatewayError`.
- [ ] Connection profiles and wallets removed (legacy SDKs only).
```
