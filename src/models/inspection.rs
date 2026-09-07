use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Formal Inspection Report entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct InspectionReport {
    pub id: Uuid,
    pub product_id: Uuid,
    pub order_id: Option<Uuid>,
    pub inspector_id: Option<Uuid>,
    pub authenticity_verified: bool,
    pub physical_condition: String,
    pub serial_number: Option<String>,
    pub functional_tests: Option<serde_json::Value>,
    pub photos_json: Option<serde_json::Value>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
