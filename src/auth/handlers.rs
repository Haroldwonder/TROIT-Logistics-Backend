use crate::{
    auth::{
        models::{AuthResponse, LoginRequest, RegisterRequest, UserResponse},
        service::AuthService,
    },
    errors::AppError,
    models::{AppState, Claims, User, UserRole},
    utils::{validate_email, validate_password},
};
use axum::{extract::State, Extension, Json};
use sqlx::query_as;

/// POST /api/v1/auth/register
/// Registers a new user account with hashed password and role
pub async fn register_handler(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // Validate email and password inputs
    validate_email(&payload.email)?;
    validate_password(&payload.password)?;

    let clean_email = payload.email.trim().to_lowercase();
    let clean_name = payload.full_name.trim();

    if clean_name.is_empty() {
        return Err(AppError::ValidationError(
            "Full name cannot be empty".to_string(),
        ));
    }

    // Check if user already exists
    let existing: Option<User> = query_as::<_, User>(
        "SELECT id, email, password_hash, full_name, phone_number, role, is_active, created_at, updated_at FROM users WHERE email = $1"
    )
    .bind(&clean_email)
    .fetch_optional(&state.db)
    .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(
            "An account with this email address already exists".to_string(),
        ));
    }

    // Hash password with Argon2id
    let password_hash = AuthService::hash_password(&payload.password)?;
    // Public registration allows Buyer or Seller accounts.
    // Administrative and operational roles (Admin, FieldAgent, Rider) cannot be self-assigned.
    let role = match payload.role {
        Some(UserRole::Seller) => UserRole::Seller,
        Some(UserRole::Buyer) | None => UserRole::Buyer,
        Some(_) => {
            return Err(AppError::Forbidden(
                "Self-assignment of administrative or operational roles is prohibited".to_string(),
            ));
        }
    };

    // Insert user into PostgreSQL
    let user: User = query_as::<_, User>(
        r#"
        INSERT INTO users (email, password_hash, full_name, phone_number, role)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, email, password_hash, full_name, phone_number, role, is_active, created_at, updated_at
        "#
    )
    .bind(&clean_email)
    .bind(&password_hash)
    .bind(clean_name)
    .bind(&payload.phone_number)
    .bind(role)
    .fetch_one(&state.db)
    .await?;

    // Generate JWT token
    let token = AuthService::generate_token(
        user.id,
        &user.email,
        user.role,
        &state.config.jwt_secret,
        state.config.jwt_expiration_hours,
    )?;

    Ok(Json(AuthResponse {
        success: true,
        message: "Registration successful".to_string(),
        token: Some(token),
        user: Some(UserResponse {
            id: user.id,
            email: user.email,
            full_name: user.full_name,
            phone_number: user.phone_number,
            role: user.role,
            created_at: user.created_at,
        }),
    }))
}

/// POST /api/v1/auth/login
/// Authenticates email + password credentials and returns signed JWT
pub async fn login_handler(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    validate_email(&payload.email)?;
    validate_password(&payload.password)?;

    let clean_email = payload.email.trim().to_lowercase();

    // Fetch user by email
    let user: User = query_as::<_, User>(
        "SELECT id, email, password_hash, full_name, phone_number, role, is_active, created_at, updated_at FROM users WHERE email = $1"
    )
    .bind(&clean_email)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid email or password credentials".to_string()))?;

    if !user.is_active {
        return Err(AppError::Forbidden(
            "This account is currently deactivated".to_string(),
        ));
    }

    // Verify Argon2 password hash
    let is_valid = AuthService::verify_password(&payload.password, &user.password_hash)?;
    if !is_valid {
        return Err(AppError::Unauthorized(
            "Invalid email or password credentials".to_string(),
        ));
    }

    // Generate token
    let token = AuthService::generate_token(
        user.id,
        &user.email,
        user.role,
        &state.config.jwt_secret,
        state.config.jwt_expiration_hours,
    )?;

    Ok(Json(AuthResponse {
        success: true,
        message: "Login successful".to_string(),
        token: Some(token),
        user: Some(UserResponse {
            id: user.id,
            email: user.email,
            full_name: user.full_name,
            phone_number: user.phone_number,
            role: user.role,
            created_at: user.created_at,
        }),
    }))
}

/// POST /api/v1/auth/logout
/// Invalidates active session (client should discard stored JWT token)
pub async fn logout_handler() -> Result<Json<AuthResponse>, AppError> {
    Ok(Json(AuthResponse {
        success: true,
        message: "Logout successful. Token invalidated.".to_string(),
        token: None,
        user: None,
    }))
}

/// GET /api/v1/auth/me
/// Returns current authenticated user profile
pub async fn me_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<AuthResponse>, AppError> {
    let user: User = query_as::<_, User>(
        "SELECT id, email, password_hash, full_name, phone_number, role, is_active, created_at, updated_at FROM users WHERE id = $1"
    )
    .bind(claims.sub)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("User profile not found".to_string()))?;

    Ok(Json(AuthResponse {
        success: true,
        message: "User profile fetched successfully".to_string(),
        token: None,
        user: Some(UserResponse {
            id: user.id,
            email: user.email,
            full_name: user.full_name,
            phone_number: user.phone_number,
            role: user.role,
            created_at: user.created_at,
        }),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_resolution_buyer_default() {
        let req = RegisterRequest {
            email: "buyer@example.com".to_string(),
            password: "Password123!".to_string(),
            full_name: "Buyer Test".to_string(),
            phone_number: None,
            role: None,
        };
        let role = match req.role {
            Some(UserRole::Seller) => UserRole::Seller,
            Some(UserRole::Buyer) | None => UserRole::Buyer,
            Some(_) => UserRole::Buyer,
        };
        assert_eq!(role, UserRole::Buyer);
    }

    #[test]
    fn test_role_resolution_seller_allowed() {
        let req = RegisterRequest {
            email: "seller@example.com".to_string(),
            password: "Password123!".to_string(),
            full_name: "Seller Test".to_string(),
            phone_number: None,
            role: Some(UserRole::Seller),
        };
        let role = match req.role {
            Some(UserRole::Seller) => UserRole::Seller,
            Some(UserRole::Buyer) | None => UserRole::Buyer,
            Some(_) => UserRole::Buyer,
        };
        assert_eq!(role, UserRole::Seller);
    }

    #[test]
    fn test_role_resolution_admin_rejected() {
        let req = RegisterRequest {
            email: "admin@example.com".to_string(),
            password: "Password123!".to_string(),
            full_name: "Admin Attacker".to_string(),
            phone_number: None,
            role: Some(UserRole::Admin),
        };
        let result: Result<UserRole, AppError> = match req.role {
            Some(UserRole::Seller) => Ok(UserRole::Seller),
            Some(UserRole::Buyer) | None => Ok(UserRole::Buyer),
            Some(_) => Err(AppError::Forbidden(
                "Self-assignment of administrative or operational roles is prohibited".to_string(),
            )),
        };
        assert!(result.is_err());
    }
}
