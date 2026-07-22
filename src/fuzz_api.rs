// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Byte-taking wrappers around the network-facing decoders, exposed **only**
//! under the internal `_fuzz` feature so the cargo-fuzz targets under `fuzz/`
//! can reach them. These are not part of the public API and must not be enabled
//! by application code. Each returns nothing: the property under test is simply
//! "does not panic on arbitrary input".

use crate::pb::common::{ChannelHeader, Envelope, Header};
use crate::pb::gateway::ProposedTransaction;
use crate::pb::protos::Proposal;
use prost::Message;

/// The `Gateway::new_proposal_from_bytes` decode chain, runtime-free.
pub fn decode_proposed_transaction(bytes: &[u8]) {
    if let Ok(pt) = ProposedTransaction::decode(bytes) {
        if let Some(sp) = pt.proposal {
            if let Ok(p) = Proposal::decode(sp.proposal_bytes.as_slice()) {
                if let Ok(h) = Header::decode(p.header.as_slice()) {
                    let _ = ChannelHeader::decode(h.channel_header.as_slice());
                }
            }
        }
    }
}

/// The endorsed-transaction envelope parser (EndorseResponse path).
pub fn parse_envelope(bytes: &[u8]) {
    if let Ok(env) = Envelope::decode(bytes) {
        let _ = crate::transaction::parse_transaction_envelope(&env);
    }
}

/// The gRPC error-detail decoder (untrusted status details from the gateway).
pub fn decode_error_details(bytes: &[u8]) {
    let status = tonic::Status::with_details(tonic::Code::Aborted, "fuzz", bytes.to_vec().into());
    let _ = crate::error::decode_error_details(&status);
}
