// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Live smoke test: connect to a real Fabric peer's Gateway over TLS and
//! evaluate (and optionally submit) a chaincode transaction.
//!
//! Usage:
//!   cargo run --example query -- \
//!     --endpoint peer0.org1.example.com:7051 \
//!     --override peer0.org1.example.com \
//!     --tls-ca tlsca.pem \
//!     --cert client-cert.pem --key client-key.pem \
//!     --msp Org1MSP --channel mychannel --chaincode basic \
//!     [--submit] --fn GetAllAssets [args...]

use fabric_gateway::identity::EcdsaP256Signer;
use fabric_gateway::{Gateway, Identity};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};

fn arg(map: &HashMap<String, String>, key: &str) -> String {
    map.get(key)
        .unwrap_or_else(|| panic!("missing --{key}"))
        .clone()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut opts: HashMap<String, String> = HashMap::new();
    let mut positional: Vec<String> = Vec::new();
    let mut submit = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--submit" => submit = true,
            s if s.starts_with("--") => {
                let key = s.trim_start_matches("--").to_string();
                let val = raw.get(i + 1).cloned().unwrap_or_default();
                opts.insert(key, val);
                i += 1;
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    let endpoint = arg(&opts, "endpoint");
    let tls_ca = std::fs::read(arg(&opts, "tls-ca"))?;
    let msp = arg(&opts, "msp");
    let channel_name = arg(&opts, "channel");
    let chaincode = arg(&opts, "chaincode");
    let function = arg(&opts, "fn");

    // TLS to the peer gateway using the peer's TLS CA. The optional --override
    // sets the SNI/authority to match the peer certificate's SANs.
    let mut tls = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(&tls_ca));
    if let Some(name) = opts.get("override") {
        tls = tls.domain_name(name.clone());
    }

    let uri = format!("https://{endpoint}");
    println!("Connecting to {uri} (TLS) ...");
    let channel = Channel::from_shared(uri)?
        .tls_config(tls)?
        .timeout(Duration::from_secs(20))
        .connect()
        .await?;
    println!("gRPC channel established.");

    // Load the identity certificate and private key straight from disk. --key
    // accepts a PEM or DER key file; use --keystore to point at a Fabric MSP
    // keystore directory instead.
    let identity = Identity::from_cert_file(msp, arg(&opts, "cert"))?;
    let signer = Arc::new(match (opts.get("key"), opts.get("keystore")) {
        (Some(key_path), _) => EcdsaP256Signer::from_file(key_path)?,
        (None, Some(dir)) => EcdsaP256Signer::from_keystore_dir(dir)?,
        (None, None) => return Err("provide --key <file> or --keystore <dir>".into()),
    });
    let gateway = Gateway::builder(identity, channel)
        .with_sign(signer)
        .with_evaluate_timeout(Duration::from_secs(15))
        .with_endorse_timeout(Duration::from_secs(30))
        .with_submit_timeout(Duration::from_secs(15))
        .with_commit_status_timeout(Duration::from_secs(60))
        .connect()?;

    let contract = gateway.get_network(&channel_name).get_contract(&chaincode);
    let args: Vec<&str> = positional.iter().map(|s| s.as_str()).collect();

    if submit {
        println!("Submitting {function}({args:?}) ...");
        let result = contract.submit_transaction(&function, &args).await?;
        println!(
            "Submit committed. Result: {}",
            String::from_utf8_lossy(&result)
        );
    } else {
        println!("Evaluating {function}({args:?}) ...");
        let result = contract.evaluate_transaction(&function, &args).await?;
        println!("Evaluate result: {}", String::from_utf8_lossy(&result));
    }
    Ok(())
}
