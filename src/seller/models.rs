use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct SellerPublicProfileResponse {
    pub seller_id: Uuid,
    pub user_id: Uuid,
    pub store_name: Option<String>,
    pub store_address: Option<String>,
    pub trust_level: String,
    pub seller_grade: String,
    pub successful_transactions: i32,
    pub fulfillment_rate: f64,
    pub verification_status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSellerProfileRequest {
    pub store_name: Option<String>,
    pub store_address: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SubmitVerificationRequest {
    pub store_name: String,
    pub store_address: String,
    pub id_type: Option<String>,
    pub id_number: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VerificationStatusResponse {
    pub seller_id: Uuid,
    pub verification_status: String,
    pub store_name: Option<String>,
    pub store_address: Option<String>,
    pub kyc_completed: bool,
    pub store_verified: bool,
    pub physical_inspection_passed: bool,
}
