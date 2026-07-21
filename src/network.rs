// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Network (channel) handle. Ports `network.go`.

use crate::client::GrpcClient;
use crate::contract::Contract;
use crate::events::{BlockEventsBuilder, ChaincodeEventsBuilder};
use crate::signing::SigningIdentity;

/// Represents a network of nodes that are members of a specific Fabric channel.
/// Obtain contracts and (in future) subscribe to events through it.
pub struct Network {
    pub(crate) client: GrpcClient,
    pub(crate) signing_id: SigningIdentity,
    pub(crate) name: String,
    pub(crate) tls_certificate_hash: Vec<u8>,
}

impl Network {
    /// Name of the Fabric channel this network represents.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the default contract for a chaincode.
    pub fn get_contract(&self, chaincode_name: &str) -> Contract {
        self.get_contract_with_name(chaincode_name, "")
    }

    /// Get a named contract within a chaincode.
    pub fn get_contract_with_name(&self, chaincode_name: &str, contract_name: &str) -> Contract {
        Contract {
            client: self.client.clone(),
            signing_id: self.signing_id.clone(),
            channel_name: self.name.clone(),
            chaincode_name: chaincode_name.to_string(),
            contract_name: contract_name.to_string(),
        }
    }

    /// Start building a chaincode-events request for the named chaincode.
    pub fn new_chaincode_events_request(&self, chaincode_name: &str) -> ChaincodeEventsBuilder {
        ChaincodeEventsBuilder::new(
            self.client.clone(),
            self.signing_id.clone(),
            self.name.clone(),
            chaincode_name.to_string(),
        )
    }

    /// Start building a block-events request (full or filtered blocks).
    pub fn new_block_events_request(&self) -> BlockEventsBuilder {
        BlockEventsBuilder::new(
            self.client.clone(),
            self.signing_id.clone(),
            self.name.clone(),
            self.tls_certificate_hash.clone(),
        )
    }
}
