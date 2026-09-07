#![allow(dead_code)]

use super::errors::BlockchainError;
use super::types::EscrowRecordDto;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEscrowArgs {
    pub escrow_id: u64,
    pub order_id: u64,
    pub buyer: String,
    pub seller: String,
    pub token: String,
    pub amount: i128,
}

#[derive(Debug, Clone)]
pub struct EscrowContractClient {
    contract_id: String,
}

impl EscrowContractClient {
    pub fn new(contract_id: String) -> Result<Self, BlockchainError> {
        if contract_id.trim().is_empty() {
            return Err(BlockchainError::InvalidConfig(
                "Soroban Escrow Contract ID is unconfigured".to_string(),
            ));
        }

        Ok(Self { contract_id })
    }

    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }

    pub fn build_create_escrow_params(&self, args: &CreateEscrowArgs) -> serde_json::Value {
        json!({
            "contract_id": self.contract_id,
            "function": "create_escrow",
            "args": [
                args.escrow_id,
                args.order_id,
                args.buyer,
                args.seller,
                args.token,
                args.amount
            ]
        })
    }

    pub fn build_fund_escrow_params(&self, escrow_id: u64) -> serde_json::Value {
        json!({
            "contract_id": self.contract_id,
            "function": "fund_escrow",
            "args": [escrow_id]
        })
    }

    pub fn build_release_escrow_params(&self, escrow_id: u64) -> serde_json::Value {
        json!({
            "contract_id": self.contract_id,
            "function": "release_escrow",
            "args": [escrow_id]
        })
    }

    pub fn build_refund_escrow_params(&self, escrow_id: u64) -> serde_json::Value {
        json!({
            "contract_id": self.contract_id,
            "function": "refund_escrow",
            "args": [escrow_id]
        })
    }

    pub fn build_dispute_escrow_params(&self, escrow_id: u64, caller: &str) -> serde_json::Value {
        json!({
            "contract_id": self.contract_id,
            "function": "dispute_escrow",
            "args": [escrow_id, caller]
        })
    }

    pub fn build_resolve_dispute_params(
        &self,
        escrow_id: u64,
        release_to_seller: bool,
    ) -> serde_json::Value {
        json!({
            "contract_id": self.contract_id,
            "function": "resolve_dispute",
            "args": [escrow_id, release_to_seller]
        })
    }

    pub fn parse_escrow_record(val: serde_json::Value) -> Result<EscrowRecordDto, BlockchainError> {
        serde_json::from_value(val)
            .map_err(|e| BlockchainError::InvalidResponse(format!("Invalid escrow record: {}", e)))
    }
}
