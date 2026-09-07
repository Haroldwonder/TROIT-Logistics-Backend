use crate::products::models::ProductResponse;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct WishlistItemResponse {
    pub id: Uuid,
    pub buyer_id: Uuid,
    pub product_id: Uuid,
    pub product: Option<ProductResponse>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
