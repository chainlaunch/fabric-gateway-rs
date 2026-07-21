// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Chaincode event listening. Ports `chaincodeevents.go`,
//! `chaincodeeventsbuilder.go` and `eventsbuilder.go`.

use crate::client::GrpcClient;
use crate::error::GatewayError;
use crate::pb::common::{
    Block, ChannelHeader, Envelope, Header, HeaderType, Payload, SignatureHeader,
};
use crate::pb::gateway::SignedChaincodeEventsRequest;
use crate::pb::gateway::{ChaincodeEventsRequest as PbCcEventsRequest, ChaincodeEventsResponse};
use crate::pb::orderer::{
    seek_position::Type as SeekType, SeekInfo, SeekNextCommit, SeekPosition, SeekSpecified,
};
use crate::pb::protos::FilteredBlock;
use crate::signing::SigningIdentity;
use crate::util::now_timestamp;
use prost::Message;
use std::pin::Pin;
use tokio_stream::{Stream, StreamExt};

/// A chaincode event emitted by a transaction function.
#[derive(Debug, Clone)]
pub struct ChaincodeEvent {
    pub block_number: u64,
    pub transaction_id: String,
    pub chaincode_name: String,
    pub event_name: String,
    pub payload: Vec<u8>,
}

/// Builder for a chaincode-events request, supporting a custom start position.
pub struct ChaincodeEventsBuilder {
    client: GrpcClient,
    signing_id: SigningIdentity,
    channel_name: String,
    chaincode_name: String,
    start_position: Option<SeekPosition>,
    after_transaction_id: String,
}

impl ChaincodeEventsBuilder {
    pub(crate) fn new(
        client: GrpcClient,
        signing_id: SigningIdentity,
        channel_name: String,
        chaincode_name: String,
    ) -> Self {
        Self {
            client,
            signing_id,
            channel_name,
            chaincode_name,
            start_position: None,
            after_transaction_id: String::new(),
        }
    }

    /// Read events starting at the specified block number.
    pub fn with_start_block(mut self, block_number: u64) -> Self {
        self.start_position = Some(SeekPosition {
            r#type: Some(SeekType::Specified(SeekSpecified {
                number: block_number,
            })),
        });
        self
    }

    /// Resume after a specific transaction within the start block.
    pub fn with_after_transaction_id(mut self, transaction_id: impl Into<String>) -> Self {
        self.after_transaction_id = transaction_id.into();
        self
    }

    fn start_position(&self) -> SeekPosition {
        self.start_position.unwrap_or(SeekPosition {
            r#type: Some(SeekType::NextCommit(SeekNextCommit {})),
        })
    }

    /// Build the request (supports off-line signing via
    /// [`ChaincodeEventsRequest::bytes`] / [`ChaincodeEventsRequest::digest`]).
    pub fn build(self) -> ChaincodeEventsRequest {
        let creator = self.signing_id.creator();
        let start_position = Some(self.start_position());
        let request = PbCcEventsRequest {
            channel_id: self.channel_name,
            chaincode_id: self.chaincode_name,
            identity: creator,
            start_position,
            after_transaction_id: self.after_transaction_id,
        };
        let signed = SignedChaincodeEventsRequest {
            request: request.encode_to_vec(),
            signature: Vec::new(),
        };
        ChaincodeEventsRequest {
            client: self.client,
            signing_id: self.signing_id,
            signed_request: signed,
        }
    }
}

/// A signed request that streams chaincode events.
pub struct ChaincodeEventsRequest {
    client: GrpcClient,
    signing_id: SigningIdentity,
    signed_request: SignedChaincodeEventsRequest,
}

impl ChaincodeEventsRequest {
    /// Serialised request bytes (off-line signing).
    pub fn bytes(&self) -> Vec<u8> {
        self.signed_request.encode_to_vec()
    }

    /// Digest of the request, for off-line signing.
    pub fn digest(&self) -> Vec<u8> {
        self.signing_id.hash(&self.signed_request.request)
    }

    fn sign(&mut self) -> Result<(), GatewayError> {
        if !self.signed_request.signature.is_empty() {
            return Ok(());
        }
        let digest = self.digest();
        self.signed_request.signature = self.signing_id.sign(&digest)?;
        Ok(())
    }

    /// Open the event stream. Yields [`ChaincodeEvent`]s in ledger order. A
    /// background task reads the gRPC stream and forwards flattened events,
    /// mirroring the goroutine in fabric-gateway-go's `Events`.
    pub async fn events(
        mut self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChaincodeEvent> + Send>>, GatewayError> {
        self.sign()?;
        let mut stream = self.client.chaincode_events(self.signed_request).await?;

        let (tx, rx) = tokio::sync::mpsc::channel::<ChaincodeEvent>(16);
        tokio::spawn(async move {
            while let Some(Ok(response)) = stream.next().await {
                for event in flatten_events(response) {
                    if tx.send(event).await.is_err() {
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

fn flatten_events(response: ChaincodeEventsResponse) -> Vec<ChaincodeEvent> {
    let block_number = response.block_number;
    response
        .events
        .into_iter()
        .map(move |e| ChaincodeEvent {
            block_number,
            transaction_id: e.tx_id,
            chaincode_name: e.chaincode_id,
            event_name: e.event_name,
            payload: e.payload,
        })
        .collect()
}

/// Builder for block / filtered-block event requests. Ports the
/// `baseBlockEventsBuilder` seek-envelope construction.
pub struct BlockEventsBuilder {
    client: GrpcClient,
    signing_id: SigningIdentity,
    channel_name: String,
    tls_certificate_hash: Vec<u8>,
    start_position: Option<SeekPosition>,
}

impl BlockEventsBuilder {
    pub(crate) fn new(
        client: GrpcClient,
        signing_id: SigningIdentity,
        channel_name: String,
        tls_certificate_hash: Vec<u8>,
    ) -> Self {
        Self {
            client,
            signing_id,
            channel_name,
            tls_certificate_hash,
            start_position: None,
        }
    }

    /// Read events starting at the specified block number.
    pub fn with_start_block(mut self, block_number: u64) -> Self {
        self.start_position = Some(SeekPosition {
            r#type: Some(SeekType::Specified(SeekSpecified {
                number: block_number,
            })),
        });
        self
    }

    fn start_position(&self) -> SeekPosition {
        self.start_position.unwrap_or(SeekPosition {
            r#type: Some(SeekType::NextCommit(SeekNextCommit {})),
        })
    }

    fn payload_bytes(&self) -> Vec<u8> {
        let channel_header = ChannelHeader {
            r#type: HeaderType::DeliverSeekInfo as i32,
            version: 0,
            timestamp: Some(now_timestamp()),
            channel_id: self.channel_name.clone(),
            tx_id: String::new(),
            epoch: 0,
            extension: Vec::new(),
            tls_cert_hash: self.tls_certificate_hash.clone(),
        };
        let signature_header = SignatureHeader {
            creator: self.signing_id.creator(),
            nonce: Vec::new(),
        };
        // Seek from start position to the largest possible block number, so the
        // stream continues indefinitely as blocks are committed.
        let seek_info = SeekInfo {
            start: Some(self.start_position()),
            stop: Some(SeekPosition {
                r#type: Some(SeekType::Specified(SeekSpecified { number: u64::MAX })),
            }),
            behavior: 0,
            error_response: 0,
            content_type: 0,
        };
        let payload = Payload {
            header: Some(Header {
                channel_header: channel_header.encode_to_vec(),
                signature_header: signature_header.encode_to_vec(),
            }),
            data: seek_info.encode_to_vec(),
        };
        payload.encode_to_vec()
    }

    fn build_envelope(&self) -> Envelope {
        Envelope {
            payload: self.payload_bytes(),
            signature: Vec::new(),
        }
    }

    /// Build a request for full block events.
    pub fn build_block(self) -> BlockEventsRequest {
        let envelope = self.build_envelope();
        BlockEventsRequest {
            client: self.client,
            signing_id: self.signing_id,
            envelope,
        }
    }

    /// Build a request for filtered block events.
    pub fn build_filtered(self) -> FilteredBlockEventsRequest {
        let envelope = self.build_envelope();
        FilteredBlockEventsRequest {
            client: self.client,
            signing_id: self.signing_id,
            envelope,
        }
    }
}

macro_rules! block_events_request {
    ($name:ident, $item:ty, $open:ident, $extract:expr, $doc:literal) => {
        #[doc = $doc]
        pub struct $name {
            client: GrpcClient,
            signing_id: SigningIdentity,
            envelope: Envelope,
        }

        impl $name {
            /// Serialised seek envelope bytes (off-line signing).
            pub fn bytes(&self) -> Vec<u8> {
                self.envelope.encode_to_vec()
            }

            /// Digest of the seek envelope payload, for off-line signing.
            pub fn digest(&self) -> Vec<u8> {
                self.signing_id.hash(&self.envelope.payload)
            }

            fn sign(&mut self) -> Result<(), GatewayError> {
                if !self.envelope.signature.is_empty() {
                    return Ok(());
                }
                let digest = self.digest();
                self.envelope.signature = self.signing_id.sign(&digest)?;
                Ok(())
            }

            /// Open the event stream.
            pub async fn events(
                mut self,
            ) -> Result<Pin<Box<dyn Stream<Item = $item> + Send>>, GatewayError> {
                self.sign()?;
                let mut stream = self.client.$open(self.envelope).await?;
                let (tx, rx) = tokio::sync::mpsc::channel::<$item>(16);
                tokio::spawn(async move {
                    while let Some(Ok(response)) = stream.next().await {
                        #[allow(clippy::redundant_closure_call)]
                        if let Some(item) = ($extract)(response) {
                            if tx.send(item).await.is_err() {
                                return;
                            }
                        }
                    }
                });
                Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
            }
        }
    };
}

block_events_request!(
    BlockEventsRequest,
    Block,
    block_events,
    |r: crate::pb::protos::DeliverResponse| match r.r#type {
        Some(crate::pb::protos::deliver_response::Type::Block(b)) => Some(b),
        _ => None,
    },
    "Streams full block events from the peer's Deliver service."
);

block_events_request!(
    FilteredBlockEventsRequest,
    FilteredBlock,
    filtered_block_events,
    |r: crate::pb::protos::DeliverResponse| match r.r#type {
        Some(crate::pb::protos::deliver_response::Type::FilteredBlock(b)) => Some(b),
        _ => None,
    },
    "Streams filtered block events from the peer's Deliver service."
);
