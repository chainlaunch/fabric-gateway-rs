// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Sign a hex-encoded digest with the parity test key and print the DER
//! signature as hex. Used by the parity runner to feed a Rust-produced
//! signature to the Go verifier.
//!
//! Usage: cargo run --example sign_digest -- <digest-hex>

use fabric_gateway::identity::{EcdsaP256Signer, Signer};

fn main() {
    let digest_hex = std::env::args()
        .nth(1)
        .expect("usage: sign_digest <digest-hex>");
    let digest = hex::decode(digest_hex.trim()).expect("valid hex digest");

    let dir = std::env::var("PARITY_DIR").unwrap_or_else(|_| "parity-harness".to_string());
    let key = std::fs::read_to_string(format!("{dir}/key.pem")).expect("key.pem fixture");
    let signer = EcdsaP256Signer::from_pem(&key).expect("parse key");

    let sig = signer.sign(&digest).expect("sign");
    println!("{}", hex::encode(sig));
}
