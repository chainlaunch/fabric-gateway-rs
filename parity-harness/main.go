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
	"google.golang.org/grpc/credentials"
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

	// The live commands build their identity from FABRIC_* env via connectLive();
	// only the offline/parity commands need the local cert.pem/key.pem fixture.
	var id *identity.X509Identity
	var key *ecdsa.PrivateKey
	if os.Args[1] != "live-evaluate" && os.Args[1] != "live-submit" {
		id, key = loadIdentity()
	}

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

	case "conformance":
		// Differential conformance: read a JSON array of proposal cases and emit
		// each one's serialized ProposedTransaction bytes (hex). The Rust side
		// rebuilds each case with the same nonce+timestamp (extracted from these
		// bytes) and asserts byte-for-byte equality.
		conformance(id, key, os.Args[2])

	case "live-evaluate":
		// Connect to a live gateway (FABRIC_* env) and evaluate a transaction.
		gw, conn := connectLive()
		defer conn.Close()
		contract := gw.GetNetwork(os.Getenv("FABRIC_CHANNEL")).GetContract(os.Getenv("FABRIC_CHAINCODE"))
		result, err := contract.EvaluateTransaction(os.Args[2], os.Args[3:]...)
		must(err)
		fmt.Println(string(result))

	case "live-submit":
		// Connect to a live gateway (FABRIC_* env) and submit a transaction.
		gw, conn := connectLive()
		defer conn.Close()
		contract := gw.GetNetwork(os.Getenv("FABRIC_CHANNEL")).GetContract(os.Getenv("FABRIC_CHAINCODE"))
		result, err := contract.SubmitTransaction(os.Args[2], os.Args[3:]...)
		must(err)
		fmt.Printf("committed: %s\n", string(result))

	default:
		panic("unknown command: " + os.Args[1])
	}
}

// connectLive builds a Gateway against a real peer using FABRIC_* environment
// variables (endpoint, TLS CA, cert, key, MSP). Used for cross-SDK interop.
func connectLive() (*client.Gateway, *grpc.ClientConn) {
	caPEM, err := os.ReadFile(os.Getenv("FABRIC_TLS_CA"))
	must(err)
	pool := x509.NewCertPool()
	if !pool.AppendCertsFromPEM(caPEM) {
		panic("failed to add TLS CA cert")
	}
	creds := credentials.NewClientTLSFromCert(pool, os.Getenv("FABRIC_OVERRIDE"))
	conn, err := grpc.NewClient(os.Getenv("FABRIC_ENDPOINT"), grpc.WithTransportCredentials(creds))
	must(err)

	certPEM, err := os.ReadFile(os.Getenv("FABRIC_CERT"))
	must(err)
	cert, err := identity.CertificateFromPEM(certPEM)
	must(err)
	id, err := identity.NewX509Identity(os.Getenv("FABRIC_MSP"), cert)
	must(err)

	keyPEM, err := os.ReadFile(os.Getenv("FABRIC_KEY"))
	must(err)
	priv, err := identity.PrivateKeyFromPEM(keyPEM)
	must(err)
	sign, err := identity.NewPrivateKeySign(priv)
	must(err)

	gw, err := client.Connect(id, client.WithSign(sign), client.WithClientConnection(conn))
	must(err)
	return gw, conn
}

// proposalCase mirrors the JSON the Rust conformance test writes.
type proposalCase struct {
	Channel       string            `json:"channel"`
	Chaincode     string            `json:"chaincode"`
	Function      string            `json:"function"`
	Args          []string          `json:"args"`
	Transient     map[string]string `json:"transient"`
	EndorsingOrgs []string          `json:"endorsingOrgs"`
}

func conformance(id *identity.X509Identity, key *ecdsa.PrivateKey, casesFile string) {
	gw := connectOffline(id, key)
	raw, err := os.ReadFile(casesFile)
	must(err)
	var cases []proposalCase
	must(json.Unmarshal(raw, &cases))

	out := make([]string, 0, len(cases))
	for _, c := range cases {
		opts := []client.ProposalOption{client.WithArguments(c.Args...)}
		if len(c.Transient) > 0 {
			transient := make(map[string][]byte, len(c.Transient))
			for k, v := range c.Transient {
				transient[k] = []byte(v)
			}
			opts = append(opts, client.WithTransient(transient))
		}
		if len(c.EndorsingOrgs) > 0 {
			opts = append(opts, client.WithEndorsingOrganizations(c.EndorsingOrgs...))
		}
		p, err := gw.GetNetwork(c.Channel).GetContract(c.Chaincode).NewProposal(c.Function, opts...)
		must(err)
		out = append(out, hex.EncodeToString(mustBytes(p.Bytes())))
	}
	emit(out)
}

func sha256Sum(b []byte) []byte { h := sha256.Sum256(b); return h[:] }
func mustBytes(b []byte, err error) []byte { must(err); return b }
func emit(v any) { b, _ := json.Marshal(v); fmt.Println(string(b)) }
