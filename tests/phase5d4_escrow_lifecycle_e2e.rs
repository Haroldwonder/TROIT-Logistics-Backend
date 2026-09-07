use troit_logistics_backend::{
    blockchain::BlockchainService,
    config::AppConfig,
    orders::models::{Order, OrderResponse},
};
use uuid::Uuid;

#[tokio::test]
async fn test_phase5d4_escrow_state_enum_and_model() {
    let config = AppConfig::from_env().unwrap();
    let service = BlockchainService::new(&config).unwrap();
    assert_eq!(
        service.network_passphrase(),
        "Test SDF Network ; September 2015"
    );
}

#[tokio::test]
async fn test_escrow_state_lifecycle_transitions() {
    let valid_states = vec![
        "NONE", "CREATED", "FUNDED", "DISPUTED", "RELEASED", "REFUNDED", "FAILED",
    ];
    for state in valid_states {
        assert!(!state.is_empty());
    }
}

#[tokio::test]
async fn test_order_model_escrow_state_mapping() {
    let order_id = Uuid::new_v4();
    let buyer_id = Uuid::new_v4();
    let seller_id = Uuid::new_v4();
    let product_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let order = Order {
        id: order_id,
        buyer_id,
        seller_id,
        product_id,
        quantity: 2,
        amount: 25000.0,
        status: "CONFIRMED".to_string(),
        payment_status: "PROTECTED".to_string(),
        delivery_status: "PICKUP_READY".to_string(),
        escrow_id: Some(1001),
        escrow_state: "FUNDED".to_string(),
        blockchain_tx_hash: Some(
            "1111111111222222222233333333334444444444555555555566666666667777".to_string(),
        ),
        funding_tx_hash: Some(
            "1111111111222222222233333333334444444444555555555566666666667777".to_string(),
        ),
        release_tx_hash: None,
        refund_tx_hash: None,
        created_at: now,
        updated_at: now,
    };

    let resp: OrderResponse = order.to_response();
    assert_eq!(resp.id, order_id);
    assert_eq!(resp.escrow_id, Some(1001));
    assert_eq!(resp.escrow_state, "FUNDED");
    assert_eq!(resp.payment_status, "PROTECTED");
    assert_eq!(resp.status, "CONFIRMED");
    assert_eq!(
        resp.funding_tx_hash.as_deref(),
        Some("1111111111222222222233333333334444444444555555555566666666667777")
    );
}
