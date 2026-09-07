use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct SubscriptionResponse {
    pub id: Uuid,
    pub seller_id: Uuid,
    pub plan_tier: String,
    pub status: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub entitlements: SubscriptionEntitlements,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct SubscriptionEntitlements {
    pub max_active_listings: Option<i64>,
    pub advanced_analytics_enabled: bool,
    pub business_insights_enabled: bool,
    pub priority_verification_enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSubscriptionRequest {
    pub seller_id: Option<Uuid>,
    pub plan_tier: String,
    pub duration_days: Option<i64>,
}
