// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Error types mirroring fabric-gateway-go's `client` error hierarchy.
//!
//! The Go library distinguishes the transaction-flow stage at which a failure
//! occurs: [`GatewayError::Endorse`], [`GatewayError::Submit`],
//! [`GatewayError::CommitStatus`] (all gRPC status errors carrying the
//! transaction ID and any [`ErrorDetail`]s), and [`GatewayError::Commit`] for a
//! transaction that is delivered but validated as invalid.

use crate::pb::gateway::ErrorDetail;
use crate::pb::protos::TxValidationCode;

/// The error type returned across the Fabric Gateway client.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// Failure endorsing a transaction proposal (Endorse RPC).
    #[error("endorse error: {0}")]
    Endorse(Box<TransactionError>),

    /// Failure submitting an endorsed transaction to the orderer (Submit RPC).
    #[error("submit error: {0}")]
    Submit(Box<TransactionError>),

    /// Failure obtaining the commit status of a transaction (CommitStatus RPC).
    #[error("commit status error: {0}")]
    CommitStatus(Box<TransactionError>),

    /// Failure evaluating a transaction proposal (Evaluate RPC).
    #[error("evaluate error: {0}")]
    Evaluate(Box<TransactionError>),

    /// A transaction that was delivered to the ledger but validated as invalid.
    #[error("transaction {transaction_id} failed to commit with status code {} ({:?})", *.code as i32, .code)]
    Commit {
        transaction_id: String,
        code: TxValidationCode,
    },

    /// Failure serialising/deserialising a protobuf message.
    #[error("protobuf error: {0}")]
    Protobuf(String),

    /// Failure related to identity handling (bad cert/key material).
    #[error("identity error: {0}")]
    Identity(String),

    /// Failure producing a digital signature.
    #[error("signing error: {0}")]
    Signing(String),

    /// Failure establishing or configuring the gRPC connection.
    #[error("connection error: {0}")]
    Connection(String),

    /// A required connection detail was not supplied when connecting.
    #[error("{0}")]
    InvalidArgument(String),
}

/// A gRPC status error tied to a specific transaction, carrying any structured
/// [`ErrorDetail`]s returned by endorsing peers or ordering nodes.
///
/// Mirrors the Go `TransactionError`.
#[derive(Debug)]
pub struct TransactionError {
    pub transaction_id: String,
    pub status: tonic::Status,
    pub details: Vec<ErrorDetail>,
}

impl TransactionError {
    pub(crate) fn from_status(
        status: tonic::Status,
        transaction_id: impl Into<String>,
    ) -> Box<Self> {
        let details = decode_error_details(&status);
        Box::new(Self {
            transaction_id: transaction_id.into(),
            status,
            details,
        })
    }
}

impl std::fmt::Display for TransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.status.message())?;
        if self.details.is_empty() {
            return Ok(());
        }
        write!(f, "\nDetails:")?;
        for d in &self.details {
            write!(
                f,
                "\n  - Address: {}\n    MspId: {}\n    Message: {}",
                d.address, d.msp_id, d.message
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for TransactionError {}

/// Minimal `google.rpc.Status` for decoding the packed status details. The
/// gateway packs `gateway.ErrorDetail` messages into `Status.details` as
/// `google.protobuf.Any`.
#[derive(Clone, PartialEq, prost::Message)]
struct RpcStatus {
    #[prost(int32, tag = "1")]
    code: i32,
    #[prost(string, tag = "2")]
    message: String,
    #[prost(message, repeated, tag = "3")]
    details: Vec<prost_types::Any>,
}

/// Decode `gateway.ErrorDetail` messages packed into a gRPC status's
/// `google.rpc.Status.details`. Mirrors Go's `grpcError.Details()`.
pub(crate) fn decode_error_details(status: &tonic::Status) -> Vec<ErrorDetail> {
    use prost::Message;

    // tonic exposes the status-details bytes as a serialized google.rpc.Status.
    let raw = status.details();
    if raw.is_empty() {
        return Vec::new();
    }
    let Ok(rpc_status) = RpcStatus::decode(raw) else {
        return Vec::new();
    };

    let type_url = "type.googleapis.com/gateway.ErrorDetail";
    rpc_status
        .details
        .into_iter()
        .filter(|any| any.type_url == type_url)
        .filter_map(|any| ErrorDetail::decode(any.value.as_slice()).ok())
        .collect()
}
