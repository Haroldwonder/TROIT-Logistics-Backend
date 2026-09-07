use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateInspectionReportRequest {
    pub order_id: Option<Uuid>,
    pub authenticity_verified: bool,
    pub physical_condition: String,
    pub serial_number: Option<String>,
    pub functional_tests: Option<serde_json::Value>,
    pub photos_json: Option<serde_json::Value>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InspectionReportResponse {
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
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ProductVerificationSummaryResponse {
    pub product_id: Uuid,
    pub product_name: String,
    pub verification_status: String,
    pub authenticity_status: String,
    pub last_inspected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub physical_condition: String,
    pub seller_id: Uuid,
    pub seller_trust_level: Option<String>,
    pub seller_grade: Option<String>,
    pub has_inspection_report: bool,
}
