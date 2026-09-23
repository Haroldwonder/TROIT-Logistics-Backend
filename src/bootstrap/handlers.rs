use crate::{
    bootstrap::models::BootstrapResponse,
    errors::AppError,
    models::{AppState, UserRole},
};
use axum::{extract::State, http::HeaderMap, Json};
use sha2::{Digest, Sha256};

const TARGET_ADMIN_EMAIL: &str = "admininout@troitlogistics.com";
const INITIAL_ADMIN_BOOTSTRAP_NAME: &str = "initial_admin_bootstrap";

/// Timing-attack resistant constant-time comparison of two secret strings via SHA-256 digests
fn constant_time_secret_match(provided: &str, configured: &str) -> bool {
    let hash_provided = Sha256::digest(provided.as_bytes());
    let hash_configured = Sha256::digest(configured.as_bytes());

    let mut diff = 0u8;
    for (b1, b2) in hash_provided.iter().zip(hash_configured.iter()) {
        diff |= b1 ^ b2;
    }
    diff == 0
}

/// POST /api/v1/bootstrap/admin
/// One-time secure server-side admin provisioning endpoint
pub async fn admin_bootstrap_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<BootstrapResponse>, AppError> {
    // 1. Verify ADMIN_BOOTSTRAP_SECRET exists in application configuration
    let configured_secret = match state.config.admin_bootstrap_secret.as_ref() {
        Some(secret) if !secret.is_empty() => secret,
        _ => {
            return Err(AppError::Forbidden(
                "Admin bootstrap is not configured or disabled".to_string(),
            ));
        }
    };

    // 2. Extract Bearer token from Authorization header
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            AppError::Unauthorized("Missing or invalid Authorization header".to_string())
        })?;

    let provided_secret = if let Some(token) = auth_header.strip_prefix("Bearer ") {
        token.trim()
    } else if let Some(token) = auth_header.strip_prefix("bearer ") {
        token.trim()
    } else {
        return Err(AppError::Unauthorized(
            "Authorization header must use Bearer scheme".to_string(),
        ));
    };

    // 3. Constant-time secret validation
    if !constant_time_secret_match(provided_secret, configured_secret) {
        return Err(AppError::Unauthorized(
            "Invalid bootstrap authorization secret".to_string(),
        ));
    }

    // 4. Execute atomic transaction for user promotion and bootstrap consumption
    let mut tx = state.db.begin().await?;

    // Lock and check bootstrap consumption state
    let bootstrap_row: Option<(Option<chrono::DateTime<chrono::Utc>>,)> = sqlx::query_as(
        "SELECT consumed_at FROM admin_bootstrap_state WHERE bootstrap_name = $1 FOR UPDATE",
    )
    .bind(INITIAL_ADMIN_BOOTSTRAP_NAME)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some((Some(_consumed_at),)) = bootstrap_row {
        let _ = tx.rollback().await;
        return Err(AppError::ValidationError(
            "Admin bootstrap has already been consumed".to_string(),
        ));
    }

    // Lock and check target user account
    let user_row: Option<(uuid::Uuid, UserRole)> =
        sqlx::query_as("SELECT id, role FROM users WHERE email = $1 FOR UPDATE")
            .bind(TARGET_ADMIN_EMAIL)
            .fetch_optional(&mut *tx)
            .await?;

    let (user_id, current_role) = match user_row {
        Some(row) => row,
        None => {
            let _ = tx.rollback().await;
            return Err(AppError::NotFound(format!(
                "Target admin user account {} does not exist. Please register the user account first.",
                TARGET_ADMIN_EMAIL
            )));
        }
    };

    // If user is already an admin, safely return idempotent response without consuming bootstrap or failing
    if current_role == UserRole::Admin {
        let _ = tx.rollback().await;
        return Ok(Json(BootstrapResponse {
            success: true,
            message: format!(
                "Admin account {} is already provisioned as admin",
                TARGET_ADMIN_EMAIL
            ),
        }));
    }

    // Promote target account role to admin and verify resulting role
    let updated_role: (UserRole,) = sqlx::query_as(
        "UPDATE users SET role = 'admin', updated_at = CURRENT_TIMESTAMP WHERE id = $1 AND email = $2 RETURNING role",
    )
    .bind(user_id)
    .bind(TARGET_ADMIN_EMAIL)
    .fetch_one(&mut *tx)
    .await?;

    if updated_role.0 != UserRole::Admin {
        let _ = tx.rollback().await;
        return Err(AppError::InternalServerError(
            "Failed to verify admin role promotion".to_string(),
        ));
    }

    // Mark initial admin bootstrap as consumed
    sqlx::query(
        r#"
        INSERT INTO admin_bootstrap_state (bootstrap_name, consumed_at)
        VALUES ($1, CURRENT_TIMESTAMP)
        ON CONFLICT (bootstrap_name) DO UPDATE SET consumed_at = EXCLUDED.consumed_at
        "#,
    )
    .bind(INITIAL_ADMIN_BOOTSTRAP_NAME)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(BootstrapResponse {
        success: true,
        message: "Admin account provisioned successfully".to_string(),
    }))
}
