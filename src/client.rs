// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Thin async wrapper around the generated gRPC clients, mirroring the Go
//! `gatewayClient`. It maps transport errors into the staged [`GatewayError`]
//! variants (endorse/submit/commit-status) carrying the transaction ID.

use crate::error::{GatewayError, TransactionError};
use crate::pb::gateway::gateway_client::GatewayClient as RawGatewayClient;
use crate::pb::gateway::{
    CommitStatusResponse, EndorseRequest, EndorseResponse, EvaluateRequest, EvaluateResponse,
    SignedChaincodeEventsRequest, SignedCommitStatusRequest, SubmitRequest, SubmitResponse,
};
use crate::pb::protos::deliver_client::DeliverClient as RawDeliverClient;
use prost::Message;
use std::time::Duration;
use tonic::transport::Channel;
use tonic::Streaming;

/// Per-call default timeouts, mirroring the Go `contextFactory`.
#[derive(Clone, Default)]
pub(crate) struct Timeouts {
    pub evaluate: Option<Duration>,
    pub endorse: Option<Duration>,
    pub submit: Option<Duration>,
    pub commit_status: Option<Duration>,
}

#[derive(Clone)]
pub(crate) struct GrpcClient {
    gateway: RawGatewayClient<Channel>,
    deliver: RawDeliverClient<Channel>,
    pub timeouts: Timeouts,
}

impl GrpcClient {
    pub fn new(channel: Channel) -> Self {
        Self {
            gateway: RawGatewayClient::new(channel.clone()),
            deliver: RawDeliverClient::new(channel),
            timeouts: Timeouts::default(),
        }
    }

    fn request<T>(msg: T, timeout: Option<Duration>) -> tonic::Request<T> {
        let mut req = tonic::Request::new(msg);
        if let Some(t) = timeout {
            req.set_timeout(t);
        }
        req
    }

    pub async fn endorse(&self, req: EndorseRequest) -> Result<EndorseResponse, GatewayError> {
        let tx_id = req.transaction_id.clone();
        let mut c = self.gateway.clone();
        c.endorse(Self::request(req, self.timeouts.endorse))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| GatewayError::Endorse(TransactionError::from_status(s, tx_id)))
    }

    pub async fn submit(&self, req: SubmitRequest) -> Result<SubmitResponse, GatewayError> {
        let tx_id = req.transaction_id.clone();
        let mut c = self.gateway.clone();
        c.submit(Self::request(req, self.timeouts.submit))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| GatewayError::Submit(TransactionError::from_status(s, tx_id)))
    }

    pub async fn evaluate(&self, req: EvaluateRequest) -> Result<EvaluateResponse, GatewayError> {
        let tx_id = req.transaction_id.clone();
        let mut c = self.gateway.clone();
        c.evaluate(Self::request(req, self.timeouts.evaluate))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| GatewayError::Evaluate(TransactionError::from_status(s, tx_id)))
    }

    pub async fn commit_status(
        &self,
        req: SignedCommitStatusRequest,
    ) -> Result<CommitStatusResponse, GatewayError> {
        let tx_id = transaction_id_from_signed_commit(&req);
        let mut c = self.gateway.clone();
        c.commit_status(Self::request(req, self.timeouts.commit_status))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| GatewayError::CommitStatus(TransactionError::from_status(s, tx_id)))
    }

    pub async fn chaincode_events(
        &self,
        req: SignedChaincodeEventsRequest,
    ) -> Result<Streaming<crate::pb::gateway::ChaincodeEventsResponse>, GatewayError> {
        let mut c = self.gateway.clone();
        c.chaincode_events(tonic::Request::new(req))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| GatewayError::CommitStatus(TransactionError::from_status(s, "?")))
    }

    /// Open a Deliver stream (block events) by sending the seek envelope.
    pub async fn block_events(
        &self,
        envelope: crate::pb::common::Envelope,
    ) -> Result<Streaming<crate::pb::protos::DeliverResponse>, GatewayError> {
        let mut c = self.deliver.clone();
        let stream = tokio_stream::once(envelope);
        c.deliver(tonic::Request::new(stream))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| GatewayError::Connection(s.to_string()))
    }

    /// Open a DeliverFiltered stream by sending the seek envelope.
    pub async fn filtered_block_events(
        &self,
        envelope: crate::pb::common::Envelope,
    ) -> Result<Streaming<crate::pb::protos::DeliverResponse>, GatewayError> {
        let mut c = self.deliver.clone();
        let stream = tokio_stream::once(envelope);
        c.deliver_filtered(tonic::Request::new(stream))
            .await
            .map(|r| r.into_inner())
            .map_err(|s| GatewayError::Connection(s.to_string()))
    }
}

fn transaction_id_from_signed_commit(req: &SignedCommitStatusRequest) -> String {
    crate::pb::gateway::CommitStatusRequest::decode(req.request.as_slice())
        .map(|r| r.transaction_id)
        .unwrap_or_else(|_| "?".to_string())
}
