# FAQ & troubleshooting

Common errors and how to resolve them. See [usage.md](usage.md) for the full
guide and [architecture.md](architecture.md) for internals.

## Build

### `protoc` failed / "File not found" for `google/protobuf/timestamp.proto`

`build.rs` compiles the vendored `.proto` files with `tonic-build`, which needs
the Protocol Buffers compiler **and** its bundled well-known types.

```bash
# macOS
brew install protobuf
# Debian / Ubuntu — libprotobuf-dev provides the well-known types
sudo apt-get install -y protobuf-compiler libprotobuf-dev
```

The "File not found" variant almost always means `protoc` is present but
`libprotobuf-dev` (the well-known types) is missing — common in clean CI
containers.

### The build works locally but fails in CI

Your local system `protoc` likely bundles the well-known types; a clean runner's
does not. Install both `protobuf-compiler` and `libprotobuf-dev` in CI. See
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml).

## Connecting

### TLS handshake fails / certificate name mismatch

The TLS `domain_name` (SNI/authority) must match a Subject Alternative Name on
the peer's TLS certificate. If you connect by IP or a name that isn't in the
cert, set it explicitly:

```rust
let tls = ClientTlsConfig::new()
    .ca_certificate(Certificate::from_pem(std::fs::read("tlsca.pem")?))
    .domain_name("peer0.org1.example.com"); // must be a SAN on the peer TLS cert
```

Also make sure you pass the peer's **TLS CA** certificate, not its MSP/signing CA.

### `no gateway connection details supplied` / connection refused

You must build and pass a `tonic::transport::Channel` yourself; the crate does
not create one. Confirm the endpoint uses `https://` for a TLS peer, and that
the peer's Gateway service is enabled (Fabric 2.4+).

## Transactions

### `creator org unknown, creator is malformed`

The peer does not recognise your client identity. The certificate you passed to
`Identity` must be issued by an MSP CA that the channel trusts, and the MSP ID
must match. A self-signed or wrong-MSP certificate produces exactly this error.

### Endorsement fails with a policy error, though the query worked

Evaluate (query) only needs a valid channel member, but submit must satisfy the
chaincode's **endorsement policy**. If the policy requires an `admin` role (e.g.
`OR('Org1MSP.admin')`), your certificate's OU/role must satisfy it — a plain
`client` identity will be rejected at endorsement.

### `no sign implementation supplied`

You connected without a signer. Pass one with `Gateway::connect(id, signer, ch)`
or `GatewayBuilder::with_sign(...)`. (Connecting without a signer is only valid
for the off-line signing flow, where you sign `Proposal::digest()` yourself.)

### Signatures are rejected by the peer (custom signer)

Fabric requires **low-S**, ASN.1-DER ECDSA signatures and rejects high-S ones.
The bundled `EcdsaP256Signer` handles this. If you implement `Signer` yourself,
you must normalise S to the lower half of the curve order before DER-encoding —
otherwise roughly half your signatures will be rejected intermittently.

### `expected exactly one key file in keystore dir ... found N`

`EcdsaP256Signer::from_keystore_dir` refuses to guess when a keystore directory
holds more than one key file. Point at the specific key with
`EcdsaP256Signer::from_file(path)` instead.

## Runtime

### "there is no reactor running" / must be called from a Tokio runtime

Every network method is `async` and needs a Tokio runtime. Annotate your entry
point with `#[tokio::main]`, or drive the futures on a runtime you create.

## Tests

### The parity tests don't seem to run

`tests/parity.rs` skips automatically unless the Go harness fixtures are present.
Generate them first — see [`parity-harness/README.md`](../parity-harness/README.md)
and [`PARITY.md`](../PARITY.md). The unit tests in `tests/unit.rs` always run.
