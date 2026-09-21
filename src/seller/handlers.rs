use crate::{
    errors::AppError,
    models::{seller::SellerProfile, AppState, Claims, UserRole},
    seller::models::{
        SellerPublicProfileResponse, SubmitVerificationRequest, UpdateSellerProfileRequest,
        VerificationStatusResponse,
    },
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

/// GET /api/v1/seller/profile
/// Fetches caller's seller profile (auto-initializes if caller is seller and profile missing)
pub async fn get_current_seller_profile_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ApiResponse<SellerPublicProfileResponse>>, AppError> {
    if claims.role != UserRole::Seller && claims.role != UserRole::Admin {
        return Err(AppError::Forbidden(
            "Only sellers can access their seller profile".to_string(),
        ));
    }

    let profile: SellerProfile = query_as::<_, SellerProfile>(
        r#"
        INSERT INTO seller_profiles (user_id, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status)
        VALUES ($1, 'LV1', 'Grade C', 0, 100.0, 'PENDING')
        ON CONFLICT (user_id) DO UPDATE SET updated_at = CURRENT_TIMESTAMP
        RETURNING id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        "#
    )
    .bind(claims.sub)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched seller profile".to_string(),
        data: Some(SellerPublicProfileResponse {
            seller_id: profile.id,
            user_id: profile.user_id,
            store_name: profile.store_name,
            store_address: profile.store_address,
            trust_level: profile.trust_level,
            seller_grade: profile.seller_grade,
            successful_transactions: profile.successful_transactions,
            fulfillment_rate: profile.fulfillment_rate,
            verification_status: profile.verification_status,
            created_at: profile.created_at,
        }),
    }))
}

/// GET /api/v1/seller/profile/:seller_id
/// Public endpoint for buyers to view seller trust metrics and store details
pub async fn get_seller_profile_by_id_handler(
    State(state): State<AppState>,
    Path(seller_id): Path<Uuid>,
) -> Result<Json<ApiResponse<SellerPublicProfileResponse>>, AppError> {
    let profile: SellerProfile = query_as::<_, SellerProfile>(
        r#"
        SELECT id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        FROM seller_profiles
        WHERE id = $1 OR user_id = $1
        "#
    )
    .bind(seller_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Seller profile not found".to_string()))?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched seller profile details".to_string(),
        data: Some(SellerPublicProfileResponse {
            seller_id: profile.id,
            user_id: profile.user_id,
            store_name: profile.store_name,
            store_address: profile.store_address,
            trust_level: profile.trust_level,
            seller_grade: profile.seller_grade,
            successful_transactions: profile.successful_transactions,
            fulfillment_rate: profile.fulfillment_rate,
            verification_status: profile.verification_status,
            created_at: profile.created_at,
        }),
    }))
}

/// PATCH /api/v1/seller/profile
/// Updates seller store details (store_name, store_address ONLY).
/// Backend-controlled metrics (trust_level, grade, transactions) CANNOT be modified.
pub async fn update_seller_profile_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<UpdateSellerProfileRequest>,
) -> Result<Json<ApiResponse<SellerPublicProfileResponse>>, AppError> {
    if claims.role != UserRole::Seller && claims.role != UserRole::Admin {
        return Err(AppError::Forbidden(
            "Only sellers can update seller profile details".to_string(),
        ));
    }

    let profile: SellerProfile = query_as::<_, SellerProfile>(
        r#"
        UPDATE seller_profiles
        SET store_name = COALESCE($1, store_name),
            store_address = COALESCE($2, store_address),
            updated_at = CURRENT_TIMESTAMP
        WHERE user_id = $3
        RETURNING id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        "#
    )
    .bind(payload.store_name)
    .bind(payload.store_address)
    .bind(claims.sub)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Seller profile not found".to_string()))?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Seller profile updated successfully".to_string(),
        data: Some(SellerPublicProfileResponse {
            seller_id: profile.id,
            user_id: profile.user_id,
            store_name: profile.store_name,
            store_address: profile.store_address,
            trust_level: profile.trust_level,
            seller_grade: profile.seller_grade,
            successful_transactions: profile.successful_transactions,
            fulfillment_rate: profile.fulfillment_rate,
            verification_status: profile.verification_status,
            created_at: profile.created_at,
        }),
    }))
}

/// GET /api/v1/seller/verification
/// Fetches current verification state for the authenticated seller
pub async fn get_seller_verification_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ApiResponse<VerificationStatusResponse>>, AppError> {
    let profile: Option<SellerProfile> = query_as::<_, SellerProfile>(
        r#"
        SELECT id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        FROM seller_profiles
        WHERE user_id = $1
        "#
    )
    .bind(claims.sub)
    .fetch_optional(&state.db)
    .await?;

    let (seller_id, v_status, s_name, s_addr) = match profile {
        Some(p) => (p.id, p.verification_status, p.store_name, p.store_address),
        None => (Uuid::nil(), "NOT_STARTED".to_string(), None, None),
    };

    let is_verified = v_status == "VERIFIED";

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched seller verification state".to_string(),
        data: Some(VerificationStatusResponse {
            seller_id,
            verification_status: v_status,
            store_name: s_name,
            store_address: s_addr,
            kyc_completed: is_verified,
            store_verified: is_verified,
            physical_inspection_passed: is_verified,
        }),
    }))
}

/// POST /api/v1/seller/verification
/// Submits seller onboarding verification details
pub async fn submit_seller_verification_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<SubmitVerificationRequest>,
) -> Result<Json<ApiResponse<VerificationStatusResponse>>, AppError> {
    if payload.store_name.trim().is_empty() || payload.store_address.trim().is_empty() {
        return Err(AppError::ValidationError(
            "Store name and store address are required".to_string(),
        ));
    }

    let profile: SellerProfile = query_as::<_, SellerProfile>(
        r#"
        INSERT INTO seller_profiles (user_id, store_name, store_address, verification_status, trust_level, seller_grade)
        VALUES ($1, $2, $3, 'UNDER_REVIEW', 'LV1', 'Grade C')
        ON CONFLICT (user_id) DO UPDATE SET
            store_name = EXCLUDED.store_name,
            store_address = EXCLUDED.store_address,
            verification_status = 'UNDER_REVIEW',
            updated_at = CURRENT_TIMESTAMP
        RETURNING id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        "#
    )
    .bind(claims.sub)
    .bind(payload.store_name.trim())
    .bind(payload.store_address.trim())
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Verification details submitted successfully and under review".to_string(),
        data: Some(VerificationStatusResponse {
            seller_id: profile.id,
            verification_status: profile.verification_status,
            store_name: profile.store_name,
            store_address: profile.store_address,
            kyc_completed: true,
            store_verified: false,
            physical_inspection_passed: false,
        }),
    }))
}

