use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct TrustEvaluationResult {
    pub seller_id: Uuid,
    pub old_trust_level: String,
    pub new_trust_level: String,
    pub old_grade: String,
    pub new_grade: String,
    pub successful_transactions: i32,
    pub fulfillment_rate: f64,
    pub level_changed: bool,
}

#[derive(Debug, Serialize)]
pub struct TrustHistoryResponse {
    pub id: Uuid,
    pub seller_id: Uuid,
    pub old_level: String,
    pub new_level: String,
    pub reason: String,
    pub trigger_transaction_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
