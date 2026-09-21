use crate::{
    admin::models::{
        AdminSellerItemResponse, AdminSellerListResponse, AdminSellerQuery, AdminUserItemResponse,
        AdminUserListResponse, AdminUserQuery,
    },
    errors::AppError,
    models::{AppState, Claims, UserRole},
};
use axum::{
    extract::{Query, State},
    Extension, Json,
};
use serde::Serialize;
use sqlx::{FromRow, Row};

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

#[derive(Debug, FromRow)]
struct SellerQueryRow {
    seller_id: uuid::Uuid,
    user_id: uuid::Uuid,
    store_name: Option<String>,
    store_address: Option<String>,
    trust_level: String,
    seller_grade: String,
    successful_transactions: i32,
    fulfillment_rate: f64,
    verification_status: String,
    user_full_name: String,
    user_email: String,
    user_phone: Option<String>,
    total_products: i64,
    total_orders: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

/// GET /api/v1/admin/sellers
/// Admin-only seller directory endpoint returning store, trust metrics, user details, and activity counts
pub async fn list_admin_sellers_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<AdminSellerQuery>,
) -> Result<Json<ApiResponse<AdminSellerListResponse>>, AppError> {
    // 1. Strict Server-Side Authorization Check
    if claims.role != UserRole::Admin {
        return Err(AppError::Forbidden(
            "Access denied: Admin role required".to_string(),
        ));
    }

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let search_pattern = query.search.as_ref().map(|s| format!("%{}%", s.trim()));
    let status_filter = query.verification_status.as_ref().map(|s| s.trim().to_uppercase());

    // Fetch total matching sellers count
    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM seller_profiles sp
        JOIN users u ON sp.user_id = u.id
        WHERE ($1::text IS NULL OR u.email ILIKE $1 OR u.full_name ILIKE $1 OR sp.store_name ILIKE $1)
          AND ($2::text IS NULL OR UPPER(sp.verification_status) = $2)
        "#,
    )
    .bind(&search_pattern)
    .bind(&status_filter)
    .fetch_one(&state.db)
    .await?;

    // Fetch paginated seller records
    let rows = sqlx::query_as::<_, SellerQueryRow>(
        r#"
        SELECT 
            sp.id AS seller_id,
            sp.user_id,
            sp.store_name,
            sp.store_address,
            sp.trust_level,
            sp.seller_grade,
            sp.successful_transactions,
            sp.fulfillment_rate,
            sp.verification_status,
            u.full_name AS user_full_name,
            u.email AS user_email,
            u.phone_number AS user_phone,
            (SELECT COUNT(*) FROM products p WHERE p.seller_id = u.id) AS total_products,
            (SELECT COUNT(*) FROM orders o WHERE o.seller_id = u.id) AS total_orders,
            sp.created_at,
            sp.updated_at
        FROM seller_profiles sp
        JOIN users u ON sp.user_id = u.id
        WHERE ($1::text IS NULL OR u.email ILIKE $1 OR u.full_name ILIKE $1 OR sp.store_name ILIKE $1)
          AND ($2::text IS NULL OR UPPER(sp.verification_status) = $2)
        ORDER BY sp.created_at DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(&search_pattern)
    .bind(&status_filter)
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<AdminSellerItemResponse> = rows
        .into_iter()
        .map(|r| AdminSellerItemResponse {
            seller_id: r.seller_id,
            user_id: r.user_id,
            store_name: r.store_name,
            store_address: r.store_address,
            trust_level: r.trust_level,
            seller_grade: r.seller_grade,
            successful_transactions: r.successful_transactions,
            fulfillment_rate: r.fulfillment_rate,
            verification_status: r.verification_status,
            user_full_name: r.user_full_name,
            user_email: r.user_email,
            user_phone: r.user_phone,
            total_products: r.total_products,
            total_orders: r.total_orders,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();

    let total_pages = (total as f64 / limit as f64).ceil() as u64;

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched seller directory successfully".to_string(),
        data: Some(AdminSellerListResponse {
            items,
            total: total as u64,
            page,
            limit,
            total_pages,
        }),
    }))
}

/// GET /api/v1/admin/users
/// Admin-only user directory endpoint returning platform user records excluding passwords & credentials
pub async fn list_admin_users_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<AdminUserQuery>,
) -> Result<Json<ApiResponse<AdminUserListResponse>>, AppError> {
    // 1. Strict Server-Side Authorization Check
    if claims.role != UserRole::Admin {
        return Err(AppError::Forbidden(
            "Access denied: Admin role required".to_string(),
        ));
    }

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let search_pattern = query.search.as_ref().map(|s| format!("%{}%", s.trim()));
    let role_str = query.role.map(|r| r.to_string());

    // Count total matching users
    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM users
        WHERE ($1::text IS NULL OR email ILIKE $1 OR full_name ILIKE $1)
          AND ($2::text IS NULL OR role::text = $2)
        "#,
    )
    .bind(&search_pattern)
    .bind(&role_str)
    .fetch_one(&state.db)
    .await?;

    // Fetch user records excluding password_hash
    let rows = sqlx::query(
        r#"
        SELECT id, email, full_name, phone_number, role, is_active, created_at, updated_at
        FROM users
        WHERE ($1::text IS NULL OR email ILIKE $1 OR full_name ILIKE $1)
          AND ($2::text IS NULL OR role::text = $2)
        ORDER BY created_at DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(&search_pattern)
    .bind(&role_str)
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(&state.db)
    .await?;

    let mut items = Vec::new();
    for row in rows {
        let role_val: UserRole = row.try_get("role")?;
        items.push(AdminUserItemResponse {
            id: row.try_get("id")?,
            email: row.try_get("email")?,
            full_name: row.try_get("full_name")?,
            phone_number: row.try_get("phone_number")?,
            role: role_val,
            is_active: row.try_get("is_active")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }

    let total_pages = (total as f64 / limit as f64).ceil() as u64;

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched user directory successfully".to_string(),
        data: Some(AdminUserListResponse {
            items,
            total: total as u64,
            page,
            limit,
            total_pages,
        }),
    }))
}
