#!/usr/bin/env bash
# Full cross-implementation parity run between fabric-gateway-go and
# fabric-gateway-rs. Exits non-zero on any mismatch.
#
# Prerequisite: generate the shared test identity (see README.md) so that
# key.pem and cert.pem exist in this directory.
set -euo pipefail

HARNESS_DIR="$(cd "$(dirname "$0")" && pwd -P)"
RUST_DIR="$(cd "$HARNESS_DIR/.." && pwd -P)"
cd "$HARNESS_DIR"

FIXED_MSG="fabric-gateway-parity-test"
DIGEST=$(printf '%s' "$FIXED_MSG" | shasum -a 256 | cut -d' ' -f1)

echo "== Building Go harness =="
go build -o parity .

echo "== [Go] build proposal + sign digest + export pubkey =="
./parity build-proposal >/dev/null
./parity sign "$DIGEST" >/dev/null
./parity pubkey >/dev/null

echo "== [Rust] run parity tests (parse Go proposal, verify Go sig, build proposal) =="
( cd "$RUST_DIR" && PARITY_DIR="$HARNESS_DIR" cargo test --test parity -- --nocapture )

echo "== [Go] consume the Rust-built proposal =="
GO_ON_RUST=$(./parity parse-proposal rust_proposal.bin)
echo "  Go parsed rust_proposal.bin -> $GO_ON_RUST"

echo "== [Go] verify a Rust-produced signature =="
# Produce a Rust signature over the same digest via a tiny helper binary.
RUST_SIG_HEX=$(cd "$RUST_DIR" && PARITY_DIR="$HARNESS_DIR" cargo run --quiet --example sign_digest -- "$DIGEST")
echo "$RUST_SIG_HEX" | xxd -r -p > rust_sig.bin
RUST_VERIFY=$(./parity verify "$DIGEST" rust_sig.bin)
echo "  Go verify(rust_sig) -> $RUST_VERIFY"
if [[ "$RUST_VERIFY" != '{"valid":true}' ]]; then
  echo "FAIL: Go could not verify the Rust signature"; exit 1
fi

echo
echo "ALL PARITY CHECKS PASSED"
