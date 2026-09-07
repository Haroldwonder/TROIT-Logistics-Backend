use crate::{errors::AppError, models::notification::Notification};
use sqlx::{query_as, PgPool};
use tracing::info;
use uuid::Uuid;

/// Reusable helper to create a database-backed notification for a user
pub async fn send_notification(
    db: &PgPool,
    user_id: Uuid,
    title: &str,
    message: &str,
    notification_type: &str,
) -> Result<Notification, AppError> {
    let notification: Notification = query_as::<_, Notification>(
        r#"
        INSERT INTO notifications (user_id, title, message, notification_type, is_read)
        VALUES ($1, $2, $3, $4, FALSE)
        RETURNING id, user_id, title, message, notification_type, is_read, created_at
        "#,
    )
    .bind(user_id)
    .bind(title)
    .bind(message)
    .bind(notification_type)
    .fetch_one(db)
    .await?;

    info!(
        "Notification created for user {} (type: {}): {}",
        user_id, notification_type, title
    );

    Ok(notification)
}
