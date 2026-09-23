use std::sync::Arc;
use troit_logistics_backend::{
    auth::service::AuthService,
    config::AppConfig,
    models::{AppState, User, UserRole},
    services::storage::TestStorageService,
};
use uuid::Uuid;

const JWT_SECRET: &str = "test_jwt_secret_for_admin_bootstrap_tests_123456789";
const TEST_BOOTSTRAP_SECRET: &str = "super_secret_test_bootstrap_token_987654321";
const TARGET_ADMIN_EMAIL: &str = "admininout@troitlogistics.com";

#[tokio::test]
async fn test_admin_bootstrap_complete_suite() {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/troit_logistics".to_string()
    });

    let pool = match sqlx::PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(_) => {
            println!("Skipping DB test: Postgres connection unavailable");
            return;
        }
    };

    let _ = sqlx::migrate!("./migrations").run(&pool).await;

    let base_config = AppConfig {
        database_url: database_url.clone(),
        app_host: "0.0.0.0".to_string(),
        app_port: 8000,
        rust_log: "info".to_string(),
        jwt_secret: JWT_SECRET.to_string(),
        jwt_expiration_hours: 24,
        cors_allowed_origins: vec!["*".to_string()],
        stellar_rpc_url: "https://soroban-testnet.stellar.org".to_string(),
        stellar_network_passphrase: "Test SDF Network ; September 2015".to_string(),
        soroban_escrow_contract_id: "".to_string(),
        soroban_token_contract_id: "".to_string(),
        soroban_admin_secret_key: "".to_string(),
        soroban_service_secret_key: "".to_string(),
        soroban_buyer_secret_key: "".to_string(),
        soroban_seller_secret_key: "".to_string(),
        soroban_max_fee: 5000000,
        storage_provider: "r2".to_string(),
        s3_bucket_name: "test-bucket".to_string(),
        s3_endpoint: "https://test.r2.cloudflarestorage.com".to_string(),
        s3_region: "auto".to_string(),
        s3_access_key_id: "key".to_string(),
        s3_secret_access_key: "secret".to_string(),
        storage_public_url: "https://cdn.test.com".to_string(),
        max_image_size_mb: 5,
        admin_bootstrap_secret: Some(TEST_BOOTSTRAP_SECRET.to_string()),
    };

    let blockchain = Arc::new(
        troit_logistics_backend::blockchain::BlockchainService::new(&base_config)
            .expect("Blockchain init failed"),
    );
    let storage = Arc::new(TestStorageService::new());

    let state = AppState {
        db: pool.clone(),
        config: base_config.clone(),
        blockchain,
        storage,
    };

    // Clean up past test state for target email and bootstrap state
    sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(TARGET_ADMIN_EMAIL)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        "DELETE FROM admin_bootstrap_state WHERE bootstrap_name = 'initial_admin_bootstrap'",
    )
    .execute(&pool)
    .await
    .unwrap();

    // 1. Missing secret configuration fails safely
    let unconfigured_config = AppConfig {
        admin_bootstrap_secret: None,
        ..base_config.clone()
    };
    let unconfigured_state = AppState {
        config: unconfigured_config,
        ..state.clone()
    };

    let mut unconfigured_headers = axum::http::HeaderMap::new();
    unconfigured_headers.insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {}", TEST_BOOTSTRAP_SECRET).parse().unwrap(),
    );

    let unconfigured_res = troit_logistics_backend::bootstrap::handlers::admin_bootstrap_handler(
        axum::extract::State(unconfigured_state),
        unconfigured_headers,
    )
    .await;

    assert!(unconfigured_res.is_err());

    // 2. Wrong secret returns 401 Unauthorized
    let mut wrong_headers = axum::http::HeaderMap::new();
    wrong_headers.insert(
        axum::http::header::AUTHORIZATION,
        "Bearer INVALID_WRONG_BOOTSTRAP_SECRET_123".parse().unwrap(),
    );

    let wrong_res = troit_logistics_backend::bootstrap::handlers::admin_bootstrap_handler(
        axum::extract::State(state.clone()),
        wrong_headers,
    )
    .await;

    assert!(wrong_res.is_err());

    // 3. User does NOT exist -> Fails safely with 404 & does NOT consume bootstrap
    let mut valid_headers = axum::http::HeaderMap::new();
    valid_headers.insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {}", TEST_BOOTSTRAP_SECRET).parse().unwrap(),
    );

    let no_user_res = troit_logistics_backend::bootstrap::handlers::admin_bootstrap_handler(
        axum::extract::State(state.clone()),
        valid_headers.clone(),
    )
    .await;

    assert!(no_user_res.is_err());

    // Verify bootstrap state is NOT consumed in DB
    let consumed_check: Option<(Option<chrono::DateTime<chrono::Utc>>,)> = sqlx::query_as(
        "SELECT consumed_at FROM admin_bootstrap_state WHERE bootstrap_name = 'initial_admin_bootstrap'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(consumed_check.is_none() || consumed_check.unwrap().0.is_none());

    // 4. Create target user account as Admin first to test already-provisioned idempotency
    let user_id = Uuid::new_v4();
    let raw_password = "PasswordAdmin123!";
    let pass_hash = AuthService::hash_password(raw_password).unwrap();

    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, role) VALUES ($1, $2, $3, 'Admin In Out', 'admin')"
    )
    .bind(user_id)
    .bind(TARGET_ADMIN_EMAIL)
    .bind(&pass_hash)
    .execute(&pool)
    .await
    .unwrap();

    // 5. Existing admin account returns safe already-provisioned response prior to bootstrap consumption
    let already_admin_res = troit_logistics_backend::bootstrap::handlers::admin_bootstrap_handler(
        axum::extract::State(state.clone()),
        valid_headers.clone(),
    )
    .await;

    assert!(already_admin_res.is_ok());
    let already_body = already_admin_res.unwrap().0;
    assert!(already_body.success);
    assert!(already_body.message.contains("already provisioned"));

    // Verify bootstrap state is still NOT consumed
    let consumed_check2: Option<(Option<chrono::DateTime<chrono::Utc>>,)> = sqlx::query_as(
        "SELECT consumed_at FROM admin_bootstrap_state WHERE bootstrap_name = 'initial_admin_bootstrap'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(consumed_check2.is_none() || consumed_check2.unwrap().0.is_none());

    // Demote role to buyer for promotion testing
    sqlx::query("UPDATE users SET role = 'buyer' WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    // 6. JWT tokens for non-admin roles (Buyer, Seller, Rider, FieldAgent) cannot call bootstrap
    let non_admin_roles = vec![
        UserRole::Buyer,
        UserRole::Seller,
        UserRole::Rider,
        UserRole::FieldAgent,
    ];

    for role in non_admin_roles {
        let jwt_token =
            AuthService::generate_token(user_id, TARGET_ADMIN_EMAIL, role, JWT_SECRET, 24).unwrap();
        let mut jwt_headers = axum::http::HeaderMap::new();
        jwt_headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", jwt_token).parse().unwrap(),
        );

        let jwt_res = troit_logistics_backend::bootstrap::handlers::admin_bootstrap_handler(
            axum::extract::State(state.clone()),
            jwt_headers,
        )
        .await;

        assert!(jwt_res.is_err());
    }

    // 7. Correct bootstrap secret promotes target account to Admin
    let promote_res = troit_logistics_backend::bootstrap::handlers::admin_bootstrap_handler(
        axum::extract::State(state.clone()),
        valid_headers.clone(),
    )
    .await;

    assert!(promote_res.is_ok());
    let body = promote_res.unwrap().0;
    assert!(body.success);
    assert!(!body.message.contains(TEST_BOOTSTRAP_SECRET));

    // Verify role in PostgreSQL is now admin
    let updated_user: User = sqlx::query_as("SELECT id, email, password_hash, full_name, phone_number, role, is_active, created_at, updated_at FROM users WHERE email = $1")
        .bind(TARGET_ADMIN_EMAIL)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(updated_user.role, UserRole::Admin);

    // 8. Second invocation when bootstrap is already consumed -> Fails safely
    let second_res = troit_logistics_backend::bootstrap::handlers::admin_bootstrap_handler(
        axum::extract::State(state.clone()),
        valid_headers.clone(),
    )
    .await;

    assert!(second_res.is_err());

    // 9. Normal login still works after promotion
    let login_res = troit_logistics_backend::auth::handlers::login_handler(
        axum::extract::State(state.clone()),
        axum::extract::Json(troit_logistics_backend::auth::models::LoginRequest {
            email: TARGET_ADMIN_EMAIL.to_string(),
            password: raw_password.to_string(),
        }),
    )
    .await;

    assert!(login_res.is_ok());
    let login_data = login_res.unwrap().0;
    assert!(login_data.success);
    let token = login_data.token.unwrap();
    assert_eq!(login_data.user.unwrap().role, UserRole::Admin);

    // 10. Promoted admin user gains access to admin-protected directory endpoints
    let claims = AuthService::verify_token(&token, JWT_SECRET).unwrap();
    let admin_sellers_res = troit_logistics_backend::admin::handlers::list_admin_sellers_handler(
        axum::extract::State(state.clone()),
        axum::extract::Extension(claims),
        axum::extract::Query(troit_logistics_backend::admin::models::AdminSellerQuery {
            page: Some(1),
            limit: Some(10),
            search: None,
            verification_status: None,
        }),
    )
    .await;

    assert!(admin_sellers_res.is_ok());
}
