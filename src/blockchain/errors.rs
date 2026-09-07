#![allow(dead_code)]

use crate::errors::AppError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BlockchainError {
    #[error("Invalid Configuration: {0}")]
    InvalidConfig(String),

    #[error("RPC Service Unavailable: {0}")]
    RpcUnavailable(String),

    #[error("Request Timeout: {0}")]
    Timeout(String),

    #[error("Transaction Rejected: {0}")]
    TransactionRejected(String),

    #[error("Transaction Failed: {0}")]
    TransactionFailed(String),

    #[error("Transaction Not Found: {0}")]
    TransactionNotFound(String),

    #[error("Contract Error: {0}")]
    ContractError(String),

    #[error("Invalid Response: {0}")]
    InvalidResponse(String),

    #[error("Semantic Verification Failed: {0}")]
    SemanticMismatch(String),

    #[error("Authentication Error: {0}")]
    AuthError(String),
}

impl From<BlockchainError> for AppError {
    fn from(err: BlockchainError) -> Self {
        AppError::BlockchainError(err.to_string())
    }
}
