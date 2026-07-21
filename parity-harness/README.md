# Parity harness

`main.go` links the real `github.com/hyperledger/fabric-gateway` client and
exchanges proposals/signatures with the Rust crate. See ../PARITY.md.

Generate a shared test identity first:

    openssl ecparam -name prime256v1 -genkey -noout | openssl pkcs8 -topk8 -nocrypt -out key.pem
    openssl req -new -x509 -key key.pem -out cert.pem -days 3650 \
      -subj "/O=Org1MSP/OU=client/CN=parity-test-user"

Then run the cross-checks (see run_parity.sh).
