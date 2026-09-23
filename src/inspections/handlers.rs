use crate::{
    errors::AppError,
    inspections::models::{
        CreateInspectionReportRequest, InspectionReportResponse, ProductVerificationSummaryResponse,
    },
    models::{inspection::InspectionReport, seller::SellerProfile, AppState, Claims, UserRole},
    products::models::Product,
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

/// POST /api/v1/products/:id/inspection
/// Field Agents or Admins create a formal physical & authenticity inspection report
pub async fn create_product_inspection_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(product_id): Path<Uuid>,
    Json(payload): Json<CreateInspectionReportRequest>,
) -> Result<Json<ApiResponse<InspectionReportResponse>>, AppError> {
    // 1. Authorization: Only Field Agents or Admins can perform inspections
    if claims.role != UserRole::FieldAgent
        && claims.role != UserRole::Admin
        && claims.role != UserRole::Rider
    {
        return Err(AppError::Forbidden(
            "Only authorized field agents or admins can record product inspection reports"
                .to_string(),
        ));
    }

    if payload.physical_condition.trim().is_empty() {
        return Err(AppError::ValidationError(
            "Physical condition description cannot be empty".to_string(),
        ));
    }

    // 2. Verify product exists
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

    // 3. Create inspection_report record
    let report: InspectionReport = query_as::<_, InspectionReport>(
        r#"
        INSERT INTO inspection_reports (
            product_id, order_id, inspector_id, authenticity_verified, physical_condition,
            serial_number, functional_tests, photos_json, notes
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, product_id, order_id, inspector_id, authenticity_verified, physical_condition,
                  serial_number, functional_tests, photos_json, notes, created_at, updated_at
        "#,
    )
    .bind(product.id)
    .bind(payload.order_id)
    .bind(claims.sub)
    .bind(payload.authenticity_verified)
    .bind(payload.physical_condition.trim())
    .bind(payload.serial_number.as_deref().map(str::trim))
    .bind(&payload.functional_tests)
    .bind(&payload.photos_json)
    .bind(payload.notes.as_deref().map(str::trim))
    .fetch_one(&state.db)
    .await?;

    // 4. Update product verification status & authenticity status
    let auth_status_str = if payload.authenticity_verified {
        "VERIFIED"
    } else {
        "REJECTED"
    };

    let _: Product = query_as::<_, Product>(
        r#"
        UPDATE products
        SET verification_status = 'VERIFIED',
            authenticity_status = $1,
            last_inspected_at = CURRENT_TIMESTAMP,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $2
        RETURNING id, seller_id, name, description, price, condition, stock, verification_status,
                  authenticity_status, last_inspected_at, is_african_made, african_made_category,
                  warranty_months, warranty_terms, created_at, updated_at
        "#,
    )
    .bind(auth_status_str)
    .bind(product.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Inspection report created successfully and product verified".to_string(),
        data: Some(InspectionReportResponse {
            id: report.id,
            product_id: report.product_id,
            order_id: report.order_id,
            inspector_id: report.inspector_id,
            authenticity_verified: report.authenticity_verified,
            physical_condition: report.physical_condition,
            serial_number: report.serial_number,
            functional_tests: report.functional_tests,
            photos_json: report.photos_json,
            notes: report.notes,
            created_at: report.created_at,
        }),
    }))
}

/// GET /api/v1/products/:id/inspection
/// Fetches official inspection report for a product
pub async fn get_product_inspection_handler(
    State(state): State<AppState>,
    Path(product_id): Path<Uuid>,
) -> Result<Json<ApiResponse<InspectionReportResponse>>, AppError> {
    let report: InspectionReport = query_as::<_, InspectionReport>(
        r#"
        SELECT id, product_id, order_id, inspector_id, authenticity_verified, physical_condition,
               serial_number, functional_tests, photos_json, notes, created_at, updated_at
        FROM inspection_reports
        WHERE product_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(product_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No inspection report found for this product".to_string()))?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched product inspection report".to_string(),
        data: Some(InspectionReportResponse {
            id: report.id,
            product_id: report.product_id,
            order_id: report.order_id,
            inspector_id: report.inspector_id,
            authenticity_verified: report.authenticity_verified,
            physical_condition: report.physical_condition,
            serial_number: report.serial_number,
            functional_tests: report.functional_tests,
            photos_json: report.photos_json,
            notes: report.notes,
            created_at: report.created_at,
        }),
    }))
}

/// GET /api/v1/products/:id/verification
/// Public endpoint returning product verification state & seller trust summary
pub async fn get_product_verification_summary_handler(
    State(state): State<AppState>,
    Path(product_id): Path<Uuid>,
) -> Result<Json<ApiResponse<ProductVerificationSummaryResponse>>, AppError> {
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

    let seller_profile: Option<SellerProfile> = query_as::<_, SellerProfile>(
        r#"
        SELECT id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        FROM seller_profiles
        WHERE user_id = $1
        "#
    )
    .bind(product.seller_id)
    .fetch_optional(&state.db)
    .await?;

    let report_exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM inspection_reports WHERE product_id = $1 LIMIT 1")
            .bind(product.id)
            .fetch_optional(&state.db)
            .await?;

    let (trust_lvl, s_grade) = match seller_profile {
        Some(sp) => (Some(sp.trust_level), Some(sp.seller_grade)),
        None => (Some("LV1".to_string()), Some("Grade C".to_string())),
    };

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched product verification summary".to_string(),
        data: Some(ProductVerificationSummaryResponse {
            product_id: product.id,
            product_name: product.name,
            verification_status: product.verification_status,
            authenticity_status: product.authenticity_status,
            last_inspected_at: product.last_inspected_at,
            physical_condition: product.condition,
            seller_id: product.seller_id,
            seller_trust_level: trust_lvl,
            seller_grade: s_grade,
            has_inspection_report: report_exists.is_some(),
        }),
    }))
}
