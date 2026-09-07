use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Product verification status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(dead_code)]
pub enum VerificationStatus {
    Pending,
    Verified,
    Rejected,
}

#[allow(dead_code)]
impl VerificationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VerificationStatus::Pending => "PENDING",
            VerificationStatus::Verified => "VERIFIED",
            VerificationStatus::Rejected => "REJECTED",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "VERIFIED" => VerificationStatus::Verified,
            "REJECTED" => VerificationStatus::Rejected,
            _ => VerificationStatus::Pending,
        }
    }
}

/// Core Product entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Product {
    pub id: Uuid,
    pub seller_id: Uuid,
    pub name: String,
    pub description: String,
    pub price: f64,
    pub condition: String,
    pub stock: i32,
    pub verification_status: String,
    pub authenticity_status: String,
    pub last_inspected_at: Option<DateTime<Utc>>,
    pub is_african_made: bool,
    pub african_made_category: Option<String>,
    pub warranty_months: i32,
    pub warranty_terms: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProductRequest {
    pub name: String,
    pub description: String,
    pub price: f64,
    pub condition: Option<String>,
    pub stock: Option<i32>,
    pub is_african_made: Option<bool>,
    pub african_made_category: Option<String>,
    pub warranty_months: Option<i32>,
    pub warranty_terms: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateVerificationRequest {
    pub verification_status: String,
}

#[derive(Debug, Serialize)]
pub struct ProductResponse {
    pub id: Uuid,
    pub seller_id: Uuid,
    pub name: String,
    pub description: String,
    pub price: f64,
    pub condition: String,
    pub stock: i32,
    pub verification_status: String,
    pub authenticity_status: String,
    pub last_inspected_at: Option<DateTime<Utc>>,
    pub is_african_made: bool,
    pub african_made_category: Option<String>,
    pub warranty_months: i32,
    pub warranty_terms: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Product {
    pub fn to_response(&self) -> ProductResponse {
        ProductResponse {
            id: self.id,
            seller_id: self.seller_id,
            name: self.name.clone(),
            description: self.description.clone(),
            price: self.price,
            condition: self.condition.clone(),
            stock: self.stock,
            verification_status: self.verification_status.clone(),
            authenticity_status: self.authenticity_status.clone(),
            last_inspected_at: self.last_inspected_at,
            is_african_made: self.is_african_made,
            african_made_category: self.african_made_category.clone(),
            warranty_months: self.warranty_months,
            warranty_terms: self.warranty_terms.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
