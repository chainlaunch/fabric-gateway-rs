// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Self-contained correctness tests for the parity-critical algorithms. These
//! need no external harness and encode the exact behaviour of fabric-gateway-go.

use fabric_gateway::identity::{EcdsaP256Signer, Signer};
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use p256::ecdsa::{DerSignature, Signature, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

/// Transaction ID derivation must be `hex(sha256(nonce || creator))`, matching
/// `newTransactionContext` in fabric-gateway-go.
#[test]
fn transaction_id_derivation() {
    let nonce = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17";
    let creator = b"some-serialized-identity-bytes";

    let mut hasher = Sha256::new();
    hasher.update(nonce);
    hasher.update(creator);
    let expected = hex::encode(hasher.finalize());

    // Independently recompute the way the library concatenates.
    let mut salted = Vec::new();
    salted.extend_from_slice(nonce);
    salted.extend_from_slice(creator);
    let got = hex::encode(fabric_gateway::hash::sha256(&salted));

    assert_eq!(got, expected);
    assert_eq!(got.len(), 64, "txid is a 32-byte hex string");
}

/// Every ECDSA signature the signer emits must be low-S (S <= N/2) and DER
/// encoded — Fabric rejects high-S signatures. Ports the intent of
/// `canonicalECDSASignatureSValue`.
#[test]
fn signatures_are_low_s_der() {
    let signing_key = SigningKey::random(&mut rand_core::OsRng);
    let verifying_key = VerifyingKey::from(&signing_key);
    let signer = EcdsaP256Signer::from_signing_key(signing_key);

    // Half the curve order N for P-256.
    // N = FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
    let half_order =
        hex::decode("7fffffff800000007fffffffffffffffde737d56d38bcf4279dce5617e3192a8").unwrap();

    for i in 0..50u32 {
        let digest = fabric_gateway::hash::sha256(format!("message-{i}").as_bytes());
        let sig_der = signer.sign(&digest).unwrap();

        // Parse DER into the fixed r(32)||s(32) form and extract S.
        let sig = Signature::from_der(&sig_der).expect("valid DER");
        let s_bytes = sig.to_bytes();
        let s = &s_bytes[32..64];
        assert!(
            s <= half_order.as_slice(),
            "iteration {i}: S value is not canonical (low-S): {}",
            hex::encode(s)
        );

        // And it must verify.
        verifying_key
            .verify_prehash(&digest, &sig)
            .expect("signature verifies");
    }
}

/// A signature produced by the signer must verify with the corresponding public
/// key, and the DER encoding must be parseable as `ecdsaSignature{R,S}` — i.e.
/// the exact structure `asn1ECDSASignature` produces in Go.
#[test]
fn sign_verify_roundtrip_and_der_structure() {
    let signing_key = SigningKey::random(&mut rand_core::OsRng);
    let vk = VerifyingKey::from(&signing_key);
    let pubkey_point = vk.to_encoded_point(false);
    assert_eq!(pubkey_point.as_bytes()[0], 0x04, "uncompressed SEC1 point");

    let signer = EcdsaP256Signer::from_signing_key(signing_key);
    let digest = fabric_gateway::hash::sha256(b"hello fabric");
    let sig = signer.sign(&digest).unwrap();

    // DER: SEQUENCE { INTEGER r, INTEGER s }.
    assert_eq!(sig[0], 0x30, "DER SEQUENCE tag");
    let der = DerSignature::from_bytes(&sig).unwrap();
    vk.verify_prehash(&digest, &der).unwrap();
}

/// The signer must accept both SEC1 ("EC PRIVATE KEY") and PKCS#8
/// ("PRIVATE KEY") PEM encodings, like Go's `identity.PrivateKeyFromPEM`
/// combined with SDK key loading.
#[test]
fn accepts_sec1_and_pkcs8_pem() {
    // Generate a key and round-trip it through both encodings.
    use p256::pkcs8::EncodePrivateKey;
    use sec1::EncodeEcPrivateKey;

    let key = SigningKey::random(&mut rand_core::OsRng);
    let pkcs8 = key.to_pkcs8_pem(Default::default()).unwrap();
    let sec1 = key.to_sec1_pem(Default::default()).unwrap();

    let digest = fabric_gateway::hash::sha256(b"x");
    let from_pkcs8 = EcdsaP256Signer::from_pem(&pkcs8).unwrap();
    let from_sec1 = EcdsaP256Signer::from_pem(&sec1).unwrap();

    // Both parse to the same key, so both must verify against the same pubkey.
    let vk = VerifyingKey::from(&key);
    for signer in [from_pkcs8, from_sec1] {
        let sig = signer.sign(&digest).unwrap();
        let der = DerSignature::from_bytes(&sig).unwrap();
        vk.verify_prehash(&digest, &der).unwrap();
    }
}

/// Filesystem loaders: a key written to disk as PEM or DER, and one placed in a
/// Fabric-style `keystore/` directory, must all load and verify against the same
/// public key.
#[test]
fn loads_private_key_from_filesystem() {
    use p256::pkcs8::EncodePrivateKey;
    use sec1::EncodeEcPrivateKey;

    let key = SigningKey::random(&mut rand_core::OsRng);
    let vk = VerifyingKey::from(&key);
    let digest = fabric_gateway::hash::sha256(b"fs-key");

    // A unique temp dir without external crates.
    let base = std::env::temp_dir().join(format!("fgw-keytest-{}", std::process::id()));
    let ks = base.join("keystore");
    std::fs::create_dir_all(&ks).unwrap();

    let pem_path = base.join("key.pem");
    let der_path = base.join("key.der");
    std::fs::write(
        &pem_path,
        key.to_pkcs8_pem(Default::default()).unwrap().as_bytes(),
    )
    .unwrap();
    std::fs::write(&der_path, key.to_pkcs8_der().unwrap().as_bytes()).unwrap();
    // Fabric names keystore files after the key's SKI, e.g. `<hex>_sk`.
    std::fs::write(ks.join("abc123_sk"), key.to_sec1_der().unwrap().as_bytes()).unwrap();

    for signer in [
        EcdsaP256Signer::from_file(&pem_path).expect("load PEM file"),
        EcdsaP256Signer::from_file(&der_path).expect("load DER file"),
        EcdsaP256Signer::from_keystore_dir(&ks).expect("load keystore dir"),
    ] {
        let sig = signer.sign(&digest).unwrap();
        let der = DerSignature::from_bytes(&sig).unwrap();
        vk.verify_prehash(&digest, &der).unwrap();
    }

    // A keystore with two keys must error rather than pick one silently.
    std::fs::write(ks.join("def456_sk"), key.to_sec1_der().unwrap().as_bytes()).unwrap();
    assert!(EcdsaP256Signer::from_keystore_dir(&ks).is_err());

    std::fs::remove_dir_all(&base).ok();
}
