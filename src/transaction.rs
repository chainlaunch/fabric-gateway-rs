// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Endorsed transaction: submit to the orderer and obtain a [`Commit`]. Ports
//! `transaction.go` and `transactionparser.go`.

use crate::client::GrpcClient;
use crate::commit::Commit;
use crate::error::GatewayError;
use crate::pb::common::{ChannelHeader, Envelope, Header, Payload};
use crate::pb::gateway::{
    CommitStatusRequest, PreparedTransaction, SignedCommitStatusRequest, SubmitRequest,
};
use crate::pb::protos::{
    ChaincodeAction, ChaincodeActionPayload, ProposalResponsePayload, Transaction as PbTransaction,
};
use crate::signing::SigningIdentity;
use prost::Message;

/// An endorsed transaction, ready to submit to the ordering service.
pub struct Transaction {
    client: GrpcClient,
    signing_id: SigningIdentity,
    channel_id: String,
    prepared: PreparedTransaction,
    result: Vec<u8>,
}

impl Transaction {
    pub(crate) fn new(
        client: GrpcClient,
        signing_id: SigningIdentity,
        prepared: PreparedTransaction,
    ) -> Result<Self, GatewayError> {
        let envelope = prepared
            .envelope
            .as_ref()
            .ok_or_else(|| GatewayError::Protobuf("prepared transaction has no envelope".into()))?;
        let (channel_id, result) = parse_transaction_envelope(envelope)?;
        Ok(Self {
            client,
            signing_id,
            channel_id,
            prepared,
            result,
        })
    }

    /// The result of the proposed transaction invocation (available before
    /// commit).
    pub fn result(&self) -> &[u8] {
        &self.result
    }

    /// Serialised `PreparedTransaction` bytes (off-line signing).
    pub fn bytes(&self) -> Vec<u8> {
        self.prepared.encode_to_vec()
    }

    /// Digest of the transaction envelope payload, for off-line signing.
    pub fn digest(&self) -> Vec<u8> {
        let payload = self
            .prepared
            .envelope
            .as_ref()
            .map(|e| e.payload.as_slice())
            .unwrap_or_default();
        self.signing_id.hash(payload)
    }

    /// Transaction ID.
    pub fn transaction_id(&self) -> &str {
        &self.prepared.transaction_id
    }

    pub(crate) fn set_signature(&mut self, signature: Vec<u8>) {
        if let Some(e) = self.prepared.envelope.as_mut() {
            e.signature = signature;
        }
    }

    fn is_signed(&self) -> bool {
        self.prepared
            .envelope
            .as_ref()
            .map(|e| !e.signature.is_empty())
            .unwrap_or(false)
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

    /// Submit the transaction to the orderer, returning a [`Commit`] to await
    /// ledger commit.
    pub async fn submit(mut self) -> Result<Commit, GatewayError> {
        self.sign()?;

        // Build the commit-status request before submit, matching Go's ordering.
        let status_request = self.new_signed_commit_status_request();

        let submit_request = SubmitRequest {
            transaction_id: self.prepared.transaction_id.clone(),
            channel_id: self.channel_id.clone(),
            prepared_transaction: self.prepared.envelope.clone(),
        };
        self.client.submit(submit_request).await?;

        Ok(Commit::new(
            self.client,
            self.signing_id,
            self.prepared.transaction_id,
            status_request,
        ))
    }

    fn new_signed_commit_status_request(&self) -> SignedCommitStatusRequest {
        let creator = self.signing_id.creator();
        let request = CommitStatusRequest {
            channel_id: self.channel_id.clone(),
            transaction_id: self.prepared.transaction_id.clone(),
            identity: creator,
        };
        SignedCommitStatusRequest {
            request: request.encode_to_vec(),
            signature: Vec::new(),
        }
    }
}

/// Extract the channel name and chaincode result payload from an endorsed
/// transaction envelope. Ports `parseTransactionEnvelope`.
fn parse_transaction_envelope(envelope: &Envelope) -> Result<(String, Vec<u8>), GatewayError> {
    let payload = Payload::decode(envelope.payload.as_slice())
        .map_err(|e| GatewayError::Protobuf(format!("failed to deserialize payload: {e}")))?;

    let channel_name = parse_channel_name_from_header(payload.header.as_ref())?;
    let result = parse_result_from_payload(&payload)?;
    Ok((channel_name, result))
}

fn parse_channel_name_from_header(header: Option<&Header>) -> Result<String, GatewayError> {
    let header = header.ok_or_else(|| GatewayError::Protobuf("payload has no header".into()))?;
    let channel_header = ChannelHeader::decode(header.channel_header.as_slice()).map_err(|e| {
        GatewayError::Protobuf(format!("failed to deserialize channel header: {e}"))
    })?;
    Ok(channel_header.channel_id)
}

fn parse_result_from_payload(payload: &Payload) -> Result<Vec<u8>, GatewayError> {
    let transaction = PbTransaction::decode(payload.data.as_slice())
        .map_err(|e| GatewayError::Protobuf(format!("failed to deserialize transaction: {e}")))?;

    let mut errors = Vec::new();
    for action in &transaction.actions {
        match parse_result_from_transaction_action(&action.payload) {
            Ok(result) => return Ok(result),
            Err(e) => errors.push(e.to_string()),
        }
    }
    Err(GatewayError::Protobuf(format!(
        "no proposal response found: {errors:?}"
    )))
}

fn parse_result_from_transaction_action(
    action_payload_bytes: &[u8],
) -> Result<Vec<u8>, GatewayError> {
    let action_payload = ChaincodeActionPayload::decode(action_payload_bytes).map_err(|e| {
        GatewayError::Protobuf(format!(
            "failed to deserialize chaincode action payload: {e}"
        ))
    })?;
    let prp_bytes = action_payload
        .action
        .as_ref()
        .map(|a| a.proposal_response_payload.as_slice())
        .unwrap_or_default();
    let response_payload = ProposalResponsePayload::decode(prp_bytes).map_err(|e| {
        GatewayError::Protobuf(format!(
            "failed to deserialize proposal response payload: {e}"
        ))
    })?;
    let chaincode_action =
        ChaincodeAction::decode(response_payload.extension.as_slice()).map_err(|e| {
            GatewayError::Protobuf(format!("failed to deserialize chaincode action: {e}"))
        })?;
    Ok(chaincode_action
        .response
        .map(|r| r.payload)
        .unwrap_or_default())
}
