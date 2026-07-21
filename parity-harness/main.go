// SPDX-License-Identifier: Apache-2.0

// Parity harness: uses the real fabric-gateway-go client library to (a) build a
// proposal and dump its bytes, (b) consume a Rust-built proposal and report what
// the Go library extracts, and (c) sign/verify digests for crypto interop.
package main

import (
	"crypto/ecdsa"
	"crypto/sha256"
	"crypto/x509"
	"encoding/hex"
	"encoding/json"
	"encoding/pem"
	"fmt"
	"os"

	"github.com/hyperledger/fabric-gateway/pkg/client"
	"github.com/hyperledger/fabric-gateway/pkg/identity"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

func must(err error) {
	if err != nil {
		panic(err)
	}
}

func loadIdentity() (*identity.X509Identity, *ecdsa.PrivateKey) {
	certPEM, err := os.ReadFile("cert.pem")
	must(err)
	keyPEM, err := os.ReadFile("key.pem")
	must(err)

	cert, err := identity.CertificateFromPEM(certPEM)
	must(err)
	id, err := identity.NewX509Identity("Org1MSP", cert)
	must(err)

	privKey, err := identity.PrivateKeyFromPEM(keyPEM)
	must(err)
	return id, privKey.(*ecdsa.PrivateKey)
}

// connectOffline builds a Gateway with a lazy (non-dialing) gRPC client so we
// can construct/parse proposals without a live peer.
func connectOffline(id *identity.X509Identity, key *ecdsa.PrivateKey) *client.Gateway {
	sign, err := identity.NewPrivateKeySign(key)
	must(err)
	conn, err := grpc.NewClient("localhost:0", grpc.WithTransportCredentials(insecure.NewCredentials()))
	must(err)
	gw, err := client.Connect(id, client.WithSign(sign), client.WithClientConnection(conn))
	must(err)
	return gw
}

func main() {
	if len(os.Args) < 2 {
		panic("usage: main <command>")
	}
	id, key := loadIdentity()

	switch os.Args[1] {
	case "creator":
		// Dump the serialized creator (SerializedIdentity) the Go lib produces.
		gw := connectOffline(id, key)
		p, err := gw.GetNetwork("mychannel").GetContract("basic").
			NewProposal("CreateAsset", client.WithArguments("a", "b", "c"))
		must(err)
		out := map[string]string{
			"transaction_id": p.TransactionID(),
			"proposal_bytes": hex.EncodeToString(mustBytes(p.Bytes())),
			"digest":         hex.EncodeToString(p.Digest()),
		}
		emit(out)

	case "build-proposal":
		// Build a proposal and write its serialized ProposedTransaction bytes.
		gw := connectOffline(id, key)
		p, err := gw.GetNetwork("mychannel").GetContract("basic").
			NewProposal("CreateAsset", client.WithArguments("asset1", "blue", "20"))
		must(err)
		must(os.WriteFile("go_proposal.bin", mustBytes(p.Bytes()), 0644))
		report := map[string]string{
			"transaction_id": p.TransactionID(),
			"digest":         hex.EncodeToString(p.Digest()),
		}
		rb, _ := json.Marshal(report)
		must(os.WriteFile("go_proposal.json", rb, 0644))
		emit(report)

	case "parse-proposal":
		// Consume a Rust-built proposal via the Go library and report extraction.
		gw := connectOffline(id, key)
		data, err := os.ReadFile(os.Args[2])
		must(err)
		p, err := gw.NewProposal(data)
		must(err)
		emit(map[string]string{
			"transaction_id": p.TransactionID(),
			"digest":         hex.EncodeToString(p.Digest()),
		})

	case "sign":
		// Sign a fixed digest (from stdin hex arg) and write DER signature.
		digest, err := hex.DecodeString(os.Args[2])
		must(err)
		sign, err := identity.NewPrivateKeySign(key)
		must(err)
		sig, err := sign(digest)
		must(err)
		must(os.WriteFile("go_sig.bin", sig, 0644))
		emit(map[string]string{"signature": hex.EncodeToString(sig)})

	case "verify":
		// Verify a DER signature (file) over a digest (hex) using the public key.
		digest, err := hex.DecodeString(os.Args[2])
		must(err)
		sig, err := os.ReadFile(os.Args[3])
		must(err)
		ok := ecdsa.VerifyASN1(&key.PublicKey, digest, sig)
		emit(map[string]bool{"valid": ok})

	case "pubkey":
		// Emit the SEC1 uncompressed public key point for Rust-side verification.
		pub := key.PublicKey
		der, err := x509.MarshalPKIXPublicKey(&pub)
		must(err)
		pemBytes := pem.EncodeToMemory(&pem.Block{Type: "PUBLIC KEY", Bytes: der})
		must(os.WriteFile("pub.pem", pemBytes, 0644))
		emit(map[string]string{"pubkey_sha256": hex.EncodeToString(sha256Sum(der))})

	default:
		panic("unknown command: " + os.Args[1])
	}
}

func sha256Sum(b []byte) []byte { h := sha256.Sum256(b); return h[:] }
func mustBytes(b []byte, err error) []byte { must(err); return b }
func emit(v any) { b, _ := json.Marshal(v); fmt.Println(string(b)) }
