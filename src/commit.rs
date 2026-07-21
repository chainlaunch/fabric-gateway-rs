// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Commit status of a submitted transaction. Ports `commit.go`.

use crate::client::GrpcClient;
use crate::error::GatewayError;
use crate::pb::gateway::SignedCommitStatusRequest;
use crate::pb::protos::TxValidationCode;
use crate::signing::SigningIdentity;
use prost::Message;

/// Provides access to the commit status of a transaction.
pub struct Commit {
    client: GrpcClient,
    signing_id: SigningIdentity,
    transaction_id: String,
    signed_request: SignedCommitStatusRequest,
}

/// Status of a committed transaction.
#[derive(Debug, Clone)]
pub struct Status {
    pub code: TxValidationCode,
    pub successful: bool,
    pub transaction_id: String,
    pub block_number: u64,
}

impl Commit {
    pub(crate) fn new(
        client: GrpcClient,
        signing_id: SigningIdentity,
        transaction_id: String,
        signed_request: SignedCommitStatusRequest,
    ) -> Self {
        Self {
            client,
            signing_id,
            transaction_id,
            signed_request,
        }
    }

    /// Serialised `SignedCommitStatusRequest` bytes (off-line signing).
    pub fn bytes(&self) -> Vec<u8> {
        self.signed_request.encode_to_vec()
    }

    /// Digest of the commit-status request, for off-line signing.
    pub fn digest(&self) -> Vec<u8> {
        self.signing_id.hash(&self.signed_request.request)
    }

    /// Transaction ID.
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(crate) fn set_signature(&mut self, signature: Vec<u8>) {
        self.signed_request.signature = signature;
    }

    fn is_signed(&self) -> bool {
        !self.signed_request.signature.is_empty()
    }

    fn sign(&mut self) -> Result<(), GatewayError> {
        if self.is_signed() {
            return Ok(());
        }
        let digest = self.digest();
        let signature = self.signing_id.sign(&digest)?;
        self.set_signature(signature);
        Ok(())
    }

    /// Get the commit status, blocking until the transaction commits.
    pub async fn status(mut self) -> Result<Status, GatewayError> {
        self.sign()?;
        let response = self
            .client
            .commit_status(self.signed_request.clone())
            .await?;

        let code = TxValidationCode::try_from(response.result)
            .unwrap_or(TxValidationCode::InvalidOtherReason);
        Ok(Status {
            code,
            successful: code == TxValidationCode::Valid,
            transaction_id: self.transaction_id.clone(),
            block_number: response.block_number,
        })
    }
}
