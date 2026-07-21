// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Client identity and signing implementations.
//!
//! This mirrors `pkg/identity` from fabric-gateway-go. An [`Identity`] pairs an
//! MSP ID with X.509 credentials (a PEM-encoded certificate). A [`Signer`]
//! produces a digital signature over a pre-computed digest.

use crate::error::GatewayError;
use p256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A client identity used to interact with a Fabric network.
///
/// Equivalent to the Go `identity.Identity` interface backed by
/// `identity.X509Identity`.
#[derive(Clone, Debug)]
pub struct Identity {
    msp_id: String,
    /// X.509 certificate in PEM-encoded ASN.1 DER format.
    credentials: Vec<u8>,
}

impl Identity {
    /// Create an identity from an MSP ID and a PEM-encoded X.509 certificate.
    pub fn new(msp_id: impl Into<String>, certificate_pem: impl Into<Vec<u8>>) -> Self {
        Self {
            msp_id: msp_id.into(),
            credentials: certificate_pem.into(),
        }
    }

    /// Create an identity by reading the PEM-encoded X.509 certificate from a
    /// file — the companion to [`EcdsaP256Signer::from_file`].
    pub fn from_cert_file<P: AsRef<Path>>(
        msp_id: impl Into<String>,
        path: P,
    ) -> Result<Self, GatewayError> {
        let path = path.as_ref();
        let cert = std::fs::read(path).map_err(|e| {
            GatewayError::Identity(format!("failed to read cert file {}: {e}", path.display()))
        })?;
        Ok(Self::new(msp_id, cert))
    }

    /// ID of the Membership Service Provider to which this identity belongs.
    pub fn msp_id(&self) -> &str {
        &self.msp_id
    }

    /// Credentials as an X.509 certificate in PEM-encoded ASN.1 DER format.
    pub fn credentials(&self) -> &[u8] {
        &self.credentials
    }
}

/// Signs a pre-computed message digest, returning a signature.
///
/// Mirrors the Go `identity.Sign` function type. Implementations are expected to
/// operate on a digest (e.g. SHA-256 output), not the raw message.
pub trait Signer: Send + Sync {
    fn sign(&self, digest: &[u8]) -> Result<Vec<u8>, GatewayError>;
}

impl<F> Signer for F
where
    F: Fn(&[u8]) -> Result<Vec<u8>, GatewayError> + Send + Sync,
{
    fn sign(&self, digest: &[u8]) -> Result<Vec<u8>, GatewayError> {
        (self)(digest)
    }
}

/// An ECDSA P-256 signer that emits low-S, ASN.1 DER-encoded signatures — the
/// exact encoding Fabric requires and that fabric-gateway-go produces.
///
/// `p256`'s `SigningKey` already normalises S to the lower half of the curve
/// order and DER-encodes, so this matches `canonicalECDSASignatureSValue` +
/// `asn1.Marshal` in the Go implementation.
#[derive(Clone)]
pub struct EcdsaP256Signer {
    key: Arc<SigningKey>,
}

impl EcdsaP256Signer {
    /// Load a signer from a PEM-encoded EC (SEC1) or PKCS#8 private key.
    pub fn from_pem(pem: &str) -> Result<Self, GatewayError> {
        use p256::pkcs8::DecodePrivateKey;
        use sec1::DecodeEcPrivateKey;

        // Try SEC1 "EC PRIVATE KEY" first, then PKCS#8 "PRIVATE KEY".
        let key = SigningKey::from_sec1_pem(pem)
            .or_else(|_| SigningKey::from_pkcs8_pem(pem))
            .map_err(|e| GatewayError::Identity(format!("failed to parse EC private key: {e}")))?;
        Ok(Self { key: Arc::new(key) })
    }

    /// Load a signer from a DER-encoded EC (SEC1) or PKCS#8 private key.
    pub fn from_der(der: &[u8]) -> Result<Self, GatewayError> {
        use p256::pkcs8::DecodePrivateKey;
        use sec1::DecodeEcPrivateKey;

        let key = SigningKey::from_sec1_der(der)
            .or_else(|_| SigningKey::from_pkcs8_der(der))
            .map_err(|e| GatewayError::Identity(format!("failed to parse EC private key: {e}")))?;
        Ok(Self { key: Arc::new(key) })
    }

    /// Load a signer from a private-key file, auto-detecting PEM vs DER and
    /// SEC1 vs PKCS#8 encoding. This is the convenient entry point for keys on
    /// disk, including Fabric MSP `keystore` key files.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, GatewayError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| {
            GatewayError::Identity(format!("failed to read key file {}: {e}", path.display()))
        })?;

        // A PEM file is valid UTF-8 beginning with "-----BEGIN"; otherwise treat
        // the contents as DER.
        match std::str::from_utf8(&bytes) {
            Ok(text) if text.trim_start().starts_with("-----BEGIN") => Self::from_pem(text),
            _ => Self::from_der(&bytes),
        }
    }

    /// Load a signer from a Fabric MSP `keystore` directory. The directory is
    /// expected to contain exactly one private-key file (Fabric names it after
    /// the key's SKI, e.g. `<hex>_sk`); any single regular file is accepted.
    pub fn from_keystore_dir<P: AsRef<Path>>(dir: P) -> Result<Self, GatewayError> {
        let dir = dir.as_ref();
        let mut key_files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| {
                GatewayError::Identity(format!(
                    "failed to read keystore dir {}: {e}",
                    dir.display()
                ))
            })?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();

        match key_files.len() {
            0 => Err(GatewayError::Identity(format!(
                "no key file found in keystore dir {}",
                dir.display()
            ))),
            1 => Self::from_file(key_files.remove(0)),
            n => {
                // Deterministic choice + explicit error so an ambiguous keystore
                // never silently picks a random key.
                key_files.sort();
                Err(GatewayError::Identity(format!(
                    "expected exactly one key file in keystore dir {}, found {n}: {:?}",
                    dir.display(),
                    key_files
                )))
            }
        }
    }

    /// Build a signer from an existing `p256` signing key.
    pub fn from_signing_key(key: SigningKey) -> Self {
        Self { key: Arc::new(key) }
    }
}

impl Signer for EcdsaP256Signer {
    fn sign(&self, digest: &[u8]) -> Result<Vec<u8>, GatewayError> {
        // Fabric requires low-S (canonical) signatures and rejects high-S ones.
        // Explicitly normalise S to the lower half of the curve order — the
        // equivalent of Go's canonicalECDSASignatureSValue — then DER-encode
        // {r,s} for the wire.
        let sig: Signature = self
            .key
            .sign_prehash(digest)
            .map_err(|e| GatewayError::Signing(format!("ecdsa sign failed: {e}")))?;
        let sig = sig.normalize_s().unwrap_or(sig);
        Ok(sig.to_der().as_bytes().to_vec())
    }
}
