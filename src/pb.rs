// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Generated protobuf modules for the Fabric Gateway wire protocol.
//!
//! prost/tonic generate one module per proto package. Fabric's `peer/*.proto`
//! files declare `package protos`, so peer messages land in [`protos`].
//!
//! Doc-comment lints are silenced here because the doc text is generated
//! verbatim from the upstream `.proto` comments, which we do not edit.
#![allow(
    clippy::doc_lazy_continuation,
    clippy::doc_overindented_list_items,
    clippy::large_enum_variant,
    rustdoc::invalid_html_tags,
    rustdoc::bare_urls
)]

pub mod common {
    tonic::include_proto!("common");
}

pub mod msp {
    tonic::include_proto!("msp");
}

pub mod protos {
    // Hyperledger Fabric peer messages (proposal, transaction, chaincode, ...).
    tonic::include_proto!("protos");
}

pub mod orderer {
    tonic::include_proto!("orderer");
}

pub mod gateway {
    tonic::include_proto!("gateway");
}

pub mod rwset {
    tonic::include_proto!("rwset");
}

pub mod kvrwset {
    tonic::include_proto!("kvrwset");
}

pub mod gossip {
    tonic::include_proto!("gossip");
}

pub mod transientstore {
    tonic::include_proto!("transientstore");
}

pub mod queryresult {
    tonic::include_proto!("queryresult");
}
