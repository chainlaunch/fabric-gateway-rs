// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Gateway connection and builder. Ports `gateway.go`.

use crate::client::{GrpcClient, Timeouts};
use crate::error::GatewayError;
use crate::hash::Hash;
use crate::identity::{Identity, Signer};
use crate::network::Network;
use crate::signing::SigningIdentity;
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::Channel;

/// A connection of a specific client identity to a Fabric Gateway.
///
/// Build one with [`Gateway::connect`] (or [`Gateway::builder`]) supplying an
/// [`Identity`], a [`Signer`], and a gRPC [`Channel`] to the gateway peer.
pub struct Gateway {
    signing_id: SigningIdentity,
    client: GrpcClient,
    tls_certificate_hash: Vec<u8>,
}

impl Gateway {
    /// Start building a gateway connection.
    pub fn builder(identity: Identity, channel: Channel) -> GatewayBuilder {
        GatewayBuilder {
            signing_id: SigningIdentity::new(identity),
            channel,
            timeouts: Timeouts::default(),
            tls_certificate_hash: Vec::new(),
        }
    }

    /// Convenience connect with a signer and default options.
    pub fn connect(
        identity: Identity,
        signer: Arc<dyn Signer>,
        channel: Channel,
    ) -> Result<Self, GatewayError> {
        Gateway::builder(identity, channel)
            .with_sign(signer)
            .connect()
    }

    /// The identity used by this gateway.
    pub fn identity(&self) -> &Identity {
        &self.signing_id.id
    }

    /// Return a [`Network`] for the named Fabric channel.
    pub fn get_network(&self, name: &str) -> Network {
        Network {
            client: self.client.clone(),
            signing_id: self.signing_id.clone(),
            name: name.to_string(),
            tls_certificate_hash: self.tls_certificate_hash.clone(),
        }
    }

    /// Recreate a [`crate::Proposal`] from its serialised `ProposedTransaction`
    /// bytes (off-line signing flow). Mirrors Go `Gateway.NewProposal`.
    pub fn new_proposal_from_bytes(&self, bytes: &[u8]) -> Result<crate::Proposal, GatewayError> {
        use crate::pb::common::{ChannelHeader, Header};
        use crate::pb::gateway::ProposedTransaction;
        use crate::pb::protos::Proposal as PbProposal;
        use prost::Message;

        let proposed = ProposedTransaction::decode(bytes).map_err(|e| {
            GatewayError::Protobuf(format!("failed to deserialize proposed transaction: {e}"))
        })?;
        let proposal_bytes = proposed
            .proposal
            .as_ref()
            .map(|p| p.proposal_bytes.as_slice())
            .unwrap_or_default();
        let proposal = PbProposal::decode(proposal_bytes)
            .map_err(|e| GatewayError::Protobuf(format!("failed to deserialize proposal: {e}")))?;
        let header = Header::decode(proposal.header.as_slice())
            .map_err(|e| GatewayError::Protobuf(format!("failed to deserialize header: {e}")))?;
        let channel_header =
            ChannelHeader::decode(header.channel_header.as_slice()).map_err(|e| {
                GatewayError::Protobuf(format!("failed to deserialize channel header: {e}"))
            })?;

        Ok(crate::Proposal::from_parts(
            self.client.clone(),
            self.signing_id.clone(),
            channel_header.channel_id,
            proposed,
        ))
    }

    /// Recreate a signed [`crate::Proposal`] from serialised bytes and an
    /// externally-produced signature. Mirrors Go `Gateway.NewSignedProposal`.
    pub fn new_signed_proposal(
        &self,
        bytes: &[u8],
        signature: Vec<u8>,
    ) -> Result<crate::Proposal, GatewayError> {
        let mut proposal = self.new_proposal_from_bytes(bytes)?;
        proposal.set_signature(signature);
        Ok(proposal)
    }
}

/// Builder for a [`Gateway`], mirroring the Go `ConnectOption` functions.
pub struct GatewayBuilder {
    signing_id: SigningIdentity,
    channel: Channel,
    timeouts: Timeouts,
    tls_certificate_hash: Vec<u8>,
}

impl GatewayBuilder {
    /// Use the supplied signing implementation.
    pub fn with_sign(mut self, signer: Arc<dyn Signer>) -> Self {
        self.signing_id.signer = Some(signer);
        self
    }

    /// Use a custom hash implementation (default: SHA-256).
    pub fn with_hash(mut self, hash: Hash) -> Self {
        self.signing_id.hash = hash;
        self
    }

    /// Default timeout for evaluating (query) transactions.
    pub fn with_evaluate_timeout(mut self, timeout: Duration) -> Self {
        self.timeouts.evaluate = Some(timeout);
        self
    }

    /// Default timeout for endorsements.
    pub fn with_endorse_timeout(mut self, timeout: Duration) -> Self {
        self.timeouts.endorse = Some(timeout);
        self
    }

    /// Default timeout for submitting to the orderer.
    pub fn with_submit_timeout(mut self, timeout: Duration) -> Self {
        self.timeouts.submit = Some(timeout);
        self
    }

    /// Default timeout for retrieving commit status.
    pub fn with_commit_status_timeout(mut self, timeout: Duration) -> Self {
        self.timeouts.commit_status = Some(timeout);
        self
    }

    /// SHA-256 hash of the TLS client certificate, required only for mutual-TLS
    /// gRPC connections to the gateway peer.
    pub fn with_tls_client_certificate_hash(mut self, hash: Vec<u8>) -> Self {
        self.tls_certificate_hash = hash;
        self
    }

    /// Finalise the connection.
    pub fn connect(self) -> Result<Gateway, GatewayError> {
        let mut client = GrpcClient::new(self.channel);
        client.timeouts = self.timeouts;
        Ok(Gateway {
            signing_id: self.signing_id,
            client,
            tls_certificate_hash: self.tls_certificate_hash,
        })
    }
}
