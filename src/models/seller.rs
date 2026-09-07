use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;

/// Seller Trust Levels supported by TROIT (LV1 to LV5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[sqlx(type_name = "VARCHAR", rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(dead_code)]
pub enum SellerTrustLevel {
    #[default]
    LV1,
    LV2,
    LV3,
    LV4,
    LV5,
}

/// Seller Trust Grade (Grade C, Grade B, Grade A)
/// IMPORTANT: Represents supplier trust built with Troit, NOT product physical condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[sqlx(type_name = "VARCHAR")]
#[allow(dead_code)]
pub enum SellerGrade {
    #[serde(rename = "Grade C")]
    #[default]
    GradeC,
    #[serde(rename = "Grade B")]
    GradeB,
    #[serde(rename = "Grade A")]
    GradeA,
}

/// Seller Profile entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct SellerProfile {
    pub id: Uuid,
    pub user_id: Uuid,
    pub store_name: Option<String>,
    pub store_address: Option<String>,
    pub trust_level: String,
    pub seller_grade: String,
    pub successful_transactions: i32,
    pub fulfillment_rate: f64,
    pub verification_status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Seller Trust Level Audit History record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct SellerTrustHistory {
    pub id: Uuid,
    pub seller_id: Uuid,
    pub old_level: String,
    pub new_level: String,
    pub reason: String,
    pub trigger_transaction_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
