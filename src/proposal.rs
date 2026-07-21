// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Transaction proposal construction, endorsement and evaluation.
//!
//! Ports `proposalbuilder.go`, `proposal.go` and `transactioncontext.go`. The
//! byte layout here must match fabric-gateway-go exactly, since the peer
//! validates signatures over these serialised structures.

use crate::client::GrpcClient;
use crate::error::GatewayError;
use crate::hash;
use crate::pb::common::{ChannelHeader, Header, HeaderType, SignatureHeader};
use crate::pb::gateway::{
    EndorseRequest, EvaluateRequest, PreparedTransaction, ProposedTransaction,
};
use crate::pb::protos::{
    ChaincodeHeaderExtension, ChaincodeId, ChaincodeInput, ChaincodeInvocationSpec,
    ChaincodeProposalPayload, ChaincodeSpec, Proposal as PbProposal, SignedProposal,
};
use crate::signing::SigningIdentity;
use crate::transaction::Transaction;
use crate::util::now_timestamp;
use prost::Message;
use rand_core::{OsRng, RngCore};
use std::collections::HashMap;

/// Transaction context: a fresh nonce, derived transaction ID, and the
/// signature header. Mirrors Go `transactionContext`.
pub(crate) struct TransactionContext {
    pub transaction_id: String,
    pub signature_header: SignatureHeader,
}

impl TransactionContext {
    pub fn new(signing_id: &SigningIdentity) -> Self {
        let mut nonce = vec![0u8; 24];
        OsRng.fill_bytes(&mut nonce);

        let creator = signing_id.creator();

        // txid = hex(sha256(nonce || creator))
        let mut salted = Vec::with_capacity(nonce.len() + creator.len());
        salted.extend_from_slice(&nonce);
        salted.extend_from_slice(&creator);
        let transaction_id = hex::encode(hash::sha256(&salted));

        let signature_header = SignatureHeader { creator, nonce };

        Self {
            transaction_id,
            signature_header,
        }
    }
}

/// Builder for a transaction [`Proposal`]. Obtain one via
/// [`crate::Contract::new_proposal`].
pub struct ProposalBuilder<'a> {
    client: GrpcClient,
    signing_id: SigningIdentity,
    channel_name: String,
    chaincode_name: String,
    transaction_name: String,
    transient: HashMap<String, Vec<u8>>,
    endorsing_orgs: Vec<String>,
    args: Vec<Vec<u8>>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> ProposalBuilder<'a> {
    pub(crate) fn new(
        client: GrpcClient,
        signing_id: SigningIdentity,
        channel_name: String,
        chaincode_name: String,
        transaction_name: String,
    ) -> Self {
        Self {
            client,
            signing_id,
            channel_name,
            chaincode_name,
            transaction_name,
            transient: HashMap::new(),
            endorsing_orgs: Vec::new(),
            args: Vec::new(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Append string arguments to the transaction function.
    pub fn with_arguments<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args
            .extend(args.into_iter().map(|s| s.into().into_bytes()));
        self
    }

    /// Append raw byte arguments to the transaction function.
    pub fn with_bytes_arguments<I: IntoIterator<Item = Vec<u8>>>(mut self, args: I) -> Self {
        self.args.extend(args);
        self
    }

    /// Set the transient data map (private data / confidential inputs).
    pub fn with_transient(mut self, transient: HashMap<String, Vec<u8>>) -> Self {
        self.transient = transient;
        self
    }

    /// Restrict endorsement to peers of the given organization MSP IDs.
    pub fn with_endorsing_organizations<I, S>(mut self, mspids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.endorsing_orgs = mspids.into_iter().map(Into::into).collect();
        self
    }

    /// Build the [`Proposal`], generating a fresh transaction context.
    pub fn build(self) -> Result<Proposal, GatewayError> {
        let ctx = TransactionContext::new(&self.signing_id);
        let proposal_bytes = self.proposal_bytes(&ctx)?;

        let proposed = ProposedTransaction {
            transaction_id: ctx.transaction_id.clone(),
            proposal: Some(SignedProposal {
                proposal_bytes,
                signature: Vec::new(),
            }),
            endorsing_organizations: self.endorsing_orgs.clone(),
        };

        Ok(Proposal {
            client: self.client,
            signing_id: self.signing_id,
            channel_id: self.channel_name,
            proposed,
        })
    }

    fn proposal_bytes(&self, ctx: &TransactionContext) -> Result<Vec<u8>, GatewayError> {
        let header_bytes = self.header_bytes(ctx)?;
        let payload_bytes = self.chaincode_proposal_payload_bytes()?;

        let proposal = PbProposal {
            header: header_bytes,
            payload: payload_bytes,
            extension: Vec::new(),
        };
        Ok(proposal.encode_to_vec())
    }

    fn header_bytes(&self, ctx: &TransactionContext) -> Result<Vec<u8>, GatewayError> {
        let channel_header_bytes = self.channel_header_bytes(ctx);
        let signature_header_bytes = ctx.signature_header.encode_to_vec();

        let header = Header {
            channel_header: channel_header_bytes,
            signature_header: signature_header_bytes,
        };
        Ok(header.encode_to_vec())
    }

    fn channel_header_bytes(&self, ctx: &TransactionContext) -> Vec<u8> {
        let extension = ChaincodeHeaderExtension {
            chaincode_id: Some(ChaincodeId {
                path: String::new(),
                name: self.chaincode_name.clone(),
                version: String::new(),
            }),
        }
        .encode_to_vec();

        let channel_header = ChannelHeader {
            r#type: HeaderType::EndorserTransaction as i32,
            version: 0,
            timestamp: Some(now_timestamp()),
            channel_id: self.channel_name.clone(),
            tx_id: ctx.transaction_id.clone(),
            epoch: 0,
            extension,
            tls_cert_hash: Vec::new(),
        };
        channel_header.encode_to_vec()
    }

    fn chaincode_proposal_payload_bytes(&self) -> Result<Vec<u8>, GatewayError> {
        let invocation_spec = ChaincodeInvocationSpec {
            chaincode_spec: Some(ChaincodeSpec {
                r#type: 0,
                chaincode_id: Some(ChaincodeId {
                    path: String::new(),
                    name: self.chaincode_name.clone(),
                    version: String::new(),
                }),
                input: Some(ChaincodeInput {
                    args: self.chaincode_args(),
                    decorations: HashMap::new(),
                    is_init: false,
                }),
                timeout: 0,
            }),
        }
        .encode_to_vec();

        let payload = ChaincodeProposalPayload {
            input: invocation_spec,
            transient_map: self.transient.clone(),
        };
        Ok(payload.encode_to_vec())
    }

    /// args[0] is the (qualified) transaction name, followed by user args.
    fn chaincode_args(&self) -> Vec<Vec<u8>> {
        let mut result = Vec::with_capacity(self.args.len() + 1);
        result.push(self.transaction_name.clone().into_bytes());
        result.extend(self.args.iter().cloned());
        result
    }
}

/// A transaction proposal that can be endorsed (submit flow) or evaluated
/// (query). Supports the off-line signing flow via [`Proposal::bytes`],
/// [`Proposal::digest`] and [`crate::Gateway::new_signed_proposal`].
pub struct Proposal {
    client: GrpcClient,
    signing_id: SigningIdentity,
    channel_id: String,
    proposed: ProposedTransaction,
}

impl Proposal {
    pub(crate) fn from_parts(
        client: GrpcClient,
        signing_id: SigningIdentity,
        channel_id: String,
        proposed: ProposedTransaction,
    ) -> Self {
        Self {
            client,
            signing_id,
            channel_id,
            proposed,
        }
    }

    /// Serialised `ProposedTransaction` bytes.
    pub fn bytes(&self) -> Vec<u8> {
        self.proposed.encode_to_vec()
    }

    /// Digest of the proposal, for generating a digital signature off-line.
    pub fn digest(&self) -> Vec<u8> {
        let bytes = self
            .proposed
            .proposal
            .as_ref()
            .map(|p| p.proposal_bytes.as_slice())
            .unwrap_or_default();
        self.signing_id.hash(bytes)
    }

    /// Transaction ID for this proposal.
    pub fn transaction_id(&self) -> &str {
        &self.proposed.transaction_id
    }

    pub(crate) fn set_signature(&mut self, signature: Vec<u8>) {
        if let Some(p) = self.proposed.proposal.as_mut() {
            p.signature = signature;
        }
    }

    fn is_signed(&self) -> bool {
        self.proposed
            .proposal
            .as_ref()
            .map(|p| !p.signature.is_empty())
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

    /// Endorse the proposal, returning an endorsed [`Transaction`] ready for
    /// submission to the orderer.
    pub async fn endorse(mut self) -> Result<Transaction, GatewayError> {
        self.sign()?;

        let request = EndorseRequest {
            transaction_id: self.proposed.transaction_id.clone(),
            channel_id: self.channel_id.clone(),
            proposed_transaction: self.proposed.proposal.clone(),
            endorsing_organizations: self.proposed.endorsing_organizations.clone(),
        };
        let response = self.client.endorse(request).await?;

        let prepared = PreparedTransaction {
            transaction_id: self.proposed.transaction_id.clone(),
            envelope: response.prepared_transaction,
        };
        Transaction::new(self.client, self.signing_id, prepared)
    }

    /// Evaluate the proposal (query) and return the raw result payload.
    pub async fn evaluate(mut self) -> Result<Vec<u8>, GatewayError> {
        self.sign()?;

        let request = EvaluateRequest {
            transaction_id: self.proposed.transaction_id.clone(),
            channel_id: self.channel_id.clone(),
            proposed_transaction: self.proposed.proposal.clone(),
            target_organizations: self.proposed.endorsing_organizations.clone(),
        };
        let response = self.client.evaluate(request).await?;
        Ok(response.result.map(|r| r.payload).unwrap_or_default())
    }
}
