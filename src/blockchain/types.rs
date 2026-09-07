#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    Submitted,
    Pending,
    Success,
    Failed,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SorobanRpcRequest<T> {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SorobanRpcResponse<T> {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcErrorData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcErrorData {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTransactionResponse {
    pub hash: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_result_xdr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_xdr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_meta_xdr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub envelope_xdr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateResultData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xdr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateTransactionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_resource_fee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<SimulateResultData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub hash: String,
    pub status: TransactionStatus,
    pub contract_id: String,
    pub function: String,
    pub escrow_id: u64,
    pub amount: Option<f64>,
    pub token: Option<String>,
    pub final_escrow_state: Option<EscrowState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ParsedTransactionSemantics {
    pub contract_id: Option<String>,
    pub function: Option<String>,
    pub escrow_id: Option<u64>,
    pub amount: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum EscrowState {
    Created = 1,
    Funded = 2,
    Released = 3,
    Refunded = 4,
    Disputed = 5,
}

impl<'de> Deserialize<'de> for EscrowState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let val = u32::deserialize(deserializer)?;
        match val {
            1 => Ok(EscrowState::Created),
            2 => Ok(EscrowState::Funded),
            3 => Ok(EscrowState::Released),
            4 => Ok(EscrowState::Refunded),
            5 => Ok(EscrowState::Disputed),
            _ => Err(serde::de::Error::custom(format!(
                "Unknown EscrowState code: {}",
                val
            ))),
        }
    }
}

impl Serialize for EscrowState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u32(*self as u32)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowRecordDto {
    pub escrow_id: u64,
    pub order_id: u64,
    pub buyer: String,
    pub seller: String,
    pub token: String,
    pub amount: i128,
    pub state: EscrowState,
    pub created_at: u64,
    pub funded_at: u64,
    pub released_at: u64,
    pub refunded_at: u64,
}
