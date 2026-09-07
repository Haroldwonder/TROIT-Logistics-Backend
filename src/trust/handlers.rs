use crate::{
    errors::AppError,
    models::{seller::SellerTrustHistory, AppState, Claims},
    trust::models::TrustHistoryResponse,
};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::Serialize;
use sqlx::query_as;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

/// GET /api/v1/seller/trust/history
/// Retrieves caller's seller trust history
pub async fn get_seller_trust_history_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ApiResponse<Vec<TrustHistoryResponse>>>, AppError> {
    let history: Vec<SellerTrustHistory> = query_as::<_, SellerTrustHistory>(
        r#"
        SELECT h.id, h.seller_id, h.old_level, h.new_level, h.reason, h.trigger_transaction_id, h.created_at
        FROM seller_trust_history h
        JOIN seller_profiles p ON h.seller_id = p.id
        WHERE p.user_id = $1
        ORDER BY h.created_at DESC
        "#
    )
    .bind(claims.sub)
    .fetch_all(&state.db)
    .await?;

    let response: Vec<TrustHistoryResponse> = history
        .into_iter()
        .map(|h| TrustHistoryResponse {
            id: h.id,
            seller_id: h.seller_id,
            old_level: h.old_level,
            new_level: h.new_level,
            reason: h.reason,
            trigger_transaction_id: h.trigger_transaction_id,
            created_at: h.created_at,
        })
        .collect();

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched seller trust history".to_string(),
        data: Some(response),
    }))
}

/// GET /api/v1/seller/trust/history/:seller_id
/// Public / Admin viewing of trust history for a seller
pub async fn get_seller_trust_history_by_id_handler(
    State(state): State<AppState>,
    Path(seller_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<TrustHistoryResponse>>>, AppError> {
    let history: Vec<SellerTrustHistory> = query_as::<_, SellerTrustHistory>(
        r#"
        SELECT id, seller_id, old_level, new_level, reason, trigger_transaction_id, created_at
        FROM seller_trust_history
        WHERE seller_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(seller_id)
    .fetch_all(&state.db)
    .await?;

    let response: Vec<TrustHistoryResponse> = history
        .into_iter()
        .map(|h| TrustHistoryResponse {
            id: h.id,
            seller_id: h.seller_id,
            old_level: h.old_level,
            new_level: h.new_level,
            reason: h.reason,
            trigger_transaction_id: h.trigger_transaction_id,
            created_at: h.created_at,
        })
        .collect();

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched seller trust history".to_string(),
        data: Some(response),
    }))
}
