#![allow(dead_code)]

use super::client::SorobanRpcClient;
use super::errors::BlockchainError;
use super::escrow::{CreateEscrowArgs, EscrowContractClient};
use super::signer::SorobanSigner;
use super::transaction::TransactionExecutor;
use super::types::{ExecutionResult, HealthResponse, TransactionStatus};
use crate::config::AppConfig;
use std::sync::Arc;
use stellar_xdr::curr::ScVal;
use tracing::{info, instrument};

#[derive(Debug, Clone)]
pub struct BlockchainService {
    rpc_client: SorobanRpcClient,
    escrow_client: Option<EscrowContractClient>,
    executor: Option<Arc<TransactionExecutor>>,
    network_passphrase: String,
}

impl BlockchainService {
    pub fn new(config: &AppConfig) -> Result<Self, BlockchainError> {
        let rpc_client = SorobanRpcClient::new(config.stellar_rpc_url.clone())?;

        let escrow_client = if !config.soroban_escrow_contract_id.trim().is_empty() {
            Some(EscrowContractClient::new(
                config.soroban_escrow_contract_id.clone(),
            )?)
        } else {
            None
        };

        let service_signer = if !config.soroban_service_secret_key.trim().is_empty() {
            Some(SorobanSigner::from_secret_key(
                &config.soroban_service_secret_key,
            )?)
        } else if !config.soroban_admin_secret_key.trim().is_empty() {
            Some(SorobanSigner::from_secret_key(
                &config.soroban_admin_secret_key,
            )?)
        } else {
            None
        };

        let executor = escrow_client.as_ref().map(|client| {
            Arc::new(TransactionExecutor::new(
                rpc_client.clone(),
                client.contract_id().to_string(),
                config.stellar_network_passphrase.clone(),
                service_signer,
                config.soroban_max_fee,
            ))
        });

        Ok(Self {
            rpc_client,
            escrow_client,
            executor,
            network_passphrase: config.stellar_network_passphrase.clone(),
        })
    }

    pub fn executor(&self) -> Result<&TransactionExecutor, BlockchainError> {
        self.executor.as_deref().ok_or_else(|| {
            BlockchainError::InvalidConfig("TransactionExecutor unconfigured".to_string())
        })
    }

    pub fn rpc_client(&self) -> &SorobanRpcClient {
        &self.rpc_client
    }

    pub fn escrow_client(&self) -> Result<&EscrowContractClient, BlockchainError> {
        self.escrow_client.as_ref().ok_or_else(|| {
            BlockchainError::InvalidConfig(
                "SOROBAN_ESCROW_CONTRACT_ID is not configured in backend environment".to_string(),
            )
        })
    }

    pub fn network_passphrase(&self) -> &str {
        &self.network_passphrase
    }

    #[instrument(skip(self))]
    pub async fn check_health(&self) -> Result<HealthResponse, BlockchainError> {
        self.rpc_client.check_health().await
    }

    #[instrument(skip(self), fields(escrow_id = args.escrow_id, order_id = args.order_id))]
    pub fn prepare_create_escrow_invocation(
        &self,
        args: &CreateEscrowArgs,
    ) -> Result<serde_json::Value, BlockchainError> {
        let client = self.escrow_client()?;
        info!(
            "Preparing create_escrow invocation for escrow_id={} order_id={}",
            args.escrow_id, args.order_id
        );
        Ok(client.build_create_escrow_params(args))
    }

    #[instrument(skip(self))]
    pub fn prepare_fund_escrow_invocation(
        &self,
        escrow_id: u64,
    ) -> Result<serde_json::Value, BlockchainError> {
        let client = self.escrow_client()?;
        info!(
            "Preparing fund_escrow invocation for escrow_id={}",
            escrow_id
        );
        Ok(client.build_fund_escrow_params(escrow_id))
    }

    #[instrument(skip(self))]
    pub fn prepare_release_escrow_invocation(
        &self,
        escrow_id: u64,
    ) -> Result<serde_json::Value, BlockchainError> {
        let client = self.escrow_client()?;
        info!(
            "Preparing release_escrow invocation for escrow_id={}",
            escrow_id
        );
        Ok(client.build_release_escrow_params(escrow_id))
    }

    #[instrument(skip(self))]
    pub fn prepare_refund_escrow_invocation(
        &self,
        escrow_id: u64,
    ) -> Result<serde_json::Value, BlockchainError> {
        let client = self.escrow_client()?;
        info!(
            "Preparing refund_escrow invocation for escrow_id={}",
            escrow_id
        );
        Ok(client.build_refund_escrow_params(escrow_id))
    }

    #[instrument(skip(self))]
    pub async fn execute_create_escrow(
        &self,
        escrow_id: u64,
        order_id: u64,
        buyer: &str,
        seller: &str,
        token: &str,
        amount: f64,
    ) -> Result<ExecutionResult, BlockchainError> {
        info!(
            "Executing create_escrow on-chain for escrow_id={}",
            escrow_id
        );
        let executor = self.executor()?;
        let buyer_addr = if buyer.starts_with('C') {
            TransactionExecutor::parse_contract_address(buyer)?
        } else {
            TransactionExecutor::parse_account_address(buyer)?
        };
        let seller_addr = if seller.starts_with('C') {
            TransactionExecutor::parse_contract_address(seller)?
        } else {
            TransactionExecutor::parse_account_address(seller)?
        };
        let token_addr = if token.starts_with('C') {
            TransactionExecutor::parse_contract_address(token)?
        } else {
            TransactionExecutor::parse_account_address(token)?
        };
        let raw_i128 = (amount * 10_000_000.0) as i128;
        let hi = (raw_i128 >> 64) as i64;
        let lo = (raw_i128 & 0xFFFF_FFFF_FFFF_FFFF) as u64;
        let amount_sc = ScVal::I128(stellar_xdr::curr::Int128Parts { hi, lo });

        let args = vec![
            ScVal::U64(escrow_id),
            ScVal::U64(order_id),
            ScVal::Address(buyer_addr),
            ScVal::Address(seller_addr),
            ScVal::Address(token_addr),
            amount_sc,
        ];

        executor
            .execute_contract_call("create_escrow", args, escrow_id, Some(amount), None)
            .await
    }

    #[instrument(skip(self))]
    pub async fn execute_fund_escrow(
        &self,
        escrow_id: u64,
        amount: f64,
    ) -> Result<ExecutionResult, BlockchainError> {
        info!("Executing fund_escrow on-chain for escrow_id={}", escrow_id);
        let executor = self.executor()?;
        let args = vec![ScVal::U64(escrow_id)];
        executor
            .execute_contract_call("fund_escrow", args, escrow_id, Some(amount), None)
            .await
    }

    #[instrument(skip(self))]
    pub async fn execute_release_escrow(
        &self,
        escrow_id: u64,
    ) -> Result<ExecutionResult, BlockchainError> {
        info!(
            "Executing release_escrow on-chain for escrow_id={}",
            escrow_id
        );
        let executor = self.executor()?;
        let args = vec![ScVal::U64(escrow_id)];
        executor
            .execute_contract_call("release_escrow", args, escrow_id, None, None)
            .await
    }

    #[instrument(skip(self))]
    pub async fn execute_refund_escrow(
        &self,
        escrow_id: u64,
    ) -> Result<ExecutionResult, BlockchainError> {
        info!(
            "Executing refund_escrow on-chain for escrow_id={}",
            escrow_id
        );
        let executor = self.executor()?;
        let args = vec![ScVal::U64(escrow_id)];
        executor
            .execute_contract_call("refund_escrow", args, escrow_id, None, None)
            .await
    }

    #[instrument(skip(self))]
    pub async fn execute_dispute_escrow(
        &self,
        escrow_id: u64,
        caller: &str,
    ) -> Result<ExecutionResult, BlockchainError> {
        info!(
            "Executing dispute_escrow on-chain for escrow_id={}",
            escrow_id
        );
        let executor = self.executor()?;
        let caller_addr = if caller.starts_with('C') {
            TransactionExecutor::parse_contract_address(caller)?
        } else {
            TransactionExecutor::parse_account_address(caller)?
        };
        let args = vec![ScVal::U64(escrow_id), ScVal::Address(caller_addr)];
        executor
            .execute_contract_call("dispute_escrow", args, escrow_id, None, None)
            .await
    }

    #[instrument(skip(self))]
    pub async fn execute_resolve_dispute(
        &self,
        escrow_id: u64,
        release_to_seller: bool,
    ) -> Result<ExecutionResult, BlockchainError> {
        info!(
            "Executing resolve_dispute on-chain for escrow_id={} release_to_seller={}",
            escrow_id, release_to_seller
        );
        let executor = self.executor()?;
        let args = vec![ScVal::U64(escrow_id), ScVal::Bool(release_to_seller)];
        executor
            .execute_contract_call("resolve_dispute", args, escrow_id, None, None)
            .await
    }

    #[instrument(skip(self))]
    pub async fn verify_transaction_confirmation(
        &self,
        hash: &str,
    ) -> Result<TransactionStatus, BlockchainError> {
        info!("Verifying transaction confirmation for hash={}", hash);
        self.rpc_client
            .poll_transaction_confirmation(hash, 10, 1000)
            .await
    }

    #[instrument(skip(self))]
    pub async fn verify_transaction_semantics(
        &self,
        hash: &str,
        expected_function: &str,
        expected_escrow_id: u64,
        expected_amount: Option<f64>,
    ) -> Result<TransactionStatus, BlockchainError> {
        info!(
            "Verifying transaction semantics for hash={} expected_func={} expected_escrow_id={}",
            hash, expected_function, expected_escrow_id
        );

        // Fail early if Soroban escrow contract is unconfigured
        let escrow_client = self.escrow_client()?;

        // Reject mock transaction hash prefixes in production verification
        if hash.starts_with("tx-fund-")
            || hash.starts_with("tx-release-")
            || hash.starts_with("tx-test-")
        {
            return Err(BlockchainError::SemanticMismatch(
                "Mock transaction hashes are not accepted for verification.".to_string(),
            ));
        }

        let (status, semantics) = self.rpc_client.get_transaction_semantics(hash).await?;
        if status != TransactionStatus::Success {
            return Ok(status);
        }

        // If parsed semantics are missing (stubbed XDR parser), fail verification safely
        if semantics.contract_id.is_none()
            && semantics.function.is_none()
            && semantics.escrow_id.is_none()
        {
            return Err(BlockchainError::SemanticMismatch(
                "On-chain transaction XDR semantic verification is not implemented. Cannot verify transaction authenticity.".to_string(),
            ));
        }

        // 1. Verify Contract ID
        let configured_contract_id = escrow_client.contract_id();
        if let Some(parsed_contract) = &semantics.contract_id {
            if parsed_contract != configured_contract_id {
                return Err(BlockchainError::SemanticMismatch(format!(
                    "Contract ID mismatch: expected {}, found {}",
                    configured_contract_id, parsed_contract
                )));
            }
        } else {
            return Err(BlockchainError::SemanticMismatch(
                "Contract ID missing in transaction semantics".to_string(),
            ));
        }

        // 2. Verify Invoked Function Name
        if let Some(parsed_func) = &semantics.function {
            if parsed_func != expected_function {
                return Err(BlockchainError::SemanticMismatch(format!(
                    "Function mismatch: expected {}, found {}",
                    expected_function, parsed_func
                )));
            }
        } else {
            return Err(BlockchainError::SemanticMismatch(
                "Function name missing in transaction semantics".to_string(),
            ));
        }

        // 3. Verify Escrow ID
        if let Some(parsed_escrow) = semantics.escrow_id {
            if parsed_escrow != expected_escrow_id {
                return Err(BlockchainError::SemanticMismatch(format!(
                    "Escrow ID mismatch: expected {}, found {}",
                    expected_escrow_id, parsed_escrow
                )));
            }
        } else {
            return Err(BlockchainError::SemanticMismatch(
                "Escrow ID missing in transaction semantics".to_string(),
            ));
        }

        // 4. Verify Amount
        if let (Some(expected_amt), Some(parsed_amt)) = (expected_amount, semantics.amount) {
            if (expected_amt - parsed_amt).abs() > 0.001 {
                return Err(BlockchainError::SemanticMismatch(format!(
                    "Amount mismatch: expected {}, found {}",
                    expected_amt, parsed_amt
                )));
            }
        }

        Ok(TransactionStatus::Success)
    }
}

pub type SharedBlockchainService = Arc<BlockchainService>;
