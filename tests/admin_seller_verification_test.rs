use std::sync::Arc;
use troit_logistics_backend::{
    admin::models::{AdminSellerQuery, AdminUpdateSellerVerificationRequest},
    auth::service::AuthService,
    config::AppConfig,
    models::{seller::SellerProfile, AppState, Claims, UserRole},
    products::models::CreateProductRequest,
    services::storage::TestStorageService,
};
use uuid::Uuid;

const JWT_SECRET: &str = "test_jwt_secret_for_admin_verification_tests_123456789";

fn mock_claims(sub: Uuid, role: UserRole) -> Claims {
    Claims {
        sub,
        email: format!("user_{}@troit.test", sub),
        role,
        exp: 9999999999,
        iat: 1000000000,
    }
}

#[tokio::test]
async fn test_admin_verification_authorization_matrix() {
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

    let config = AppConfig {
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
        admin_bootstrap_secret: None,
    };

    let blockchain = Arc::new(
        troit_logistics_backend::blockchain::BlockchainService::new(&config)
            .expect("Blockchain init failed"),
    );
    let storage = Arc::new(TestStorageService::new());

    let state = AppState {
        db: pool.clone(),
        config,
        blockchain,
        storage,
    };

    // 1. Create test seller user & seller profile
    let seller_user_id = Uuid::new_v4();
    let seller_email = format!("seller_{}@test.com", seller_user_id);
    let pass_hash = AuthService::hash_password("Password123!").unwrap();

    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, role) VALUES ($1, $2, $3, 'Test Seller', 'seller')"
    )
    .bind(seller_user_id)
    .bind(&seller_email)
    .bind(&pass_hash)
    .execute(&pool)
    .await
    .unwrap();

    let seller_profile: SellerProfile = sqlx::query_as(
        r#"
        INSERT INTO seller_profiles (user_id, store_name, store_address, verification_status, trust_level, seller_grade)
        VALUES ($1, 'Test Store', 'Port Harcourt', 'UNDER_REVIEW', 'LV1', 'Grade C')
        RETURNING id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        "#
    )
    .bind(seller_user_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // 2. Non-admin roles (Buyer, Seller, Rider, FieldAgent) must be rejected with 403
    let non_admin_roles = vec![
        UserRole::Buyer,
        UserRole::Seller,
        UserRole::Rider,
        UserRole::FieldAgent,
    ];

    let update_req = AdminUpdateSellerVerificationRequest {
        status: "VERIFIED".to_string(),
    };

    for role in non_admin_roles {
        let claims = mock_claims(Uuid::new_v4(), role);
        let res =
            troit_logistics_backend::admin::handlers::update_admin_seller_verification_handler(
                axum::extract::State(state.clone()),
                axum::extract::Extension(claims),
                axum::extract::Path(seller_profile.id),
                axum::extract::Json(AdminUpdateSellerVerificationRequest {
                    status: "VERIFIED".to_string(),
                }),
            )
            .await;

        assert!(res.is_err());
        match res.unwrap_err() {
            troit_logistics_backend::errors::AppError::Forbidden(_) => {}
            other => panic!(
                "Expected Forbidden error for role {:?}, got {:?}",
                role, other
            ),
        }
    }

    // 3. Unverified seller (UNDER_REVIEW) cannot create products
    let seller_claims = mock_claims(seller_user_id, UserRole::Seller);
    let create_prod_req = CreateProductRequest {
        name: "Test Laptop".to_string(),
        description: "High performance laptop".to_string(),
        price: 150000.0,
        condition: Some("Grade A".to_string()),
        stock: Some(5),
        is_african_made: Some(false),
        african_made_category: None,
        warranty_months: Some(12),
        warranty_terms: Some("Standard warranty".to_string()),
    };

    let unverified_prod_res = troit_logistics_backend::products::handlers::create_product_handler(
        axum::extract::State(state.clone()),
        axum::extract::Extension(seller_claims.clone()),
        axum::extract::Json(CreateProductRequest {
            name: "Test Laptop".to_string(),
            description: "High performance laptop".to_string(),
            price: 150000.0,
            condition: Some("Grade A".to_string()),
            stock: Some(5),
            is_african_made: Some(false),
            african_made_category: None,
            warranty_months: Some(12),
            warranty_terms: Some("Standard warranty".to_string()),
        }),
    )
    .await;

    assert!(unverified_prod_res.is_err());
    match unverified_prod_res.unwrap_err() {
        troit_logistics_backend::errors::AppError::Forbidden(msg) => {
            assert!(msg.contains("restricted until seller verification is approved"));
        }
        other => panic!("Expected Forbidden for unverified seller, got {:?}", other),
    }

    // 4. Admin lists seller directory (200)
    let admin_id = Uuid::new_v4();
    let admin_claims = mock_claims(admin_id, UserRole::Admin);
    let list_res = troit_logistics_backend::admin::handlers::list_admin_sellers_handler(
        axum::extract::State(state.clone()),
        axum::extract::Extension(admin_claims.clone()),
        axum::extract::Query(AdminSellerQuery {
            page: Some(1),
            limit: Some(10),
            search: Some("Test Store".to_string()),
            verification_status: Some("UNDER_REVIEW".to_string()),
        }),
    )
    .await;

    assert!(list_res.is_ok());
    let list_data = list_res.unwrap().0.data.unwrap();
    assert_eq!(list_data.total, 1);
    assert_eq!(list_data.items[0].seller_id, seller_profile.id);

    // 5. Admin approves seller verification (200) -> status becomes VERIFIED
    let approve_res =
        troit_logistics_backend::admin::handlers::update_admin_seller_verification_handler(
            axum::extract::State(state.clone()),
            axum::extract::Extension(admin_claims.clone()),
            axum::extract::Path(seller_profile.id),
            axum::extract::Json(update_req),
        )
        .await;

    assert!(approve_res.is_ok());
    let updated_seller = approve_res.unwrap().0.data.unwrap();
    assert_eq!(updated_seller.verification_status, "VERIFIED");

    // 6. Verified seller CAN create product (200)
    let verified_prod_res = troit_logistics_backend::products::handlers::create_product_handler(
        axum::extract::State(state.clone()),
        axum::extract::Extension(seller_claims),
        axum::extract::Json(create_prod_req),
    )
    .await;

    assert!(verified_prod_res.is_ok());
    let created_prod = verified_prod_res.unwrap().0.data.unwrap();
    assert_eq!(created_prod.name, "Test Laptop");

    // 7. Admin rejects seller verification -> status becomes REJECTED
    let reject_res =
        troit_logistics_backend::admin::handlers::update_admin_seller_verification_handler(
            axum::extract::State(state.clone()),
            axum::extract::Extension(admin_claims),
            axum::extract::Path(seller_profile.id),
            axum::extract::Json(AdminUpdateSellerVerificationRequest {
                status: "REJECTED".to_string(),
            }),
        )
        .await;

    assert!(reject_res.is_ok());
    assert_eq!(
        reject_res.unwrap().0.data.unwrap().verification_status,
        "REJECTED"
    );
}
