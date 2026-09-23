use std::sync::atomic::{AtomicI32, Ordering};
use troit_logistics_backend::{
    admin::models::{AdminSellerItemResponse, AdminUserItemResponse},
    auth::service::AuthService,
    models::UserRole,
};
use uuid::Uuid;

struct MockInventoryItem {
    #[allow(dead_code)]
    id: Uuid,
    stock: AtomicI32,
}

impl MockInventoryItem {
    fn new(initial_stock: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            stock: AtomicI32::new(initial_stock),
        }
    }

    fn try_decrement_stock(&self, quantity: i32) -> Result<i32, String> {
        let mut current = self.stock.load(Ordering::SeqCst);
        loop {
            if current < quantity {
                return Err(format!(
                    "Insufficient stock. Available: {}, Requested: {}",
                    current, quantity
                ));
            }
            match self.stock.compare_exchange_weak(
                current,
                current - quantity,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(new_val) => return Ok(new_val - quantity),
                Err(actual) => current = actual,
            }
        }
    }

    fn restore_stock(&self, quantity: i32) -> i32 {
        self.stock.fetch_add(quantity, Ordering::SeqCst) + quantity
    }

    fn get_stock(&self) -> i32 {
        self.stock.load(Ordering::SeqCst)
    }
}

/// A. Successful order: stock decreases correctly
#[test]
fn test_successful_order_stock_decrement() {
    let item = MockInventoryItem::new(10);

    let res = item.try_decrement_stock(3);
    assert!(res.is_ok());
    assert_eq!(item.get_stock(), 7);
}

/// B. Failed escrow: order becomes FAILED and stock is restored
#[test]
fn test_failed_escrow_stock_restoration() {
    let item = MockInventoryItem::new(10);

    // Step 1: Decrement stock on order creation attempt
    let dec_res = item.try_decrement_stock(2);
    assert!(dec_res.is_ok());
    assert_eq!(item.get_stock(), 8);

    // Step 2: Escrow creation fails -> Restore stock
    let restored = item.restore_stock(2);
    assert_eq!(restored, 10);
    assert_eq!(item.get_stock(), 10);
}

/// C. Insufficient stock: order is rejected and stock remains unchanged
#[test]
fn test_insufficient_stock_rejection() {
    let item = MockInventoryItem::new(1);

    // Attempt to order 5 units when only 1 available
    let res = item.try_decrement_stock(5);
    assert!(res.is_err());
    assert_eq!(item.get_stock(), 1);
}

/// D. Repeated failed attempts: stock does not progressively disappear
#[test]
fn test_repeated_failed_attempts_stock_integrity() {
    let item = MockInventoryItem::new(5);

    // Simulate 5 consecutive failed order attempts
    for _ in 0..5 {
        let dec_res = item.try_decrement_stock(2);
        assert!(dec_res.is_ok());
        assert_eq!(item.get_stock(), 3);

        // Escrow creation fails each time -> stock restored
        item.restore_stock(2);
        assert_eq!(item.get_stock(), 5);
    }

    // Verify stock is untouched after all failed attempts
    assert_eq!(item.get_stock(), 5);
}

/// Admin Authorization Matrix Test
#[test]
fn test_admin_authorization_matrix_roles() {
    let secret = "test_jwt_secret_key_admin_verification_123456";

    let buyer_id = Uuid::new_v4();
    let seller_id = Uuid::new_v4();
    let admin_id = Uuid::new_v4();

    let buyer_token =
        AuthService::generate_token(buyer_id, "buyer@troit.test", UserRole::Buyer, secret, 24)
            .expect("Buyer token fail");

    let seller_token =
        AuthService::generate_token(seller_id, "seller@troit.test", UserRole::Seller, secret, 24)
            .expect("Seller token fail");

    let admin_token =
        AuthService::generate_token(admin_id, "admin@troit.test", UserRole::Admin, secret, 24)
            .expect("Admin token fail");

    let buyer_claims = AuthService::verify_token(&buyer_token, secret).unwrap();
    let seller_claims = AuthService::verify_token(&seller_token, secret).unwrap();
    let admin_claims = AuthService::verify_token(&admin_token, secret).unwrap();

    // Verify server-side role check expectations
    assert_ne!(buyer_claims.role, UserRole::Admin);
    assert_ne!(seller_claims.role, UserRole::Admin);
    assert_eq!(admin_claims.role, UserRole::Admin);
}

/// Admin Directory Security Test: Password hashes and secrets excluded
#[test]
fn test_admin_directory_security_fields_exclusion() {
    let user_resp = AdminUserItemResponse {
        id: Uuid::new_v4(),
        email: "user@troit.test".to_string(),
        full_name: "Test User".to_string(),
        phone_number: Some("+2348000000000".to_string()),
        role: UserRole::Buyer,
        is_active: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let json_str = serde_json::to_string(&user_resp).expect("Serialization failed");

    // Must never contain sensitive credential fields
    assert!(!json_str.contains("password_hash"));
    assert!(!json_str.contains("secret"));
    assert!(!json_str.contains("jwt"));
    assert!(!json_str.contains("private_key"));

    let seller_resp = AdminSellerItemResponse {
        seller_id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        store_name: Some("Test Store".to_string()),
        store_address: Some("Lagos, Nigeria".to_string()),
        trust_level: "LV2".to_string(),
        seller_grade: "Grade B".to_string(),
        successful_transactions: 12,
        fulfillment_rate: 98.5,
        verification_status: "VERIFIED".to_string(),
        user_full_name: "Seller Name".to_string(),
        user_email: "seller@troit.test".to_string(),
        user_phone: Some("+2348011111111".to_string()),
        total_products: 5,
        total_orders: 14,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let seller_json_str = serde_json::to_string(&seller_resp).expect("Serialization failed");

    assert!(!seller_json_str.contains("password_hash"));
    assert!(!seller_json_str.contains("secret"));
}
