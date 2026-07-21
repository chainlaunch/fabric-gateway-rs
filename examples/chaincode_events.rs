// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Stream chaincode events from a Fabric network.
//!
//! Connects to a peer's Gateway, subscribes to events emitted by a chaincode,
//! and prints each one as it is committed. Uses the same environment variables
//! as the `hello_world` example.
//!
//! ```text
//! FABRIC_ENDPOINT=peer0.org1.example.com:7051 \
//! FABRIC_OVERRIDE=peer0.org1.example.com \
//! FABRIC_TLS_CA=tlsca.pem FABRIC_CERT=client-cert.pem FABRIC_KEY=client-key.pem \
//! FABRIC_MSP=Org1MSP FABRIC_CHANNEL=mychannel FABRIC_CHAINCODE=basic \
//!   cargo run --example chaincode_events
//! ```

use fabric_gateway::identity::EcdsaP256Signer;
use fabric_gateway::{Gateway, Identity};
use std::sync::Arc;
use tokio_stream::StreamExt;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("set the {key} environment variable"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(std::fs::read(env("FABRIC_TLS_CA"))?))
        .domain_name(env("FABRIC_OVERRIDE"));
    let channel = Channel::from_shared(format!("https://{}", env("FABRIC_ENDPOINT")))?
        .tls_config(tls)?
        .connect()
        .await?;

    let identity = Identity::from_cert_file(env("FABRIC_MSP"), env("FABRIC_CERT"))?;
    let signer = Arc::new(EcdsaP256Signer::from_file(env("FABRIC_KEY"))?);
    let gateway = Gateway::connect(identity, signer, channel)?;

    let chaincode = env("FABRIC_CHAINCODE");
    let network = gateway.get_network(&env("FABRIC_CHANNEL"));

    // Subscribe from the next committed block. Use `.with_start_block(n)` to
    // replay history, or add `.with_after_transaction_id(id)` to resume.
    let request = network.new_chaincode_events_request(&chaincode).build();
    let mut events = request.events().await?;

    println!("Listening for events from '{chaincode}' (Ctrl-C to stop)...");
    while let Some(event) = events.next().await {
        println!(
            "block {} | tx {} | {}::{} | {}",
            event.block_number,
            event.transaction_id,
            event.chaincode_name,
            event.event_name,
            String::from_utf8_lossy(&event.payload),
        );
    }
    Ok(())
}
