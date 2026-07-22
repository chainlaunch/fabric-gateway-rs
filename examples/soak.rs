// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Soak test: hammer a live gateway with many query and submit cycles and
//! assert zero failures. Proves the client works *reliably*, not just once, and
//! (when RSS is sampled externally) that it does not leak memory over a long run.
//!
//! Uses the same FABRIC_* environment variables as the other examples, plus:
//!   SOAK_EVALUATES (default 500) — number of query iterations
//!   SOAK_SUBMITS   (default 50)  — number of submit+commit iterations
//!
//! ```text
//! FABRIC_ENDPOINT=... FABRIC_OVERRIDE=... FABRIC_TLS_CA=... FABRIC_CERT=... \
//! FABRIC_KEY=... FABRIC_MSP=... FABRIC_CHANNEL=... FABRIC_CHAINCODE=... \
//! SOAK_EVALUATES=500 SOAK_SUBMITS=50 cargo run --release --example soak
//! ```

use fabric_gateway::identity::EcdsaP256Signer;
use fabric_gateway::{Gateway, Identity};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tonic::transport::{Certificate, Channel, ClientTlsConfig};

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("set {key}"))
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
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
    let gateway = Gateway::builder(identity, channel)
        .with_endorse_timeout(Duration::from_secs(30))
        .with_submit_timeout(Duration::from_secs(30))
        .with_commit_status_timeout(Duration::from_secs(120))
        .with_sign(signer)
        .connect()?;

    let contract = gateway
        .get_network(&env("FABRIC_CHANNEL"))
        .get_contract(&env("FABRIC_CHAINCODE"));

    let n_eval = env_usize("SOAK_EVALUATES", 500);
    let n_submit = env_usize("SOAK_SUBMITS", 50);
    println!(
        "PID {} — soak: {n_eval} evaluates, {n_submit} submits",
        std::process::id()
    );

    let start = Instant::now();
    let mut failures = 0usize;

    // Query loop.
    for i in 0..n_eval {
        match contract.evaluate_transaction("GetAllAssets", &[]).await {
            Ok(_) => {}
            Err(e) => {
                failures += 1;
                eprintln!("evaluate #{i} failed: {e}");
            }
        }
        if i > 0 && i % 100 == 0 {
            println!("  evaluates: {i}/{n_eval} ({} failures)", failures);
        }
    }

    // Submit + read-back loop.
    for i in 0..n_submit {
        let id = format!("soak-{}-{i}", std::process::id());
        let size = (i % 50 + 1).to_string();
        let val = (i * 7 + 1).to_string();
        let args = [id.as_str(), "cyan", size.as_str(), "soak", val.as_str()];
        if let Err(e) = contract.submit_transaction("CreateAsset", &args).await {
            failures += 1;
            eprintln!("submit #{i} failed: {e}");
            continue;
        }
        match contract.evaluate_transaction("ReadAsset", &[&id]).await {
            Ok(bytes) => {
                let s = String::from_utf8_lossy(&bytes);
                if !s.contains(&id) || !s.contains(&val) {
                    failures += 1;
                    eprintln!("read-back #{i} mismatch: {s}");
                }
            }
            Err(e) => {
                failures += 1;
                eprintln!("read-back #{i} failed: {e}");
            }
        }
        if i > 0 && i % 10 == 0 {
            println!("  submits: {i}/{n_submit} ({} failures)", failures);
        }
    }

    let elapsed = start.elapsed();
    let total = n_eval + n_submit;
    println!(
        "SOAK DONE: {total} operations in {:.1}s, {failures} failures",
        elapsed.as_secs_f64()
    );
    if failures > 0 {
        std::process::exit(1);
    }
    println!("SOAK OK: zero failures");
    Ok(())
}
