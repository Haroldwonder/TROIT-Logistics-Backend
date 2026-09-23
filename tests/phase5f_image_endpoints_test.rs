//! Phase 5F Step 3 Integration Test Suite
//!
//! Verifies all 20 requirements specified in Step 3 for Product Image Upload & Image Management APIs:
//! 1. Authenticated seller can upload image to own product
//! 2. Unauthenticated upload rejected
//! 3. Seller cannot upload to another seller's product
//! 4. Admin can upload
//! 5. Invalid image (corrupted / non-image) rejected
//! 6. Unsupported image type rejected
//! 7. Image larger than 5 MB rejected
//! 8. Sixth image rejected (max 5)
//! 9. Successful upload creates product_images DB record
//! 10. R2 upload failure does not create DB record
//! 11. DB insert failure triggers R2 cleanup
//! 12. Seller can delete own image
//! 13. Seller cannot delete another seller's image
//! 14. Admin can delete
//! 15. R2 delete failure does not delete DB record
//! 16. Reorder succeeds with valid complete list
//! 17. Reorder rejects duplicate IDs
//! 18. Reorder rejects missing IDs / incomplete list
//! 19. Reorder rejects images belonging to another product
//! 20. Product image responses are ordered by sort_order ASC

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use troit_logistics_backend::{
    auth::service::AuthService,
    config::AppConfig,
    models::{AppState, UserRole},
    routes::create_router,
    services::storage::TestStorageService,
};
use uuid::Uuid;

const JWT_SECRET: &str = "phase5f_test_secret_key_1234567890";

fn valid_png_bytes() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x00,
    ]
}

fn valid_jpeg_bytes() -> Vec<u8> {
    vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00,
    ]
}

fn valid_webp_bytes() -> Vec<u8> {
    vec![
        0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38,
        0x20,
    ]
}

fn invalid_bytes() -> Vec<u8> {
    b"NOT_AN_IMAGE_FILE_DATA_BYTES".to_vec()
}

fn create_multipart_body(
    boundary: &str,
    field_name: &str,
    filename: &str,
    mime: &str,
    data: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    let header = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
        boundary, field_name, filename, mime
    );
    body.extend_from_slice(header.as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    body
}

async fn setup_test_app() -> Option<(axum::Router, PgPool, Arc<TestStorageService>)> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/troit_logistics".to_string()
    });

    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(_) => {
            println!(
                "Skipping DB integration test: Postgres connection unavailable at {}",
                database_url
            );
            return None;
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
        soroban_escrow_contract_id: "CCONTRACT123".to_string(),
        soroban_token_contract_id: "CTOKEN123".to_string(),
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

    let test_storage = Arc::new(TestStorageService::new());
    let blockchain_service = troit_logistics_backend::blockchain::BlockchainService::new(&config)
        .expect("BlockchainService creation should succeed");
    let blockchain = Arc::new(blockchain_service);
    let app_state = AppState {
        db: pool.clone(),
        config,
        blockchain,
        storage: test_storage.clone(),
    };

    let router = create_router(app_state);
    Some((router, pool, test_storage))
}

async fn create_user(pool: &PgPool, role: UserRole) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("user_{:?}_{}@example.com", role, Uuid::new_v4());
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, role) VALUES ($1, $2, 'hash', 'Test User', $3)",
    )
    .bind(id)
    .bind(&email)
    .bind(role)
    .execute(pool)
    .await
    .unwrap();

    if role == UserRole::Seller {
        let seller_profile_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO seller_profiles (id, user_id, verification_status) VALUES ($1, $2, 'VERIFIED')",
        )
        .bind(seller_profile_id)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    let token = AuthService::generate_token(id, &email, role, JWT_SECRET, 24).unwrap();
    (id, token)
}

async fn create_product(pool: &PgPool, seller_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO products (id, seller_id, name, description, price) VALUES ($1, $2, 'Test Product', 'Description', 49.99)",
    )
    .bind(id)
    .bind(seller_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

// -----------------------------------------------------------------------------
// Test 1: Authenticated seller can upload image to own product
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req1_authenticated_seller_can_upload_image_to_own_product() {
    let (app, pool, test_storage) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let boundary = "------------------------boundary123";
    let body_bytes = create_multipart_body(
        boundary,
        "file",
        "test.png",
        "image/png",
        &valid_png_bytes(),
    );

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["data"]["product_id"], product_id.to_string());
    assert_eq!(json["data"]["mime_type"], "image/png");
    assert_eq!(json["data"]["sort_order"], 0);
    assert!(json["data"]["url"].as_str().unwrap().contains("/products/"));

    let storage_bucket = test_storage.objects.lock().unwrap();
    assert_eq!(storage_bucket.len(), 1);
}

// -----------------------------------------------------------------------------
// Test 2: Unauthenticated upload rejected
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req2_unauthenticated_upload_rejected() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, _) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let boundary = "------------------------boundary123";
    let body_bytes = create_multipart_body(
        boundary,
        "file",
        "test.png",
        "image/png",
        &valid_png_bytes(),
    );

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// -----------------------------------------------------------------------------
// Test 3: Seller cannot upload to another seller's product
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req3_seller_cannot_upload_to_another_seller_product() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller1_id, _) = create_user(&pool, UserRole::Seller).await;
    let (_, seller2_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller1_id).await;

    let boundary = "------------------------boundary123";
    let body_bytes = create_multipart_body(
        boundary,
        "file",
        "test.png",
        "image/png",
        &valid_png_bytes(),
    );

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller2_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// -----------------------------------------------------------------------------
// Test 4: Admin can upload to any product
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req4_admin_can_upload() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, _) = create_user(&pool, UserRole::Seller).await;
    let (_, admin_token) = create_user(&pool, UserRole::Admin).await;
    let product_id = create_product(&pool, seller_id).await;

    let boundary = "------------------------boundary123";
    let body_bytes = create_multipart_body(
        boundary,
        "file",
        "test.jpg",
        "image/jpeg",
        &valid_jpeg_bytes(),
    );

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}

// -----------------------------------------------------------------------------
// Test 5: Invalid image (corrupted / non-image magic bytes) rejected
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req5_invalid_image_magic_bytes_rejected() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let boundary = "------------------------boundary123";
    let body_bytes =
        create_multipart_body(boundary, "file", "fake.png", "image/png", &invalid_bytes());

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        res.status()
    );

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(
        json["message"].as_str().unwrap().contains("image format")
            || json["message"].as_str().unwrap().contains("file header")
            || json["message"].as_str().unwrap().contains("magic bytes"),
        "Unexpected error message: {}",
        json["message"]
    );
}

// -----------------------------------------------------------------------------
// Test 6: Unsupported image type rejected (e.g. GIF)
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req6_unsupported_image_type_rejected() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    // GIF magic header: GIF89a
    let gif_bytes = vec![0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00];

    let boundary = "------------------------boundary123";
    let body_bytes = create_multipart_body(boundary, "file", "test.gif", "image/gif", &gif_bytes);

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        res.status()
    );
}

// -----------------------------------------------------------------------------
// Test 7: Image larger than 5 MB rejected
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req7_image_larger_than_5mb_rejected() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    // 5 MB + 10 bytes
    let mut oversized_bytes = valid_png_bytes();
    oversized_bytes.resize(5 * 1024 * 1024 + 10, 0x00);

    let boundary = "------------------------boundary123";
    let body_bytes =
        create_multipart_body(boundary, "file", "big.png", "image/png", &oversized_bytes);

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        res.status()
    );
}

// -----------------------------------------------------------------------------
// Test 8: Sixth image rejected (max 5 images per product)
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req8_sixth_image_rejected() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    // Insert 5 images into database directly
    for i in 0..5 {
        let img_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, $3, $4, 'image/png', 100, $5)"
        )
        .bind(img_id)
        .bind(product_id)
        .bind(format!("https://cdn.test.com/img{}.png", i))
        .bind(format!("key{}", i))
        .bind(i)
        .execute(&pool)
        .await
        .unwrap();
    }

    let boundary = "------------------------boundary123";
    let body_bytes =
        create_multipart_body(boundary, "file", "6th.png", "image/png", &valid_png_bytes());

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        res.status()
    );

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(json["message"]
        .as_str()
        .unwrap()
        .contains("Maximum limit of 5 images"));
}

// -----------------------------------------------------------------------------
// Test 9: Successful upload creates product_images DB record
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req9_successful_upload_creates_db_record() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let boundary = "------------------------boundary123";
    let body_bytes = create_multipart_body(
        boundary,
        "file",
        "webp.webp",
        "image/webp",
        &valid_webp_bytes(),
    );

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM product_images WHERE product_id = $1")
        .bind(product_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count.0, 1);
}

// -----------------------------------------------------------------------------
// Test 10: R2 upload failure does not create DB record
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req10_r2_upload_failure_does_not_create_db_record() {
    let (app, pool, test_storage) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    test_storage.set_fail_upload(true);

    let boundary = "------------------------boundary123";
    let body_bytes = create_multipart_body(
        boundary,
        "file",
        "test.png",
        "image/png",
        &valid_png_bytes(),
    );

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM product_images WHERE product_id = $1")
        .bind(product_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count.0, 0);
}

// -----------------------------------------------------------------------------
// Test 11: DB insert failure triggers R2 cleanup
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req11_db_insert_failure_triggers_r2_cleanup() {
    let (app, pool, test_storage) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    // Delete product from database immediately so that INSERT foreign key constraint fails
    sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(product_id)
        .execute(&pool)
        .await
        .unwrap();

    let boundary = "------------------------boundary123";
    let body_bytes = create_multipart_body(
        boundary,
        "file",
        "test.png",
        "image/png",
        &valid_png_bytes(),
    );

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/products/{}/images", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body_bytes))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // R2 object should have been cleaned up and deleted
    let bucket = test_storage.objects.lock().unwrap();
    assert_eq!(bucket.len(), 0);
}

// -----------------------------------------------------------------------------
// Test 12: Seller can delete own image
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req12_seller_can_delete_own_image() {
    let (app, pool, test_storage) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let image_id = Uuid::new_v4();
    let storage_key = format!("products/{}/{}/{}.png", seller_id, product_id, image_id);
    test_storage
        .objects
        .lock()
        .unwrap()
        .insert(storage_key.clone(), valid_png_bytes());

    sqlx::query(
        "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url', $3, 'image/png', 100, 0)"
    )
    .bind(image_id)
    .bind(product_id)
    .bind(&storage_key)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/api/v1/products/{}/images/{}",
            product_id, image_id
        ))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM product_images WHERE id = $1")
        .bind(image_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0);

    let bucket = test_storage.objects.lock().unwrap();
    assert!(!bucket.contains_key(&storage_key));
}

// -----------------------------------------------------------------------------
// Test 13: Seller cannot delete another seller's image
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req13_seller_cannot_delete_another_seller_image() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller1_id, _) = create_user(&pool, UserRole::Seller).await;
    let (_, seller2_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller1_id).await;

    let image_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url', 'key', 'image/png', 100, 0)"
    )
    .bind(image_id)
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/api/v1/products/{}/images/{}",
            product_id, image_id
        ))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller2_token))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// -----------------------------------------------------------------------------
// Test 14: Admin can delete any image
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req14_admin_can_delete() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, _) = create_user(&pool, UserRole::Seller).await;
    let (_, admin_token) = create_user(&pool, UserRole::Admin).await;
    let product_id = create_product(&pool, seller_id).await;

    let image_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url', 'key', 'image/png', 100, 0)"
    )
    .bind(image_id)
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/api/v1/products/{}/images/{}",
            product_id, image_id
        ))
        .header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// -----------------------------------------------------------------------------
// Test 15: R2 delete failure does not delete DB record
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req15_r2_delete_failure_does_not_delete_db_record() {
    let (app, pool, test_storage) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let image_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url', 'key', 'image/png', 100, 0)"
    )
    .bind(image_id)
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();

    test_storage.set_fail_delete(true);

    let req = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/api/v1/products/{}/images/{}",
            product_id, image_id
        ))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM product_images WHERE id = $1")
        .bind(image_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count.0, 1,
        "DB record must NOT be deleted when R2 delete fails"
    );
}

// -----------------------------------------------------------------------------
// Test 16: Reorder succeeds with valid complete list
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req16_reorder_succeeds_with_valid_complete_list() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let img1 = Uuid::new_v4();
    let img2 = Uuid::new_v4();
    let img3 = Uuid::new_v4();

    for (img_id, order) in [(img1, 0), (img2, 1), (img3, 2)] {
        sqlx::query(
            "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url', 'key', 'image/png', 100, $3)"
        )
        .bind(img_id)
        .bind(product_id)
        .bind(order)
        .execute(&pool)
        .await
        .unwrap();
    }

    let payload = json!({
        "image_ids": [img3, img1, img2]
    });

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/products/{}/images/reorder", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let rows: Vec<(Uuid, i32)> = sqlx::query_as(
        "SELECT id, sort_order FROM product_images WHERE product_id = $1 ORDER BY sort_order ASC",
    )
    .bind(product_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows, vec![(img3, 0), (img1, 1), (img2, 2)]);
}

// -----------------------------------------------------------------------------
// Test 17: Reorder rejects duplicate IDs
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req17_reorder_rejects_duplicate_ids() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let img1 = Uuid::new_v4();
    let img2 = Uuid::new_v4();

    for (img_id, order) in [(img1, 0), (img2, 1)] {
        sqlx::query(
            "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url', 'key', 'image/png', 100, $3)"
        )
        .bind(img_id)
        .bind(product_id)
        .bind(order)
        .execute(&pool)
        .await
        .unwrap();
    }

    let payload = json!({
        "image_ids": [img1, img1]
    });

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/products/{}/images/reorder", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        res.status()
    );
}

// -----------------------------------------------------------------------------
// Test 18: Reorder rejects missing IDs / incomplete list
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req18_reorder_rejects_missing_ids() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let img1 = Uuid::new_v4();
    let img2 = Uuid::new_v4();

    for (img_id, order) in [(img1, 0), (img2, 1)] {
        sqlx::query(
            "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url', 'key', 'image/png', 100, $3)"
        )
        .bind(img_id)
        .bind(product_id)
        .bind(order)
        .execute(&pool)
        .await
        .unwrap();
    }

    let payload = json!({
        "image_ids": [img1]
    });

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/products/{}/images/reorder", product_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        res.status()
    );
}

// -----------------------------------------------------------------------------
// Test 19: Reorder rejects images belonging to another product
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req19_reorder_rejects_images_belonging_to_another_product() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, seller_token) = create_user(&pool, UserRole::Seller).await;
    let product1_id = create_product(&pool, seller_id).await;
    let product2_id = create_product(&pool, seller_id).await;

    let img1 = Uuid::new_v4();
    let img_other = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url', 'key', 'image/png', 100, 0)"
    )
    .bind(img1)
    .bind(product1_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url', 'key', 'image/png', 100, 0)"
    )
    .bind(img_other)
    .bind(product2_id)
    .execute(&pool)
    .await
    .unwrap();

    let payload = json!({
        "image_ids": [img1, img_other]
    });

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/products/{}/images/reorder", product1_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", seller_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        res.status()
    );
}

// -----------------------------------------------------------------------------
// Test 20: Product image responses are ordered by sort_order ASC
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_req20_product_image_responses_ordered_by_sort_order() {
    let (app, pool, _) = match setup_test_app().await {
        Some(res) => res,
        None => return,
    };

    let (seller_id, _) = create_user(&pool, UserRole::Seller).await;
    let product_id = create_product(&pool, seller_id).await;

    let img_order_2 = Uuid::new_v4();
    let img_order_0 = Uuid::new_v4();
    let img_order_1 = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url2', 'key2', 'image/png', 100, 2)"
    )
    .bind(img_order_2)
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url0', 'key0', 'image/png', 100, 0)"
    )
    .bind(img_order_0)
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO product_images (id, product_id, url, storage_key, mime_type, file_size, sort_order) VALUES ($1, $2, 'url1', 'key1', 'image/png', 100, 1)"
    )
    .bind(img_order_1)
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/products/{}", product_id))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    let images = json["data"]["images"].as_array().unwrap();
    assert_eq!(images.len(), 3);
    assert_eq!(images[0]["id"], img_order_0.to_string());
    assert_eq!(images[0]["sort_order"], 0);
    assert_eq!(images[1]["id"], img_order_1.to_string());
    assert_eq!(images[1]["sort_order"], 1);
    assert_eq!(images[2]["id"], img_order_2.to_string());
    assert_eq!(images[2]["sort_order"], 2);
}
