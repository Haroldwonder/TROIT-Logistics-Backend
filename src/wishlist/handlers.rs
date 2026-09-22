use crate::{
    errors::AppError,
    models::{wishlist::Wishlist, AppState, Claims, UserRole},
    products::{handlers::fetch_product_images, models::Product},
    wishlist::models::WishlistItemResponse,
};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::Serialize;
use sqlx::{query_as, FromRow};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

#[derive(Debug, FromRow)]
pub struct WishlistItemRecord {
    pub id: Uuid,
    pub buyer_id: Uuid,
    pub product_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub prod_id: Option<Uuid>,
    pub prod_seller_id: Option<Uuid>,
    pub prod_name: Option<String>,
    pub prod_description: Option<String>,
    pub prod_price: Option<f64>,
    pub prod_condition: Option<String>,
    pub prod_stock: Option<i32>,
    pub prod_verification_status: Option<String>,
    pub prod_authenticity_status: Option<String>,
    pub prod_last_inspected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub prod_is_african_made: Option<bool>,
    pub prod_african_made_category: Option<String>,
    pub prod_warranty_months: Option<i32>,
    pub prod_warranty_terms: Option<String>,
    pub prod_created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub prod_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// POST /api/v1/wishlist/:product_id
/// Adds a product to caller's wishlist
pub async fn add_to_wishlist_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(product_id): Path<Uuid>,
) -> Result<Json<ApiResponse<WishlistItemResponse>>, AppError> {
    if claims.role != UserRole::Buyer && claims.role != UserRole::Admin {
        return Err(AppError::Forbidden(
            "Only buyers can save products to their wishlist".to_string(),
        ));
    }

    // Ensure product exists
    let product: Product = query_as::<_, Product>(
        r#"
        SELECT id, seller_id, name, description, price, condition, stock, verification_status,
               authenticity_status, last_inspected_at, is_african_made, african_made_category,
               warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        FROM products
        WHERE id = $1
        "#,
    )
    .bind(product_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    let images = fetch_product_images(&state.db, product.id).await?;

    let item: Wishlist = query_as::<_, Wishlist>(
        r#"
        INSERT INTO wishlists (buyer_id, product_id)
        VALUES ($1, $2)
        ON CONFLICT (buyer_id, product_id) DO UPDATE SET created_at = wishlists.created_at
        RETURNING id, buyer_id, product_id, created_at
        "#,
    )
    .bind(claims.sub)
    .bind(product.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Product added to wishlist".to_string(),
        data: Some(WishlistItemResponse {
            id: item.id,
            buyer_id: item.buyer_id,
            product_id: item.product_id,
            product: Some(product.to_response(images)),
            created_at: item.created_at,
        }),
    }))
}

/// DELETE /api/v1/wishlist/:product_id
/// Removes a product from caller's wishlist
pub async fn remove_from_wishlist_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(product_id): Path<Uuid>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let result =
        sqlx::query("DELETE FROM wishlists WHERE buyer_id = $1 AND (product_id = $2 OR id = $2)")
            .bind(claims.sub)
            .bind(product_id)
            .execute(&state.db)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Wishlist item not found".to_string()));
    }

    Ok(Json(ApiResponse {
        success: true,
        message: "Product removed from wishlist".to_string(),
        data: None,
    }))
}

/// GET /api/v1/wishlist
/// Retrieves caller's saved wishlist products
pub async fn list_wishlist_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ApiResponse<Vec<WishlistItemResponse>>>, AppError> {
    let records: Vec<WishlistItemRecord> = sqlx::query_as::<_, WishlistItemRecord>(
        r#"
        SELECT w.id, w.buyer_id, w.product_id, w.created_at,
               p.id AS prod_id, p.seller_id AS prod_seller_id, p.name AS prod_name,
               p.description AS prod_description, p.price AS prod_price, p.condition AS prod_condition,
               p.stock AS prod_stock, p.verification_status AS prod_verification_status,
               p.authenticity_status AS prod_authenticity_status, p.last_inspected_at AS prod_last_inspected_at,
               p.is_african_made AS prod_is_african_made, p.african_made_category AS prod_african_made_category,
               p.warranty_months AS prod_warranty_months, p.warranty_terms AS prod_warranty_terms,
               p.created_at AS prod_created_at, p.updated_at AS prod_updated_at
        FROM wishlists w
        LEFT JOIN products p ON w.product_id = p.id
        WHERE w.buyer_id = $1
        ORDER BY w.created_at DESC
        "#
    )
    .bind(claims.sub)
    .fetch_all(&state.db)
    .await?;

    let mut response: Vec<WishlistItemResponse> = Vec::with_capacity(records.len());
    for r in records {
        let product_opt = if let Some(pid) = r.prod_id {
            let p = Product {
                id: pid,
                seller_id: r.prod_seller_id.unwrap(),
                name: r.prod_name.unwrap(),
                description: r.prod_description.unwrap(),
                price: r.prod_price.unwrap(),
                condition: r.prod_condition.unwrap(),
                stock: r.prod_stock.unwrap(),
                verification_status: r.prod_verification_status.unwrap(),
                authenticity_status: r
                    .prod_authenticity_status
                    .unwrap_or_else(|| "UNVERIFIED".to_string()),
                last_inspected_at: r.prod_last_inspected_at,
                is_african_made: r.prod_is_african_made.unwrap_or(false),
                african_made_category: r.prod_african_made_category,
                warranty_months: r.prod_warranty_months.unwrap_or(0),
                warranty_terms: r.prod_warranty_terms,
                is_archived: false,
                archived_at: None,
                created_at: r.prod_created_at.unwrap(),
                updated_at: r.prod_updated_at.unwrap(),
            };
            let images = fetch_product_images(&state.db, p.id).await?;
            Some(p.to_response(images))
        } else {
            None
        };

        response.push(WishlistItemResponse {
            id: r.id,
            buyer_id: r.buyer_id,
            product_id: r.product_id,
            product: product_opt,
            created_at: r.created_at,
        });
    }

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched user wishlist".to_string(),
        data: Some(response),
    }))
}
