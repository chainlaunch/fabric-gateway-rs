//! Robustness ("fuzz-lite") tests: the decode paths that consume bytes coming
//! off the network must never panic on malformed, truncated, or hostile input —
//! they must return an error instead. This runs in the normal test suite (no
//! nightly/libFuzzer needed); `fuzz/` contains deeper cargo-fuzz targets for the
//! same entry points.

use crate::error::decode_error_details;
use crate::pb::common::Envelope;
use crate::transaction::parse_transaction_envelope;
use crate::{Gateway, Identity};
use prost::Message;
use rand_core::{OsRng, RngCore};

fn offline_gateway() -> Gateway {
    // A self-signed throwaway cert is fine here — we only exercise decoders.
    let identity = Identity::new("Org1MSP", b"not-a-real-cert".to_vec());
    let channel = tonic::transport::Channel::from_static("http://127.0.0.1:0").connect_lazy();
    Gateway::builder(identity, channel).connect().unwrap()
}

fn random_bytes(max: usize) -> Vec<u8> {
    let mut lenb = [0u8; 2];
    OsRng.fill_bytes(&mut lenb);
    let len = (u16::from_le_bytes(lenb) as usize) % (max + 1);
    let mut buf = vec![0u8; len];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// Feed a byte buffer through every network-facing decoder. Must not panic;
/// return value is irrelevant.
fn exercise_decoders(gw: &Gateway, bytes: &[u8]) {
    // 1. Proposal re-hydration (off-line signing entry point).
    let _ = gw.new_proposal_from_bytes(bytes);
    let _ = gw.new_signed_proposal(bytes, vec![1, 2, 3]);

    // 2. Endorsed-transaction envelope parsing (EndorseResponse path).
    if let Ok(env) = Envelope::decode(bytes) {
        let _ = parse_transaction_envelope(&env);
    }
    // Also treat the raw bytes as an already-built Envelope payload.
    let env = Envelope {
        payload: bytes.to_vec(),
        signature: Vec::new(),
    };
    let _ = parse_transaction_envelope(&env);

    // 3. gRPC error-detail decoding (untrusted status details from the gateway).
    let status = tonic::Status::with_details(tonic::Code::Aborted, "fuzz", bytes.to_vec().into());
    let _ = decode_error_details(&status);
}

#[tokio::test]
async fn decoders_survive_random_bytes() {
    let gw = offline_gateway();
    for _ in 0..20_000 {
        let bytes = random_bytes(512);
        exercise_decoders(&gw, &bytes);
    }
}

#[tokio::test]
async fn decoders_survive_truncated_valid_proposal() {
    // Build a real proposal, then feed every truncation of it to the decoders —
    // catches off-by-one / slice-out-of-bounds bugs near valid structure.
    let gw = offline_gateway();
    let proposal = gw
        .get_network("mychannel")
        .get_contract("basic")
        .new_proposal("CreateAsset")
        .with_arguments(["a", "b", "c"])
        .build()
        .unwrap();
    let bytes = proposal.bytes();

    for cut in 0..=bytes.len() {
        exercise_decoders(&gw, &bytes[..cut]);
    }
}

#[tokio::test]
async fn decoders_survive_bitflipped_valid_proposal() {
    // Flip random bytes in an otherwise-valid proposal.
    let gw = offline_gateway();
    let base = gw
        .get_network("mychannel")
        .get_contract("basic")
        .new_proposal("Tx")
        .with_arguments(["x"])
        .build()
        .unwrap()
        .bytes();

    for _ in 0..5_000 {
        let mut b = base.clone();
        if !b.is_empty() {
            let mut idx = [0u8; 4];
            OsRng.fill_bytes(&mut idx);
            let i = (u32::from_le_bytes(idx) as usize) % b.len();
            let mut xor = [0u8; 1];
            OsRng.fill_bytes(&mut xor);
            b[i] ^= xor[0];
        }
        exercise_decoders(&gw, &b);
    }
}
