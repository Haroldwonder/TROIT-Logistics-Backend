use crate::{
    errors::AppError,
    models::{AppState, Claims, UserRole},
    products::models::{CreateProductRequest, Product, ProductResponse, UpdateVerificationRequest},
};
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sqlx::query_as;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

#[derive(Debug, Deserialize)]
pub struct ProductFilterQuery {
    pub status: Option<String>,
    pub is_african_made: Option<bool>,
    pub african_made_category: Option<String>,
}

/// POST /api/v1/products
/// Sellers create a new product for listing
pub async fn create_product_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<CreateProductRequest>,
) -> Result<Json<ApiResponse<ProductResponse>>, AppError> {
    // Only Sellers or Admins can create products
    if claims.role != UserRole::Seller && claims.role != UserRole::Admin {
        return Err(AppError::Forbidden(
            "Only authenticated sellers can list products".to_string(),
        ));
    }

    let clean_name = payload.name.trim();
    let clean_desc = payload.description.trim();

    if clean_name.is_empty() {
        return Err(AppError::ValidationError(
            "Product name cannot be empty".to_string(),
        ));
    }

    if payload.price <= 0.0 {
        return Err(AppError::ValidationError(
            "Product price must be greater than zero".to_string(),
        ));
    }

    let stock = payload.stock.unwrap_or(1);
    if stock < 0 {
        return Err(AppError::ValidationError(
            "Product stock cannot be negative".to_string(),
        ));
    }

    let condition = payload.condition.unwrap_or_else(|| "Grade A".to_string());
    let is_african_made = payload.is_african_made.unwrap_or(false);
    let warranty_months = payload.warranty_months.unwrap_or(0);

    let african_made_category = if is_african_made {
        let cat = payload
            .african_made_category
            .ok_or_else(|| {
                AppError::ValidationError(
                    "african_made_category is required when is_african_made is true".to_string(),
                )
            })?
            .to_uppercase();

        if cat != "ELECTRONICS" && cat != "HOME_APPLIANCES" && cat != "FURNITURE" {
            return Err(AppError::ValidationError(
                "Invalid african_made_category. Must be ELECTRONICS, HOME_APPLIANCES, or FURNITURE"
                    .to_string(),
            ));
        }
        Some(cat)
    } else {
        if payload.african_made_category.is_some() {
            return Err(AppError::ValidationError(
                "african_made_category cannot be set when is_african_made is false".to_string(),
            ));
        }
        None
    };

    // Create product in database (default verification_status: PENDING)
    let product: Product = query_as::<_, Product>(
        r#"
        INSERT INTO products (
            seller_id, name, description, price, condition, stock, verification_status,
            is_african_made, african_made_category, warranty_months, warranty_terms
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'PENDING', $7, $8, $9, $10)
        RETURNING id, seller_id, name, description, price, condition, stock, verification_status,
                  authenticity_status, last_inspected_at, is_african_made, african_made_category,
                  warranty_months, warranty_terms, created_at, updated_at
        "#,
    )
    .bind(claims.sub)
    .bind(clean_name)
    .bind(clean_desc)
    .bind(payload.price)
    .bind(condition)
    .bind(stock)
    .bind(is_african_made)
    .bind(african_made_category)
    .bind(warranty_months)
    .bind(payload.warranty_terms)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Product created successfully and submitted for verification".to_string(),
        data: Some(product.to_response()),
    }))
}

/// GET /api/v1/products
/// Lists verified products for buyers by default, or all products if status parameter provided
pub async fn list_products_handler(
    State(state): State<AppState>,
    Query(query): Query<ProductFilterQuery>,
) -> Result<Json<ApiResponse<Vec<ProductResponse>>>, AppError> {
    let target_status = query
        .status
        .map(|s| s.to_uppercase())
        .unwrap_or_else(|| "VERIFIED".to_string());

    let african_made_cat = query.african_made_category.map(|c| c.to_uppercase());

    let products: Vec<Product> = query_as::<_, Product>(
        r#"
        SELECT id, seller_id, name, description, price, condition, stock, verification_status,
               authenticity_status, last_inspected_at, is_african_made, african_made_category,
               warranty_months, warranty_terms, created_at, updated_at
        FROM products
        WHERE verification_status = $1
          AND ($2::boolean IS NULL OR is_african_made = $2)
          AND ($3::text IS NULL OR african_made_category = $3)
        ORDER BY created_at DESC
        "#,
    )
    .bind(&target_status)
    .bind(query.is_african_made)
    .bind(african_made_cat)
    .fetch_all(&state.db)
    .await?;

    let response_data: Vec<ProductResponse> =
        products.into_iter().map(|p| p.to_response()).collect();

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Fetched products with status: {}", target_status),
        data: Some(response_data),
    }))
}

/// GET /api/v1/products/:id
/// Fetches details for a single product
pub async fn get_product_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<ProductResponse>>, AppError> {
    let product: Product = query_as::<_, Product>(
        r#"
        SELECT id, seller_id, name, description, price, condition, stock, verification_status,
               authenticity_status, last_inspected_at, is_african_made, african_made_category,
               warranty_months, warranty_terms, created_at, updated_at
        FROM products
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Product details fetched successfully".to_string(),
        data: Some(product.to_response()),
    }))
}

/// PATCH /api/v1/products/:id/verify
/// Demonstration endpoint to mark a product as VERIFIED or REJECTED
pub async fn verify_product_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateVerificationRequest>,
) -> Result<Json<ApiResponse<ProductResponse>>, AppError> {
    let new_status = payload.verification_status.to_uppercase();
    if new_status != "VERIFIED" && new_status != "REJECTED" && new_status != "PENDING" {
        return Err(AppError::ValidationError(
            "Invalid status. Must be VERIFIED, REJECTED, or PENDING".to_string(),
        ));
    }

    let product: Product = query_as::<_, Product>(
        r#"
        UPDATE products
        SET verification_status = $1,
            authenticity_status = CASE WHEN $1 = 'VERIFIED' THEN 'VERIFIED' ELSE authenticity_status END,
            last_inspected_at = CASE WHEN $1 = 'VERIFIED' THEN CURRENT_TIMESTAMP ELSE last_inspected_at END,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $2
        RETURNING id, seller_id, name, description, price, condition, stock, verification_status,
                  authenticity_status, last_inspected_at, is_african_made, african_made_category,
                  warranty_months, warranty_terms, created_at, updated_at
        "#
    )
    .bind(&new_status)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Product verification status updated to {}", new_status),
        data: Some(product.to_response()),
    }))
}
