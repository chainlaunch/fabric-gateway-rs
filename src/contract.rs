// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

//! Smart contract handle. Ports `contract.go`.

use crate::client::GrpcClient;
use crate::error::GatewayError;
use crate::proposal::ProposalBuilder;
use crate::signing::SigningIdentity;

/// Represents a smart contract, allowing evaluation (query) and submission
/// (ledger update) of transactions.
pub struct Contract {
    pub(crate) client: GrpcClient,
    pub(crate) signing_id: SigningIdentity,
    pub(crate) channel_name: String,
    pub(crate) chaincode_name: String,
    pub(crate) contract_name: String,
}

impl Contract {
    /// Name of the chaincode containing this contract.
    pub fn chaincode_name(&self) -> &str {
        &self.chaincode_name
    }

    /// Contract name within the chaincode (empty for the default contract).
    pub fn contract_name(&self) -> &str {
        &self.contract_name
    }

    /// Evaluate a transaction function with string arguments and return its
    /// result. Equivalent to `evaluate(name).with_arguments(args).build()...`.
    pub async fn evaluate_transaction(
        &self,
        name: &str,
        args: &[&str],
    ) -> Result<Vec<u8>, GatewayError> {
        self.new_proposal(name)
            .with_arguments(args.iter().map(|s| s.to_string()))
            .build()?
            .evaluate()
            .await
    }

    /// Submit a transaction to the ledger and return its result only after it
    /// has committed. Returns [`GatewayError::Commit`] if it commits invalid.
    pub async fn submit_transaction(
        &self,
        name: &str,
        args: &[&str],
    ) -> Result<Vec<u8>, GatewayError> {
        let (result, commit) = self.submit_async(name, args).await?;
        let status = commit.status().await?;
        if !status.successful {
            return Err(GatewayError::Commit {
                transaction_id: status.transaction_id,
                code: status.code,
            });
        }
        Ok(result)
    }

    /// Submit a transaction and return its result immediately after the orderer
    /// accepts it, along with the [`crate::Commit`] to await ledger commit.
    pub async fn submit_async(
        &self,
        name: &str,
        args: &[&str],
    ) -> Result<(Vec<u8>, crate::Commit), GatewayError> {
        let transaction = self
            .new_proposal(name)
            .with_arguments(args.iter().map(|s| s.to_string()))
            .build()?
            .endorse()
            .await?;
        let result = transaction.result().to_vec();
        let commit = transaction.submit().await?;
        Ok((result, commit))
    }

    /// Start building a proposal for finer-grained control (transient data,
    /// endorsing orgs, off-line signing).
    pub fn new_proposal(&self, transaction_name: &str) -> ProposalBuilder<'_> {
        ProposalBuilder::new(
            self.client.clone(),
            self.signing_id.clone(),
            self.channel_name.clone(),
            self.chaincode_name.clone(),
            self.qualified_transaction_name(transaction_name),
        )
    }

    fn qualified_transaction_name(&self, name: &str) -> String {
        if self.contract_name.is_empty() {
            name.to_string()
        } else {
            format!("{}:{}", self.contract_name, name)
        }
    }
}
