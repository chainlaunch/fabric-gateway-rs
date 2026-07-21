// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Cross-implementation parity tests against fabric-gateway-go.
//!
//! These are wire-compatibility tests. A Go harness (in `parity-harness`, built
//! from the real `fabric-gateway` library) exchanges artefacts with this Rust
//! client:
//!
//!   * `go_proposal.bin` — a `ProposedTransaction` the Go library built. This
//!     test parses it with the Rust client and checks the extracted transaction
//!     ID / digest match what Go reported.
//!   * `go_sig.bin` / `pub.pem` — a DER ECDSA signature the Go library produced
//!     over a fixed digest, verified here with the Rust crypto stack.
//!
//! It also builds a proposal with the Rust client and writes `rust_proposal.bin`
//! so the Go harness can confirm it consumes Rust output (checked by the shell
//! runner, `run_parity.sh`).
//!
//! The fixtures live in `PARITY_DIR` (default `parity-harness`). Tests are skipped
//! with a clear message if the harness has not been run.

use fabric_gateway::identity::EcdsaP256Signer;
use fabric_gateway::{Gateway, Identity};
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use p256::ecdsa::{DerSignature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use std::path::PathBuf;
use std::sync::Arc;

fn parity_dir() -> PathBuf {
    PathBuf::from(std::env::var("PARITY_DIR").unwrap_or_else(|_| "parity-harness".to_string()))
}

fn read(name: &str) -> Option<Vec<u8>> {
    std::fs::read(parity_dir().join(name)).ok()
}

/// An offline Gateway (no live connection) is enough to build/parse proposals.
/// Returns `None` when the harness fixtures have not been generated, so the
/// tests can skip cleanly on a fresh checkout instead of panicking.
fn offline_gateway() -> Option<Gateway> {
    let dir = parity_dir();
    let cert = std::fs::read(dir.join("cert.pem")).ok()?;
    let key = std::fs::read_to_string(dir.join("key.pem")).ok()?;
    let identity = Identity::new("Org1MSP", cert);
    let signer = Arc::new(EcdsaP256Signer::from_pem(&key).expect("parse key"));

    // A lazily-connected channel; we never dial for build/parse operations.
    let channel = tonic::transport::Channel::from_static("http://127.0.0.1:0").connect_lazy();
    Some(
        Gateway::builder(identity, channel)
            .with_sign(signer)
            .connect()
            .expect("gateway connect"),
    )
}

#[tokio::test]
async fn rust_parses_go_proposal() {
    let Some(bytes) = read("go_proposal.bin") else {
        eprintln!("SKIP: run ./run_parity.sh first (missing go_proposal.bin)");
        return;
    };
    let expected = std::fs::read_to_string(parity_dir().join("go_proposal.json"))
        .expect("go_proposal.json (tx id + digest reported by Go)");

    let Some(gw) = offline_gateway() else {
        eprintln!("SKIP: missing harness identity fixtures (cert.pem/key.pem)");
        return;
    };
    let proposal = gw
        .new_proposal_from_bytes(&bytes)
        .expect("rust parses go proposal");

    let tx_id = proposal.transaction_id().to_string();
    let digest = hex::encode(proposal.digest());

    // The Go harness reported these values; they must match exactly.
    assert!(
        expected.contains(&tx_id),
        "transaction id mismatch: rust={tx_id} go_report={expected}"
    );
    assert!(
        expected.contains(&digest),
        "digest mismatch: rust={digest} go_report={expected}"
    );
    println!("OK: Rust parsed Go's proposal — txid={tx_id}");
}

#[tokio::test]
async fn rust_verifies_go_signature() {
    let (Some(sig), Some(pub_pem)) = (read("go_sig.bin"), read("pub.pem")) else {
        eprintln!("SKIP: run ./run_parity.sh first (missing go_sig.bin/pub.pem)");
        return;
    };

    // Fixed message the Go harness signed: sha256("fabric-gateway-parity-test").
    let digest = fabric_gateway::hash::sha256(b"fabric-gateway-parity-test");

    let vk = VerifyingKey::from_public_key_pem(std::str::from_utf8(&pub_pem).unwrap())
        .expect("parse pubkey");
    let der = DerSignature::from_bytes(&sig).expect("parse der sig");
    vk.verify_prehash(&digest, &der)
        .expect("Rust must verify Go's low-S DER signature");
    println!("OK: Rust verified Go's ECDSA signature");
}

#[tokio::test]
async fn rust_builds_proposal_for_go() {
    // Build a proposal with the Rust client and persist it so the Go harness can
    // confirm cross-consumption (asserted by run_parity.sh).
    let Some(gw) = offline_gateway() else {
        eprintln!("SKIP: missing harness identity fixtures (cert.pem/key.pem)");
        return;
    };
    let proposal = gw
        .get_network("mychannel")
        .get_contract("basic")
        .new_proposal("CreateAsset")
        .with_arguments(["asset1", "blue", "20"])
        .build()
        .expect("build proposal");

    let bytes = proposal.bytes();
    std::fs::write(parity_dir().join("rust_proposal.bin"), &bytes).expect("write rust proposal");

    // Round-trip through our own parser as a sanity check.
    let reparsed = gw.new_proposal_from_bytes(&bytes).expect("reparse");
    assert_eq!(reparsed.transaction_id(), proposal.transaction_id());
    println!(
        "OK: Rust built proposal — txid={}",
        proposal.transaction_id()
    );
}
