// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Internal signing identity — binds an [`Identity`], a [`Signer`] and a hash
//! function, mirroring the Go `signingIdentity` struct.

use crate::error::GatewayError;
use crate::hash::{self, Hash};
use crate::identity::{Identity, Signer};
use crate::pb::msp::SerializedIdentity;
use prost::Message;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct SigningIdentity {
    pub id: Identity,
    pub signer: Option<Arc<dyn Signer>>,
    pub hash: Hash,
}

impl SigningIdentity {
    pub fn new(id: Identity) -> Self {
        Self {
            id,
            signer: None,
            hash: hash::sha256,
        }
    }

    /// Hash a message to a digest for signing.
    pub fn hash(&self, message: &[u8]) -> Vec<u8> {
        (self.hash)(message)
    }

    /// Sign a pre-computed digest, or error if no signer was supplied — matching
    /// Go's default "no sign implementation supplied".
    pub fn sign(&self, digest: &[u8]) -> Result<Vec<u8>, GatewayError> {
        match &self.signer {
            Some(s) => s.sign(digest),
            None => Err(GatewayError::Signing(
                "no sign implementation supplied".to_string(),
            )),
        }
    }

    /// Serialised `msp.SerializedIdentity{mspid, id_bytes}` — the transaction
    /// creator bytes.
    pub fn creator(&self) -> Vec<u8> {
        let serialized = SerializedIdentity {
            mspid: self.id.msp_id().to_string(),
            id_bytes: self.id.credentials().to_vec(),
        };
        serialized.encode_to_vec()
    }
}
