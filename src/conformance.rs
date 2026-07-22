//! Differential byte-equality conformance test against fabric-gateway-go.
//!
//! For each of many *randomized* proposal inputs, this builds the proposal with
//! both the real Go client (via the `parity-harness` binary) and this crate,
//! then asserts the serialized `ProposedTransaction` bytes are **identical**.
//!
//! Because the Go library generates the nonce and timestamp internally, we let
//! Go build first, extract its nonce + timestamp from the produced bytes, and
//! rebuild the Rust proposal with those exact values — so any remaining
//! difference is a genuine encoding divergence, not just randomness.
//!
//! The test is self-sufficient: it generates the shared identity fixture with
//! `openssl` and builds the harness with `go` if needed. It skips cleanly (does
//! not fail) when `go` or `openssl` are unavailable, e.g. on a contributor
//! machine without the Go toolchain.

use crate::pb::common::{ChannelHeader, Header, SignatureHeader};
use crate::pb::gateway::ProposedTransaction;
use crate::pb::protos::Proposal as PbProposal;
use crate::{Gateway, Identity};
use prost::Message;
use rand_core::{OsRng, RngCore};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const NUM_CASES: usize = 200;

fn harness_dir() -> PathBuf {
    PathBuf::from(std::env::var("PARITY_DIR").unwrap_or_else(|_| "parity-harness".to_string()))
}

fn have(bin: &str) -> bool {
    Command::new(bin)
        .arg("version")
        .output()
        .map(|o| o.status.success() || !o.stdout.is_empty() || !o.stderr.is_empty())
        .unwrap_or(false)
}

/// Ensure the shared P-256 identity (cert.pem + PKCS#8 key.pem) exists.
fn ensure_identity(dir: &Path) -> bool {
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");
    if cert.exists() && key.exists() {
        return true;
    }
    if !have("openssl") {
        return false;
    }
    let sec1 = dir.join("key_sec1.pem");
    let ok = run(
        dir,
        "openssl",
        &[
            "ecparam",
            "-name",
            "prime256v1",
            "-genkey",
            "-noout",
            "-out",
            "key_sec1.pem",
        ],
    ) && run(
        dir,
        "openssl",
        &[
            "pkcs8",
            "-topk8",
            "-nocrypt",
            "-in",
            "key_sec1.pem",
            "-out",
            "key.pem",
        ],
    ) && run(
        dir,
        "openssl",
        &[
            "req",
            "-new",
            "-x509",
            "-key",
            "key.pem",
            "-out",
            "cert.pem",
            "-days",
            "3650",
            "-subj",
            "/O=Org1MSP/OU=client/CN=conformance-test",
        ],
    );
    let _ = std::fs::remove_file(sec1);
    ok
}

fn run(dir: &Path, bin: &str, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .current_dir(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn rand_u32() -> u32 {
    let mut b = [0u8; 4];
    OsRng.fill_bytes(&mut b);
    u32::from_le_bytes(b)
}

fn rand_below(n: u32) -> u32 {
    if n == 0 {
        0
    } else {
        rand_u32() % n
    }
}

/// A pool of "interesting" characters to stress protobuf string/byte encoding:
/// ascii, punctuation that needs JSON escaping, and multi-byte UTF-8.
fn rand_string(max_len: usize) -> String {
    const POOL: &[char] = &[
        'a', 'Z', '0', '9', ' ', '"', '\\', '/', ':', '-', '_', '{', '}', '\n', 'é', 'λ', '漢',
        '🚀',
    ];
    let len = rand_below(max_len as u32 + 1) as usize;
    (0..len)
        .map(|_| POOL[rand_below(POOL.len() as u32) as usize])
        .collect()
}

#[derive(serde::Serialize)]
struct Case {
    channel: String,
    chaincode: String,
    function: String,
    args: Vec<String>,
    transient: HashMap<String, String>,
    #[serde(rename = "endorsingOrgs")]
    endorsing_orgs: Vec<String>,
}

fn random_case() -> Case {
    // Non-empty channel/chaincode/function; args 0..5; transient 0 or 1 entry
    // (protobuf map field order is unspecified, so >1 entry could differ purely
    // by iteration order — 0/1 keeps the byte comparison meaningful); orgs 0..3.
    let non_empty = |max| {
        let s = rand_string(max);
        if s.is_empty() {
            "x".to_string()
        } else {
            s
        }
    };
    let args = (0..rand_below(6)).map(|_| rand_string(12)).collect();
    let mut transient = HashMap::new();
    if rand_below(2) == 1 {
        // Non-empty key AND value: protobuf map-entry encoding of an empty
        // (default) value is implementation-defined across proto libraries and
        // unrelated to gateway logic, so we don't assert on that degenerate case.
        transient.insert(non_empty(8), non_empty(16));
    }
    let endorsing_orgs = (0..rand_below(4)).map(|_| non_empty(10)).collect();
    Case {
        channel: non_empty(16),
        chaincode: non_empty(12),
        function: non_empty(12),
        args,
        transient,
        endorsing_orgs,
    }
}

/// Extract (nonce, timestamp) from a Go-produced ProposedTransaction so the Rust
/// build can be made byte-comparable.
fn extract_context(go_bytes: &[u8]) -> (Vec<u8>, prost_types::Timestamp) {
    let pt = ProposedTransaction::decode(go_bytes).expect("decode ProposedTransaction");
    let proposal_bytes = pt.proposal.expect("proposal").proposal_bytes;
    let prop = PbProposal::decode(proposal_bytes.as_slice()).expect("decode Proposal");
    let header = Header::decode(prop.header.as_slice()).expect("decode Header");
    let sh = SignatureHeader::decode(header.signature_header.as_slice()).expect("decode SigHeader");
    let ch = ChannelHeader::decode(header.channel_header.as_slice()).expect("decode ChannelHeader");
    (sh.nonce, ch.timestamp.expect("timestamp"))
}

#[tokio::test]
async fn differential_byte_equality_vs_go() {
    let dir = harness_dir();

    if !have("go") {
        eprintln!("SKIP: `go` not available — cannot run differential conformance");
        return;
    }
    if !ensure_identity(&dir) {
        eprintln!("SKIP: could not create identity fixture (needs openssl)");
        return;
    }
    // Build the harness binary.
    if !run(&dir, "go", &["build", "-o", "parity", "."]) {
        eprintln!("SKIP: `go build` of the parity harness failed");
        return;
    }

    // Generate randomized cases and write them for the Go side.
    let cases: Vec<Case> = (0..NUM_CASES).map(|_| random_case()).collect();
    let cases_path = dir.join("conformance_cases.json");
    std::fs::write(&cases_path, serde_json::to_vec(&cases).unwrap()).unwrap();

    // Go builds every case; collect its hex-encoded ProposedTransaction bytes.
    let output = Command::new("./parity")
        .args(["conformance", "conformance_cases.json"])
        .current_dir(&dir)
        .output()
        .expect("run parity conformance");
    assert!(
        output.status.success(),
        "harness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let go_hex: Vec<String> = serde_json::from_slice(&output.stdout).expect("parse harness output");
    assert_eq!(go_hex.len(), cases.len(), "case count mismatch");

    // Offline gateway using the same identity the harness loaded.
    let cert = std::fs::read(dir.join("cert.pem")).unwrap();
    let identity = Identity::new("Org1MSP", cert);
    let channel = tonic::transport::Channel::from_static("http://127.0.0.1:0").connect_lazy();
    let gateway = Gateway::builder(identity, channel).connect().unwrap();

    let mut compared = 0;
    for (i, case) in cases.iter().enumerate() {
        let go_bytes = hex::decode(&go_hex[i]).expect("decode go hex");
        let (nonce, timestamp) = extract_context(&go_bytes);

        // Rebuild the identical proposal in Rust with Go's nonce + timestamp.
        let network = gateway.get_network(&case.channel);
        let contract = network.get_contract(&case.chaincode);
        let mut builder = contract
            .new_proposal(&case.function)
            .with_arguments(case.args.clone());
        if !case.transient.is_empty() {
            let transient = case
                .transient
                .iter()
                .map(|(k, v)| (k.clone(), v.clone().into_bytes()))
                .collect();
            builder = builder.with_transient(transient);
        }
        if !case.endorsing_orgs.is_empty() {
            builder = builder.with_endorsing_organizations(case.endorsing_orgs.clone());
        }
        let rust_bytes = builder
            .build_with_context(nonce, timestamp)
            .unwrap()
            .bytes();

        assert_eq!(
            hex::encode(&rust_bytes),
            go_hex[i],
            "byte mismatch on case {i}: {}",
            serde_json::to_string(case).unwrap()
        );
        compared += 1;
    }

    let _ = std::fs::remove_file(&cases_path);
    println!("OK: {compared} randomized proposals are byte-identical to fabric-gateway-go");
    assert_eq!(compared, NUM_CASES);
}
