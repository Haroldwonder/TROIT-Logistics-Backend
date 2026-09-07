use crate::{
    errors::AppError,
    models::{notification::Notification, AppState, Claims},
    notifications::models::NotificationResponse,
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

/// GET /api/v1/notifications
/// Lists notifications scoped to the authenticated caller
pub async fn list_notifications_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ApiResponse<Vec<NotificationResponse>>>, AppError> {
    let notifications: Vec<Notification> = query_as::<_, Notification>(
        r#"
        SELECT id, user_id, title, message, notification_type, is_read, created_at
        FROM notifications
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT 50
        "#,
    )
    .bind(claims.sub)
    .fetch_all(&state.db)
    .await?;

    let response: Vec<NotificationResponse> = notifications
        .into_iter()
        .map(|n| NotificationResponse {
            id: n.id,
            user_id: n.user_id,
            title: n.title,
            message: n.message,
            notification_type: n.notification_type,
            is_read: n.is_read,
            created_at: n.created_at,
        })
        .collect();

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched notifications".to_string(),
        data: Some(response),
    }))
}

/// PATCH /api/v1/notifications/:id/read
/// Marks a single notification as read
pub async fn mark_notification_read_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<NotificationResponse>>, AppError> {
    let notification: Notification = query_as::<_, Notification>(
        r#"
        UPDATE notifications
        SET is_read = TRUE
        WHERE id = $1 AND user_id = $2
        RETURNING id, user_id, title, message, notification_type, is_read, created_at
        "#,
    )
    .bind(id)
    .bind(claims.sub)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Notification not found".to_string()))?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Notification marked as read".to_string(),
        data: Some(NotificationResponse {
            id: notification.id,
            user_id: notification.user_id,
            title: notification.title,
            message: notification.message,
            notification_type: notification.notification_type,
            is_read: notification.is_read,
            created_at: notification.created_at,
        }),
    }))
}

/// PATCH /api/v1/notifications/read-all
/// Marks all notifications for the caller as read
pub async fn mark_all_notifications_read_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    sqlx::query("UPDATE notifications SET is_read = TRUE WHERE user_id = $1 AND is_read = FALSE")
        .bind(claims.sub)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "All notifications marked as read".to_string(),
        data: None,
    }))
}
