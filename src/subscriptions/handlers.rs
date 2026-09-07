use crate::{
    errors::AppError,
    models::{seller::SellerProfile, subscription::SellerSubscription, AppState, Claims, UserRole},
    subscriptions::{
        models::{SubscriptionResponse, UpdateSubscriptionRequest},
        service::get_subscription_entitlements,
    },
};
use axum::{extract::State, Extension, Json};
use chrono::{Duration, Utc};
use serde::Serialize;
use sqlx::query_as;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

/// GET /api/v1/seller/subscription
/// Fetches caller's active seller subscription and entitlements
pub async fn get_seller_subscription_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ApiResponse<SubscriptionResponse>>, AppError> {
    let profile: SellerProfile = query_as::<_, SellerProfile>(
        r#"
        INSERT INTO seller_profiles (user_id, trust_level, seller_grade)
        VALUES ($1, 'LV1', 'Grade C')
        ON CONFLICT (user_id) DO UPDATE SET updated_at = CURRENT_TIMESTAMP
        RETURNING id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        "#
    )
    .bind(claims.sub)
    .fetch_one(&state.db)
    .await?;

    let subscription: SellerSubscription = query_as::<_, SellerSubscription>(
        r#"
        INSERT INTO seller_subscriptions (seller_id, plan_tier, status)
        VALUES ($1, 'FREE', 'ACTIVE')
        ON CONFLICT (seller_id) DO UPDATE SET updated_at = CURRENT_TIMESTAMP
        RETURNING id, seller_id, plan_tier, status, expires_at, created_at, updated_at
        "#,
    )
    .bind(profile.id)
    .fetch_one(&state.db)
    .await?;

    let entitlements = get_subscription_entitlements(&subscription);

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched seller subscription state".to_string(),
        data: Some(SubscriptionResponse {
            id: subscription.id,
            seller_id: subscription.seller_id,
            plan_tier: subscription.plan_tier,
            status: subscription.status,
            expires_at: subscription.expires_at,
            entitlements,
            created_at: subscription.created_at,
        }),
    }))
}

/// POST /api/v1/seller/subscription
/// Admin or system endpoint to update seller subscription plan
pub async fn update_seller_subscription_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<UpdateSubscriptionRequest>,
) -> Result<Json<ApiResponse<SubscriptionResponse>>, AppError> {
    if claims.role != UserRole::Admin {
        return Err(AppError::Forbidden(
            "Only administrators can alter seller subscription tiers".to_string(),
        ));
    }

    let plan = payload.plan_tier.to_uppercase();
    if plan != "FREE" && plan != "PRO" && plan != "ENTERPRISE" && plan != "VIP" {
        return Err(AppError::ValidationError(
            "Invalid plan tier. Must be FREE, PRO, ENTERPRISE, or VIP".to_string(),
        ));
    }

    let target_seller_id = payload.seller_id.ok_or_else(|| {
        AppError::ValidationError("seller_id is required in admin update payload".to_string())
    })?;

    let expires_at = payload
        .duration_days
        .map(|days| Utc::now() + Duration::days(days));

    let subscription: SellerSubscription = query_as::<_, SellerSubscription>(
        r#"
        INSERT INTO seller_subscriptions (seller_id, plan_tier, status, expires_at)
        VALUES ($1, $2, 'ACTIVE', $3)
        ON CONFLICT (seller_id) DO UPDATE SET
            plan_tier = EXCLUDED.plan_tier,
            status = 'ACTIVE',
            expires_at = EXCLUDED.expires_at,
            updated_at = CURRENT_TIMESTAMP
        RETURNING id, seller_id, plan_tier, status, expires_at, created_at, updated_at
        "#,
    )
    .bind(target_seller_id)
    .bind(&plan)
    .bind(expires_at)
    .fetch_one(&state.db)
    .await?;

    let entitlements = get_subscription_entitlements(&subscription);

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Seller subscription plan updated to {}", plan),
        data: Some(SubscriptionResponse {
            id: subscription.id,
            seller_id: subscription.seller_id,
            plan_tier: subscription.plan_tier,
            status: subscription.status,
            expires_at: subscription.expires_at,
            entitlements,
            created_at: subscription.created_at,
        }),
    }))
}
