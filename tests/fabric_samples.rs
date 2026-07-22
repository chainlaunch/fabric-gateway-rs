// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Live integration test against a real Hyperledger Fabric network.
//!
//! This runs only when `FABRIC_IT=1` is set (the CI integration workflow sets it
//! after standing up the official `fabric-samples` test network). On a normal
//! `cargo test` it skips cleanly, so contributors don't need a running network.
//!
//! It drives the full lifecycle through this crate against the standard
//! `asset-transfer-basic` chaincode: evaluate (query), submit (endorse → order →
//! commit), and read-back — asserting the committed state matches what we wrote.
//!
//! Configuration (all from environment, matching the examples):
//!   FABRIC_ENDPOINT, FABRIC_OVERRIDE, FABRIC_TLS_CA, FABRIC_CERT, FABRIC_KEY,
//!   FABRIC_MSP, FABRIC_CHANNEL, FABRIC_CHAINCODE, and optional FABRIC_ASSET_ID.

use fabric_gateway::identity::EcdsaP256Signer;
use fabric_gateway::{Gateway, Identity};
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("integration test needs {key}"))
}

async fn connect() -> Result<Gateway, Box<dyn std::error::Error>> {
    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(std::fs::read(env("FABRIC_TLS_CA"))?))
        .domain_name(env("FABRIC_OVERRIDE"));
    let channel = Channel::from_shared(format!("https://{}", env("FABRIC_ENDPOINT")))?
        .tls_config(tls)?
        .timeout(Duration::from_secs(30))
        .connect()
        .await?;

    let identity = Identity::from_cert_file(env("FABRIC_MSP"), env("FABRIC_CERT"))?;
    let signer = Arc::new(EcdsaP256Signer::from_file(env("FABRIC_KEY"))?);
    Ok(Gateway::builder(identity, channel)
        .with_sign(signer)
        .with_endorse_timeout(Duration::from_secs(30))
        .with_submit_timeout(Duration::from_secs(30))
        .with_commit_status_timeout(Duration::from_secs(120))
        .connect()?)
}

#[tokio::test]
async fn full_lifecycle_against_fabric_samples() {
    if std::env::var("FABRIC_IT").as_deref() != Ok("1") {
        eprintln!("SKIP: set FABRIC_IT=1 with a running fabric-samples network to run this");
        return;
    }

    let gateway = connect().await.expect("connect to gateway");
    let contract = gateway
        .get_network(&env("FABRIC_CHANNEL"))
        .get_contract(&env("FABRIC_CHAINCODE"));

    // 1. Query — must succeed and return the seeded ledger.
    let all = contract
        .evaluate_transaction("GetAllAssets", &[])
        .await
        .expect("GetAllAssets");
    println!("GetAllAssets -> {}", String::from_utf8_lossy(&all));

    // 2. Submit a new asset (endorse → order → commit), then read it back.
    let id = std::env::var("FABRIC_ASSET_ID").unwrap_or_else(|_| "rust-it-asset".to_string());
    contract
        .submit_transaction(
            "CreateAsset",
            &[&id, "magenta", "11", "rust-gateway", "424"],
        )
        .await
        .expect("CreateAsset submit+commit");

    let read = contract
        .evaluate_transaction("ReadAsset", &[&id])
        .await
        .expect("ReadAsset");
    let read = String::from_utf8_lossy(&read);
    println!("ReadAsset({id}) -> {read}");

    // 3. Assert the committed state matches what we submitted.
    assert!(read.contains(&id), "asset id not found in read-back");
    assert!(read.contains("magenta"), "color not persisted");
    assert!(read.contains("rust-gateway"), "owner not persisted");
    assert!(read.contains("424"), "appraisedValue not persisted");

    println!("OK: full submit+commit+read-back lifecycle verified on live Fabric");
}
