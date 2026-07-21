# Documentation

Technical documentation for the `fabric-gateway` Rust crate.

| Document | What it covers |
|---|---|
| [usage.md](usage.md) | How to use the crate — connecting, querying, submitting, events, off-line signing, custom signers, error handling, and a compact API reference. Start here. |
| [architecture.md](architecture.md) | Internal design — module layout, object model, transaction lifecycle, wire format, cryptography, transport, error model, and codegen. |
| [migration.md](migration.md) | Porting an existing app from fabric-gateway-go, the Node.js and Java Gateway SDKs, or the legacy `fabric-network` / `fabric-sdk-go` SDKs. |
| [faq.md](faq.md) | Troubleshooting common build, connection, transaction, and runtime errors. |
| [../PARITY.md](../PARITY.md) | How wire/crypto parity with fabric-gateway-go was verified, and live-network test results. |
| [../README.md](../README.md) | Project overview, feature-parity table, and quick start. |

New to the crate? Read **usage.md**, then dip into **architecture.md** when you
need to understand *why* something is shaped the way it is. Coming from another
Fabric SDK? Go straight to **migration.md**.
