use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Buyer Wishlist Saved Product entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct Wishlist {
    pub id: Uuid,
    pub buyer_id: Uuid,
    pub product_id: Uuid,
    pub created_at: DateTime<Utc>,
}
