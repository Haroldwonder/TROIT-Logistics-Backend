use crate::{
    auth::handlers::{login_handler, logout_handler, me_handler, register_handler},
    inspections::handlers::{
        create_product_inspection_handler, get_product_inspection_handler,
        get_product_verification_summary_handler,
    },
    middleware::require_auth,
    models::AppState,
    notifications::handlers::{
        list_notifications_handler, mark_all_notifications_read_handler,
        mark_notification_read_handler,
    },
    orders::handlers::{
        confirm_delivery_handler, create_order_handler, create_pickup_inspection_handler,
        dispute_order_handler, fund_order_handler, get_order_handler, get_order_history_handler,
        list_orders_handler, refund_order_handler, resolve_dispute_handler,
        update_order_status_handler,
    },
    products::handlers::{
        create_product_handler, get_product_handler, list_products_handler, verify_product_handler,
    },
    seed::handlers::seed_demo_data_handler,
    seller::handlers::{
        get_current_seller_profile_handler, get_seller_profile_by_id_handler,
        get_seller_verification_handler, submit_seller_verification_handler,
        update_seller_profile_handler,
    },
    subscriptions::handlers::{
        get_seller_subscription_handler, update_seller_subscription_handler,
    },
    trust::handlers::{get_seller_trust_history_by_id_handler, get_seller_trust_history_handler},
    wishlist::handlers::{
        add_to_wishlist_handler, list_wishlist_handler, remove_from_wishlist_handler,
    },
};
use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde_json::{json, Value};

/// Infrastructure Health Check Handler: GET /health
pub async fn health_handler() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "TROIT Logistics Backend API",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// GET /api/v1/blockchain/config
/// Publishes the public Soroban coordinates (contract IDs, RPC URL, network passphrase)
/// a wallet-enabled frontend needs to build and simulate escrow transactions itself
/// before sending them to a wallet for signing. Contains no secrets.
pub async fn blockchain_config_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<Value> {
    Json(json!({
        "success": true,
        "data": {
            "escrow_contract_id": state.config.soroban_escrow_contract_id,
            "token_contract_id": state.config.soroban_token_contract_id,
            "rpc_url": state.config.stellar_rpc_url,
            "network_passphrase": state.config.stellar_network_passphrase,
        }
    }))
}

/// Constructs complete Axum application router hierarchy
pub fn create_router(state: AppState) -> Router {
    // 1. Auth routes
    let public_auth = Router::new()
        .route("/register", post(register_handler))
        .route("/login", post(login_handler))
        .route("/logout", post(logout_handler));

    let protected_auth = Router::new()
        .route("/me", get(me_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let auth_routes = Router::new().merge(public_auth).merge(protected_auth);

    // 2. Product & Inspection routes
    let public_products = Router::new()
        .route("/", get(list_products_handler))
        .route("/:id", get(get_product_handler))
        .route("/:id/verify", patch(verify_product_handler))
        .route("/:id/inspection", get(get_product_inspection_handler))
        .route(
            "/:id/verification",
            get(get_product_verification_summary_handler),
        );

    let protected_products = Router::new()
        .route("/", post(create_product_handler))
        .route("/:id/inspection", post(create_product_inspection_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let product_routes = Router::new()
        .merge(public_products)
        .merge(protected_products);

    // 3. Order & Delivery routes (Protected)
    let order_routes = Router::new()
        .route("/", post(create_order_handler))
        .route("/", get(list_orders_handler))
        .route("/:id", get(get_order_handler))
        .route("/:id/history", get(get_order_history_handler))
        .route("/:id/status", patch(update_order_status_handler))
        .route(
            "/:id/pickup-inspection",
            post(create_pickup_inspection_handler),
        )
        .route("/:id/fund", post(fund_order_handler))
        .route("/:id/confirm-delivery", post(confirm_delivery_handler))
        .route("/:id/refund", post(refund_order_handler))
        .route("/:id/dispute", post(dispute_order_handler))
        .route("/:id/resolve-dispute", post(resolve_dispute_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // 4. Seller & Trust routes
    let public_seller = Router::new()
        .route("/profile/:seller_id", get(get_seller_profile_by_id_handler))
        .route(
            "/trust/history/:seller_id",
            get(get_seller_trust_history_by_id_handler),
        );

    let protected_seller = Router::new()
        .route("/profile", get(get_current_seller_profile_handler))
        .route("/profile", patch(update_seller_profile_handler))
        .route("/verification", get(get_seller_verification_handler))
        .route("/verification", post(submit_seller_verification_handler))
        .route("/trust/history", get(get_seller_trust_history_handler))
        .route("/subscription", get(get_seller_subscription_handler))
        .route("/subscription", post(update_seller_subscription_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let seller_routes = Router::new().merge(public_seller).merge(protected_seller);

    // 5. Wishlist routes (Protected)
    let wishlist_routes = Router::new()
        .route("/", get(list_wishlist_handler))
        .route("/:product_id", post(add_to_wishlist_handler))
        .route("/:product_id", delete(remove_from_wishlist_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // 6. Notification routes (Protected)
    let notification_routes = Router::new()
        .route("/", get(list_notifications_handler))
        .route("/read-all", patch(mark_all_notifications_read_handler))
        .route("/:id/read", patch(mark_notification_read_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Combine API v1 routes
    let api_v1 = Router::new()
        .nest("/auth", auth_routes)
        .nest("/products", product_routes)
        .nest("/orders", order_routes)
        .nest("/seller", seller_routes)
        .nest("/wishlist", wishlist_routes)
        .nest("/notifications", notification_routes)
        .route("/blockchain/config", get(blockchain_config_handler))
        .route("/seed", post(seed_demo_data_handler));

    // Root Router
    Router::new()
        .route("/health", get(health_handler))
        .nest("/api/v1", api_v1)
        .with_state(state)
}
