// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! # fabric-gateway
//!
//! A Rust client for the Hyperledger Fabric Gateway, ported from the official
//! [fabric-gateway-go](https://github.com/hyperledger/fabric-gateway) `client`
//! package. It lets applications evaluate (query) and submit (update)
//! transactions against chaincode deployed to a Fabric network, connecting
//! through a peer's embedded Gateway service.
//!
//! ## Programming model
//!
//! The type hierarchy mirrors the Go library:
//! [`Gateway`] → [`Network`] (a channel) → [`Contract`] (a chaincode) →
//! [`Proposal`] → [`Transaction`] → [`Commit`].
//!
//! ```no_run
//! use fabric_gateway::{Gateway, Identity, identity::EcdsaP256Signer};
//! use std::sync::Arc;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let channel = tonic::transport::Channel::from_static("https://peer:7051")
//!     .connect()
//!     .await?;
//! let identity = Identity::new("Org1MSP", std::fs::read("cert.pem")?);
//! let signer = Arc::new(EcdsaP256Signer::from_pem(&std::fs::read_to_string("key.pem")?)?);
//!
//! let gateway = Gateway::connect(identity, signer, channel)?;
//! let contract = gateway.get_network("mychannel").get_contract("basic");
//!
//! let result = contract.evaluate_transaction("GetAllAssets", &[]).await?;
//! println!("{}", String::from_utf8_lossy(&result));
//! # Ok(())
//! # }
//! ```
//!
//! ## Wire-compatibility
//!
//! Proposals, transactions and commit-status requests are serialised with the
//! same protobuf layout, transaction-ID derivation
//! (`hex(sha256(nonce || creator))`), SHA-256 digesting and low-S ASN.1-DER
//! ECDSA signing as fabric-gateway-go, so signatures verify on the peer.

pub mod error;
pub mod hash;
pub mod identity;
pub mod pb;

mod client;
mod commit;
mod contract;
mod events;
mod gateway;
mod network;
mod proposal;
mod signing;
mod transaction;
mod util;

pub use commit::{Commit, Status};
pub use contract::Contract;
pub use error::{GatewayError, TransactionError};
pub use events::{
    BlockEventsBuilder, BlockEventsRequest, ChaincodeEvent, ChaincodeEventsBuilder,
    ChaincodeEventsRequest, FilteredBlockEventsRequest,
};
pub use gateway::{Gateway, GatewayBuilder};
pub use identity::{EcdsaP256Signer, Identity, Signer};
pub use network::Network;
pub use proposal::{Proposal, ProposalBuilder};
pub use transaction::Transaction;
