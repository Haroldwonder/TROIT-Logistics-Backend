use crate::models::UserRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct AdminSellerQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub search: Option<String>,
    pub verification_status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AdminUpdateSellerVerificationRequest {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminSellerItemResponse {
    pub seller_id: Uuid,
    pub user_id: Uuid,
    pub store_name: Option<String>,
    pub store_address: Option<String>,
    pub trust_level: String,
    pub seller_grade: String,
    pub successful_transactions: i32,
    pub fulfillment_rate: f64,
    pub verification_status: String,
    pub user_full_name: String,
    pub user_email: String,
    pub user_phone: Option<String>,
    pub total_products: i64,
    pub total_orders: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminSellerListResponse {
    pub items: Vec<AdminSellerItemResponse>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}

#[derive(Debug, Deserialize)]
pub struct AdminUserQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub role: Option<UserRole>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminUserItemResponse {
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
    pub phone_number: Option<String>,
    pub role: UserRole,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminUserListResponse {
    pub items: Vec<AdminUserItemResponse>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
