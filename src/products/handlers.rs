use crate::{
    errors::AppError,
    models::{AppState, Claims, UserRole},
    products::models::{
        ArchiveProductRequest, CreateProductRequest, Product, ProductImage, ProductImageResponse,
        ProductResponse, ReorderImagesRequest, UpdateProductRequest, UpdateStockRequest,
        UpdateVerificationRequest,
    },
    services::storage::{
        generate_product_image_key, validate_image_bytes, MAX_IMAGES_PER_PRODUCT,
        MAX_IMAGE_SIZE_BYTES,
    },
};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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

/// Helper function to fetch product images ordered by `sort_order ASC, created_at ASC`
pub async fn fetch_product_images(
    pool: &sqlx::PgPool,
    product_id: Uuid,
) -> Result<Vec<ProductImageResponse>, AppError> {
    let images: Vec<ProductImage> = query_as::<_, ProductImage>(
        r#"
        SELECT id, product_id, url, storage_key, mime_type, file_size, sort_order, created_at
        FROM product_images
        WHERE product_id = $1
        ORDER BY sort_order ASC, created_at ASC
        "#,
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;

    Ok(images.into_iter().map(|img| img.to_response()).collect())
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

    // Sellers must be VERIFIED by an administrator before listing products
    if claims.role == UserRole::Seller {
        let profile_status: Option<(String,)> =
            sqlx::query_as("SELECT verification_status FROM seller_profiles WHERE user_id = $1")
                .bind(claims.sub)
                .fetch_optional(&state.db)
                .await?;

        let is_verified = match profile_status {
            Some((status,)) => status.to_uppercase() == "VERIFIED",
            None => false,
        };

        if !is_verified {
            return Err(AppError::Forbidden(
                "Product listing is restricted until seller verification is approved by an administrator".to_string(),
            ));
        }
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
                  warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
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
        data: Some(product.to_response(vec![])),
    }))
}

/// GET /api/v1/products
/// Lists verified non-archived products for buyers by default, or all products if status parameter provided
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
               warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        FROM products
        WHERE verification_status = $1
          AND is_archived = FALSE
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

    let mut response_data = Vec::with_capacity(products.len());
    for p in products {
        let images = fetch_product_images(&state.db, p.id).await?;
        response_data.push(p.to_response(images));
    }

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Fetched products with status: {}", target_status),
        data: Some(response_data),
    }))
}

/// GET /api/v1/products/:id
/// Fetches details for a single product. Excludes archived products unless requester is owner/admin.
pub async fn get_product_handler(
    State(state): State<AppState>,
    claims_opt: Option<Extension<Claims>>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<ProductResponse>>, AppError> {
    let product: Product = query_as::<_, Product>(
        r#"
        SELECT id, seller_id, name, description, price, condition, stock, verification_status,
               authenticity_status, last_inspected_at, is_african_made, african_made_category,
               warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        FROM products
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    if product.is_archived {
        let is_authorized = match claims_opt {
            Some(Extension(claims)) => {
                claims.role == UserRole::Admin || claims.sub == product.seller_id
            }
            None => false,
        };
        if !is_authorized {
            return Err(AppError::NotFound("Product not found".to_string()));
        }
    }

    let images = fetch_product_images(&state.db, product.id).await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Product details fetched successfully".to_string(),
        data: Some(product.to_response(images)),
    }))
}

/// PATCH /api/v1/products/:id/verify
/// Demonstration endpoint to mark a product as VERIFIED or REJECTED
pub async fn verify_product_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateVerificationRequest>,
) -> Result<Json<ApiResponse<ProductResponse>>, AppError> {
    if claims.role != UserRole::Admin && claims.role != UserRole::FieldAgent {
        return Err(AppError::Forbidden(
            "Only admins or field agents can update product verification status".to_string(),
        ));
    }

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
                  warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        "#
    )
    .bind(&new_status)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    let images = fetch_product_images(&state.db, product.id).await?;

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Product verification status updated to {}", new_status),
        data: Some(product.to_response(images)),
    }))
}

/// POST /api/v1/products/:id/images
/// Uploads a new product image via multipart/form-data stream to Cloudflare R2 / S3
pub async fn upload_product_image_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(product_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ApiResponse<ProductImageResponse>>), AppError> {
    // 1. Verify product exists
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

    // 2. Authorization check: Admin or product seller owner
    if claims.role != UserRole::Admin && claims.sub != product.seller_id {
        return Err(AppError::Forbidden(
            "You do not have permission to manage images for this product".to_string(),
        ));
    }

    // 3. Verify maximum image limit (max 5 images per product)
    let count_row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM product_images WHERE product_id = $1")
            .bind(product_id)
            .fetch_one(&state.db)
            .await?;

    if count_row.0 >= MAX_IMAGES_PER_PRODUCT as i64 {
        return Err(AppError::ValidationError(format!(
            "Maximum limit of {} images per product reached",
            MAX_IMAGES_PER_PRODUCT
        )));
    }

    // 4. Safely parse multipart field named 'file' or 'image'
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::ValidationError(format!("Failed to parse multipart payload: {}", e))
    })? {
        let name = field.name().unwrap_or("");
        if name == "file" || name == "image" {
            let data = field.bytes().await.map_err(|e| {
                AppError::ValidationError(format!("Failed to read file stream: {}", e))
            })?;
            file_bytes = Some(data.to_vec());
            break;
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        AppError::ValidationError("Missing 'file' field in multipart payload".to_string())
    })?;

    // 5. Validate file size (max 5 MB)
    if bytes.len() > MAX_IMAGE_SIZE_BYTES {
        return Err(AppError::ValidationError(format!(
            "Image file size ({} bytes) exceeds maximum limit of 5 MB",
            bytes.len()
        )));
    }

    // 6. Inspect magic bytes header to determine real MIME type (JPEG, PNG, WEBP)
    let mime_type =
        validate_image_bytes(&bytes).map_err(|e| AppError::ValidationError(e.to_string()))?;

    let extension = match mime_type.as_str() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg",
    };

    // 7. Generate secure UUID storage key: products/{seller_id}/{product_id}/{uuid}.{ext}
    let storage_key = generate_product_image_key(product.seller_id, product.id, extension);

    // 8. Upload object to R2 / S3 storage
    let public_url = state
        .storage
        .upload_object(&storage_key, &bytes, &mime_type)
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!(
                "Failed to upload image to object storage: {}",
                e
            ))
        })?;

    // 9. Insert image record into product_images table
    let sort_order = count_row.0 as i32;
    let image_id = Uuid::new_v4();

    let insert_result: Result<ProductImage, sqlx::Error> = query_as::<_, ProductImage>(
        r#"
        INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, product_id, url, storage_key, mime_type, file_size, sort_order, created_at
        "#,
    )
    .bind(image_id)
    .bind(product.id)
    .bind(&public_url)
    .bind(&storage_key)
    .bind(&mime_type)
    .bind(bytes.len() as i32)
    .bind(sort_order)
    .fetch_one(&state.db)
    .await;

    // 10. FAILURE CLEANUP: If DB insert fails, cleanup newly uploaded R2 object
    let db_image = match insert_result {
        Ok(img) => img,
        Err(db_err) => {
            tracing::error!(
                "Database INSERT failed for product image. Executing R2 object cleanup for key '{}'...",
                storage_key
            );
            if let Err(cleanup_err) = state.storage.delete_object(&storage_key).await {
                tracing::error!(
                    "R2 cleanup failed for key '{}': {}",
                    storage_key,
                    cleanup_err
                );
            }
            return Err(AppError::from(db_err));
        }
    };

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse {
            success: true,
            message: "Product image uploaded successfully".to_string(),
            data: Some(db_image.to_response()),
        }),
    ))
}

/// DELETE /api/v1/products/:id/images/:image_id
/// Deletes a product image from R2 / S3 storage and PostgreSQL database
pub async fn delete_product_image_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((product_id, image_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    // 1. Verify product exists
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

    // 2. Verify authorization: Admin or seller owner
    if claims.role != UserRole::Admin && claims.sub != product.seller_id {
        return Err(AppError::Forbidden(
            "You do not have permission to manage images for this product".to_string(),
        ));
    }

    // 3. Verify image exists and belongs to this product
    let image: ProductImage = query_as::<_, ProductImage>(
        r#"
        SELECT id, product_id, url, storage_key, mime_type, file_size, sort_order, created_at
        FROM product_images
        WHERE id = $1 AND product_id = $2
        "#,
    )
    .bind(image_id)
    .bind(product_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Image not found for this product".to_string()))?;

    // 4. Delete object from R2 storage first
    state
        .storage
        .delete_object(&image.storage_key)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to delete object from storage key '{}': {}",
                image.storage_key,
                e
            );
            AppError::InternalServerError(format!(
                "Failed to delete image from object storage: {}",
                e
            ))
        })?;

    // 5. Delete image record from database
    sqlx::query("DELETE FROM product_images WHERE id = $1")
        .bind(image_id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Product image deleted successfully".to_string(),
        data: Some(json!({
            "deleted_image_id": image_id,
            "product_id": product_id
        })),
    }))
}

/// PUT /api/v1/products/:id/images/reorder
/// Reorders product display sequence atomically in a database transaction
pub async fn reorder_product_images_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(product_id): Path<Uuid>,
    Json(payload): Json<ReorderImagesRequest>,
) -> Result<Json<ApiResponse<Vec<ProductImageResponse>>>, AppError> {
    // 1. Verify product exists
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

    // 2. Authorization check: Admin or product seller owner
    if claims.role != UserRole::Admin && claims.sub != product.seller_id {
        return Err(AppError::Forbidden(
            "You do not have permission to manage images for this product".to_string(),
        ));
    }

    // 3. Fetch existing images
    let existing_images: Vec<ProductImage> = query_as::<_, ProductImage>(
        r#"
        SELECT id, product_id, url, storage_key, mime_type, file_size, sort_order, created_at
        FROM product_images
        WHERE product_id = $1
        "#,
    )
    .bind(product_id)
    .fetch_all(&state.db)
    .await?;

    // Validation: Check duplicate IDs in payload
    let mut unique_ids = std::collections::HashSet::new();
    for img_id in &payload.image_ids {
        if !unique_ids.insert(*img_id) {
            return Err(AppError::ValidationError(
                "Duplicate image IDs found in reorder payload".to_string(),
            ));
        }
    }

    // Validation: Count must match exactly
    if payload.image_ids.len() != existing_images.len() {
        return Err(AppError::ValidationError(format!(
            "Reorder payload must contain exactly all {} existing images for this product",
            existing_images.len()
        )));
    }

    // Validation: Verify every ID in payload belongs to this product
    let existing_map: std::collections::HashMap<Uuid, &ProductImage> =
        existing_images.iter().map(|img| (img.id, img)).collect();

    for img_id in &payload.image_ids {
        if !existing_map.contains_key(img_id) {
            return Err(AppError::ValidationError(format!(
                "Image ID '{}' does not belong to product '{}'",
                img_id, product_id
            )));
        }
    }

    // 4. Update sort_order atomically in a SQL transaction
    let mut tx = state.db.begin().await?;

    for (seq, img_id) in payload.image_ids.iter().enumerate() {
        sqlx::query("UPDATE product_images SET sort_order = $1 WHERE id = $2 AND product_id = $3")
            .bind(seq as i32)
            .bind(img_id)
            .bind(product_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    // 5. Fetch updated images ordered by sort_order
    let updated_images = fetch_product_images(&state.db, product_id).await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Product image sort order updated successfully".to_string(),
        data: Some(updated_images),
    }))
}

/// PATCH /api/v1/products/:id
/// Allows product seller owner or Admin to partially update editable product fields
pub async fn update_product_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateProductRequest>,
) -> Result<Json<ApiResponse<ProductResponse>>, AppError> {
    let product: Product = query_as::<_, Product>(
        r#"
        SELECT id, seller_id, name, description, price, condition, stock, verification_status,
               authenticity_status, last_inspected_at, is_african_made, african_made_category,
               warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        FROM products
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    if claims.role != UserRole::Admin && claims.sub != product.seller_id {
        return Err(AppError::Forbidden(
            "You do not have permission to update this product".to_string(),
        ));
    }

    let name = match payload.name {
        Some(n) => {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                return Err(AppError::ValidationError(
                    "Product name cannot be empty".to_string(),
                ));
            }
            trimmed.to_string()
        }
        None => product.name,
    };

    let description = match payload.description {
        Some(d) => {
            let trimmed = d.trim();
            if trimmed.is_empty() {
                return Err(AppError::ValidationError(
                    "Product description cannot be empty".to_string(),
                ));
            }
            trimmed.to_string()
        }
        None => product.description,
    };

    let price = match payload.price {
        Some(p) => {
            if p <= 0.0 || p.is_nan() || p.is_infinite() {
                return Err(AppError::ValidationError(
                    "Price must be a valid positive number".to_string(),
                ));
            }
            p
        }
        None => product.price,
    };

    let condition = match payload.condition {
        Some(c) => {
            let normalized =
                match c.trim().to_uppercase().as_str() {
                    "NEW" => "New".to_string(),
                    "GRADE A" => "Grade A".to_string(),
                    "GRADE B" => "Grade B".to_string(),
                    "REFURBISHED" => "Refurbished".to_string(),
                    _ => return Err(AppError::ValidationError(
                        "Invalid condition. Must be 'New', 'Grade A', 'Grade B', or 'Refurbished'"
                            .to_string(),
                    )),
                };
            normalized
        }
        None => product.condition,
    };

    let warranty_months = match payload.warranty_months {
        Some(w) => {
            if w < 0 {
                return Err(AppError::ValidationError(
                    "Warranty months must be non-negative".to_string(),
                ));
            }
            w
        }
        None => product.warranty_months,
    };

    let warranty_terms = payload.warranty_terms.or(product.warranty_terms);

    let is_african_made = payload.is_african_made.unwrap_or(product.is_african_made);

    let african_made_category = if is_african_made {
        let cat_opt = payload
            .african_made_category
            .or(product.african_made_category);
        let cat = cat_opt
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

    let updated_product: Product = query_as::<_, Product>(
        r#"
        UPDATE products
        SET name = $1,
            description = $2,
            price = $3,
            condition = $4,
            is_african_made = $5,
            african_made_category = $6,
            warranty_months = $7,
            warranty_terms = $8,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $9
        RETURNING id, seller_id, name, description, price, condition, stock, verification_status,
                  authenticity_status, last_inspected_at, is_african_made, african_made_category,
                  warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        "#,
    )
    .bind(&name)
    .bind(&description)
    .bind(price)
    .bind(&condition)
    .bind(is_african_made)
    .bind(african_made_category)
    .bind(warranty_months)
    .bind(warranty_terms)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let images = fetch_product_images(&state.db, updated_product.id).await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Product details updated successfully".to_string(),
        data: Some(updated_product.to_response(images)),
    }))
}

/// PATCH /api/v1/products/:id/stock
/// Updates inventory stock count for seller owner or Admin
pub async fn update_stock_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateStockRequest>,
) -> Result<Json<ApiResponse<ProductResponse>>, AppError> {
    if payload.stock < 0 {
        return Err(AppError::ValidationError(
            "Stock quantity must be a non-negative integer".to_string(),
        ));
    }

    let product: Product = query_as::<_, Product>(
        r#"
        SELECT id, seller_id, name, description, price, condition, stock, verification_status,
               authenticity_status, last_inspected_at, is_african_made, african_made_category,
               warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        FROM products
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    if claims.role != UserRole::Admin && claims.sub != product.seller_id {
        return Err(AppError::Forbidden(
            "You do not have permission to update stock for this product".to_string(),
        ));
    }

    let updated_product: Product = query_as::<_, Product>(
        r#"
        UPDATE products
        SET stock = $1,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $2
        RETURNING id, seller_id, name, description, price, condition, stock, verification_status,
                  authenticity_status, last_inspected_at, is_african_made, african_made_category,
                  warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        "#,
    )
    .bind(payload.stock)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let images = fetch_product_images(&state.db, updated_product.id).await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Product stock updated successfully".to_string(),
        data: Some(updated_product.to_response(images)),
    }))
}

/// DELETE /api/v1/products/:id
/// Soft-deletes / archives product (preserves historical database records and R2 images)
pub async fn archive_product_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<ProductResponse>>, AppError> {
    let product: Product = query_as::<_, Product>(
        r#"
        SELECT id, seller_id, name, description, price, condition, stock, verification_status,
               authenticity_status, last_inspected_at, is_african_made, african_made_category,
               warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        FROM products
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    if claims.role != UserRole::Admin && claims.sub != product.seller_id {
        return Err(AppError::Forbidden(
            "You do not have permission to archive this product".to_string(),
        ));
    }

    let archived_product: Product = query_as::<_, Product>(
        r#"
        UPDATE products
        SET is_archived = TRUE,
            archived_at = CURRENT_TIMESTAMP,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING id, seller_id, name, description, price, condition, stock, verification_status,
                  authenticity_status, last_inspected_at, is_african_made, african_made_category,
                  warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        "#,
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let images = fetch_product_images(&state.db, archived_product.id).await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Product archived successfully".to_string(),
        data: Some(archived_product.to_response(images)),
    }))
}

/// PATCH /api/v1/products/:id/archive
/// Restores / unarchives a product or updates archive state explicitly
pub async fn restore_product_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ArchiveProductRequest>,
) -> Result<Json<ApiResponse<ProductResponse>>, AppError> {
    let product: Product = query_as::<_, Product>(
        r#"
        SELECT id, seller_id, name, description, price, condition, stock, verification_status,
               authenticity_status, last_inspected_at, is_african_made, african_made_category,
               warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        FROM products
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    if claims.role != UserRole::Admin && claims.sub != product.seller_id {
        return Err(AppError::Forbidden(
            "You do not have permission to update archive status for this product".to_string(),
        ));
    }

    let updated_product: Product = query_as::<_, Product>(
        r#"
        UPDATE products
        SET is_archived = $1,
            archived_at = CASE WHEN $1 = TRUE THEN CURRENT_TIMESTAMP ELSE NULL END,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $2
        RETURNING id, seller_id, name, description, price, condition, stock, verification_status,
                  authenticity_status, last_inspected_at, is_african_made, african_made_category,
                  warranty_months, warranty_terms, is_archived, archived_at, created_at, updated_at
        "#,
    )
    .bind(payload.archived)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let images = fetch_product_images(&state.db, updated_product.id).await?;

    let action_msg = if payload.archived {
        "Product archived successfully"
    } else {
        "Product restored successfully"
    };

    Ok(Json(ApiResponse {
        success: true,
        message: action_msg.to_string(),
        data: Some(updated_product.to_response(images)),
    }))
}
