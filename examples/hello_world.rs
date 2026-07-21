// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! The smallest possible fabric-gateway program: connect to a peer's Gateway
//! over TLS and evaluate a single query.
//!
//! Configure it with environment variables, then run:
//!
//! ```text
//! FABRIC_ENDPOINT=peer0.org1.example.com:7051 \
//! FABRIC_OVERRIDE=peer0.org1.example.com \
//! FABRIC_TLS_CA=tlsca.pem \
//! FABRIC_CERT=client-cert.pem \
//! FABRIC_KEY=client-key.pem \
//! FABRIC_MSP=Org1MSP \
//! FABRIC_CHANNEL=mychannel \
//! FABRIC_CHAINCODE=basic \
//!   cargo run --example hello_world
//! ```

use fabric_gateway::identity::EcdsaP256Signer;
use fabric_gateway::{Gateway, Identity};
use std::sync::Arc;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("set the {key} environment variable"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Build a TLS gRPC channel to the gateway peer.
    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(std::fs::read(env("FABRIC_TLS_CA"))?))
        .domain_name(env("FABRIC_OVERRIDE"));
    let channel = Channel::from_shared(format!("https://{}", env("FABRIC_ENDPOINT")))?
        .tls_config(tls)?
        .connect()
        .await?;

    // 2. Load the client identity and signer from disk.
    let identity = Identity::from_cert_file(env("FABRIC_MSP"), env("FABRIC_CERT"))?;
    let signer = Arc::new(EcdsaP256Signer::from_file(env("FABRIC_KEY"))?);

    // 3. Connect and get a contract handle.
    let gateway = Gateway::connect(identity, signer, channel)?;
    let contract = gateway
        .get_network(&env("FABRIC_CHANNEL"))
        .get_contract(&env("FABRIC_CHAINCODE"));

    // 4. Evaluate a query and print the result.
    let result = contract.evaluate_transaction("GetAllAssets", &[]).await?;
    println!("{}", String::from_utf8_lossy(&result));
    Ok(())
}
