// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Hash implementations used to create digests for digital signatures.
//!
//! Mirrors `pkg/hash` from fabric-gateway-go. The default is [`sha256`].

use sha2::{Digest, Sha256, Sha384};

/// Hash function type: maps a message to a digest.
pub type Hash = fn(&[u8]) -> Vec<u8>;

/// Returns the input message unchanged. Used when the signer needs the full
/// message (e.g. Ed25519).
pub fn none(message: &[u8]) -> Vec<u8> {
    message.to_vec()
}

/// SHA-256 digest — the default hash for P-256 signing.
pub fn sha256(message: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(message);
    hasher.finalize().to_vec()
}

/// SHA-384 digest — typically paired with P-384 signing.
pub fn sha384(message: &[u8]) -> Vec<u8> {
    let mut hasher = Sha384::new();
    hasher.update(message);
    hasher.finalize().to_vec()
}
