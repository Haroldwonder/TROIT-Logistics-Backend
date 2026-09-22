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
    pub is_archived: bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProductImage {
    pub id: Uuid,
    pub product_id: Uuid,
    pub url: String,
    pub storage_key: String,
    pub mime_type: String,
    pub file_size: i32,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductImageResponse {
    pub id: Uuid,
    pub product_id: Uuid,
    pub url: String,
    pub mime_type: String,
    pub file_size: i32,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

impl ProductImage {
    pub fn to_response(&self) -> ProductImageResponse {
        ProductImageResponse {
            id: self.id,
            product_id: self.product_id,
            url: self.url.clone(),
            mime_type: self.mime_type.clone(),
            file_size: self.file_size,
            sort_order: self.sort_order,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ReorderImagesRequest {
    pub image_ids: Vec<Uuid>,
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
pub struct UpdateProductRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub condition: Option<String>,
    pub is_african_made: Option<bool>,
    pub african_made_category: Option<String>,
    pub warranty_months: Option<i32>,
    pub warranty_terms: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStockRequest {
    pub stock: i32,
}

#[derive(Debug, Deserialize)]
pub struct ArchiveProductRequest {
    pub archived: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateVerificationRequest {
    pub verification_status: String,
}

#[derive(Debug, Clone, Serialize)]
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
    pub is_archived: bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub images: Vec<ProductImageResponse>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Product {
    pub fn to_response(&self, images: Vec<ProductImageResponse>) -> ProductResponse {
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
            is_archived: self.is_archived,
            archived_at: self.archived_at,
            images,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
