use crate::{
    blockchain::TransactionStatus,
    errors::AppError,
    models::{order_history::OrderStatusHistory, AppState, Claims, UserRole},
    notifications::service::send_notification,
    orders::models::{
        ConfirmDeliveryRequest, CreateOrderRequest, CreatePickupInspectionRequest,
        DisputeOrderRequest, FundOrderRequest, Order, OrderResponse, OrderStatusHistoryResponse,
        PickupInspection, RefundOrderRequest, ResolveDisputeRequest, UpdateOrderStatusRequest,
    },
    products::models::Product,
    trust::service::record_successful_transaction,
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

/// Helper to record order status lifecycle audit trail
pub async fn record_order_status_history(
    db: &sqlx::PgPool,
    order_id: Uuid,
    status: &str,
    metadata: Option<serde_json::Value>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO order_status_history (order_id, status, metadata) VALUES ($1, $2, $3)",
    )
    .bind(order_id)
    .bind(status)
    .bind(metadata)
    .execute(db)
    .await?;
    Ok(())
}

/// POST /api/v1/orders
/// Buyers place an order for a verified product -> Server allocates escrow_id & establishes Soroban escrow on Testnet
pub async fn create_order_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<ApiResponse<OrderResponse>>, AppError> {
    let quantity = payload.quantity.unwrap_or(1);
    if quantity <= 0 {
        return Err(AppError::ValidationError(
            "Order quantity must be greater than zero".to_string(),
        ));
    }

    // Fetch product
    let product: Product = query_as::<_, Product>(
        "SELECT id, seller_id, name, description, price, condition, stock, verification_status, authenticity_status, last_inspected_at, is_african_made, african_made_category, warranty_months, warranty_terms, created_at, updated_at FROM products WHERE id = $1"
    )
    .bind(payload.product_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".to_string()))?;

    // Check product is verified for buyer purchase
    if product.verification_status != "VERIFIED" {
        return Err(AppError::BadRequest(
            "Only verified products can be ordered".to_string(),
        ));
    }

    // Check seller is not buying their own product
    if product.seller_id == claims.sub {
        return Err(AppError::BadRequest(
            "Sellers cannot place orders on their own products".to_string(),
        ));
    }

    // Check stock availability
    if product.stock < quantity {
        return Err(AppError::Conflict(format!(
            "Insufficient stock. Available: {}, Requested: {}",
            product.stock, quantity
        )));
    }

    // Calculate total amount server-side
    let total_amount = product.price * (quantity as f64);

    // Deduct stock
    sqlx::query("UPDATE products SET stock = stock - $1 WHERE id = $2")
        .bind(quantity)
        .bind(product.id)
        .execute(&state.db)
        .await?;

    // Generate unique numeric escrow_id sequence for Soroban escrow mapping
    let escrow_id_row: (i64,) = sqlx::query_as("SELECT nextval('order_escrow_id_seq')::BIGINT")
        .fetch_one(&state.db)
        .await?;
    let escrow_id = escrow_id_row.0;

    // Insert order with initial status PENDING, payment_status PENDING, escrow_state NONE
    let order: Order = query_as::<_, Order>(
        r#"
        INSERT INTO orders (buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state)
        VALUES ($1, $2, $3, $4, $5, 'PENDING', 'PENDING', 'PENDING', $6, 'NONE')
        RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
        "#
    )
    .bind(claims.sub)
    .bind(product.seller_id)
    .bind(product.id)
    .bind(quantity)
    .bind(total_amount)
    .bind(escrow_id)
    .fetch_one(&state.db)
    .await?;

    // Attempt Soroban create_escrow on Testnet if blockchain service executor is configured
    let updated_order: Order = if state.blockchain.executor().is_ok() {
        let buyer_pubkey = if !state.config.soroban_buyer_secret_key.is_empty() {
            crate::blockchain::signer::derive_public_key_from_secret(
                &state.config.soroban_buyer_secret_key,
            )
            .map_err(|e| {
                AppError::BlockchainError(format!("Invalid buyer signer configuration: {}", e))
            })?
        } else {
            state
                .blockchain
                .executor()
                .ok()
                .and_then(|e| e.service_signer().ok())
                .map(|s| s.public_key().to_string())
                .unwrap_or_default()
        };
        let seller_pubkey = if !state.config.soroban_seller_secret_key.is_empty() {
            crate::blockchain::signer::derive_public_key_from_secret(
                &state.config.soroban_seller_secret_key,
            )
            .map_err(|e| {
                AppError::BlockchainError(format!("Invalid seller signer configuration: {}", e))
            })?
        } else {
            state
                .blockchain
                .executor()
                .ok()
                .and_then(|e| e.service_signer().ok())
                .map(|s| s.public_key().to_string())
                .unwrap_or_default()
        };
        let token_addr = state.config.soroban_escrow_contract_id.clone();

        match state
            .blockchain
            .execute_create_escrow(
                escrow_id as u64,
                escrow_id as u64,
                &buyer_pubkey,
                &seller_pubkey,
                &token_addr,
                total_amount,
            )
            .await
        {
            Ok(exec_res) => query_as::<_, Order>(
                r#"
                UPDATE orders
                SET escrow_state = 'CREATED', blockchain_tx_hash = $1, updated_at = CURRENT_TIMESTAMP
                WHERE id = $2
                RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
                "#,
            )
            .bind(&exec_res.hash)
            .bind(order.id)
            .fetch_one(&state.db)
            .await?,
            Err(err) => {
                let _ = sqlx::query(
                    "UPDATE orders SET escrow_state = 'FAILED', updated_at = CURRENT_TIMESTAMP WHERE id = $1",
                )
                .bind(order.id)
                .execute(&state.db)
                .await;
                return Err(AppError::BlockchainError(format!(
                    "Soroban escrow creation failed on Testnet: {}. Order state marked FAILED.",
                    err
                )));
            }
        }
    } else {
        // Fallback for unconfigured test environments
        query_as::<_, Order>(
            r#"
            UPDATE orders
            SET escrow_state = 'CREATED', updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
            "#,
        )
        .bind(order.id)
        .fetch_one(&state.db)
        .await?
    };

    // Audit status lifecycle entry
    record_order_status_history(
        &state.db,
        updated_order.id,
        "ORDER_CREATED",
        Some(serde_json::json!({ "escrow_id": escrow_id, "escrow_state": updated_order.escrow_state, "blockchain_tx_hash": updated_order.blockchain_tx_hash })),
    )
    .await?;

    // Notify seller of incoming order
    let _ = send_notification(
        &state.db,
        updated_order.seller_id,
        "New Order Received",
        &format!(
            "You received a new order for product '{}' (Quantity: {})",
            product.name, quantity
        ),
        "NEW_ORDER",
    )
    .await;

    Ok(Json(ApiResponse {
        success: true,
        message: "Order placed successfully. Soroban escrow established on Testnet. Awaiting payment funding.".to_string(),
        data: Some(updated_order.to_response()),
    }))
}

/// POST /api/v1/orders/:id/fund
/// Buyer funds the escrow on-chain for an order -> Verifies/Executes Soroban fund_escrow transaction
pub async fn fund_order_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    payload: Option<Json<FundOrderRequest>>,
) -> Result<Json<ApiResponse<OrderResponse>>, AppError> {
    let order: Order = query_as::<_, Order>(
        "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    // Ownership check: Must be the buyer who placed the order
    if order.buyer_id != claims.sub {
        return Err(AppError::Forbidden(
            "Only the buyer who placed this order can fund payment".to_string(),
        ));
    }

    // Idempotency check: If already PROTECTED or FUNDED, return current state without error
    if order.payment_status == "PROTECTED" || order.escrow_state == "FUNDED" {
        return Ok(Json(ApiResponse {
            success: true,
            message: "Order payment is already protected in escrow.".to_string(),
            data: Some(order.to_response()),
        }));
    }

    // Verify order payment status is PENDING and escrow_state is CREATED
    if order.payment_status != "PENDING" {
        return Err(AppError::BadRequest(format!(
            "Cannot fund order in '{}' payment status.",
            order.payment_status
        )));
    }

    let escrow_id = order
        .escrow_id
        .ok_or_else(|| AppError::BadRequest("Order lacks valid escrow_id mapping".to_string()))?;

    let req = payload.map(|Json(p)| p).unwrap_or_default();

    let tx_hash = if let Some(hash) = req.tx_hash {
        if hash.trim().is_empty()
            || hash.starts_with("tx-fund-")
            || hash.starts_with("tx-release-")
            || hash.starts_with("tx-test-")
        {
            return Err(AppError::BadRequest(
                "Mock transaction hashes are not accepted for escrow funding.".to_string(),
            ));
        }

        // Verify existing transaction on ledger
        let confirmation_status = state
            .blockchain
            .verify_transaction_semantics(
                &hash,
                "fund_escrow",
                escrow_id as u64,
                Some(order.amount),
            )
            .await?;

        if confirmation_status != TransactionStatus::Success {
            return Err(AppError::BlockchainError(
                "Escrow funding transaction was not confirmed on Stellar ledger.".to_string(),
            ));
        }
        hash
    } else if state.blockchain.executor().is_ok() {
        // Execute real fund_escrow on Soroban Testnet via backend signer
        let exec_res = state
            .blockchain
            .execute_fund_escrow(escrow_id as u64, order.amount)
            .await?;
        exec_res.hash
    } else {
        return Err(AppError::BlockchainError(
            "Blockchain service executor unconfigured for funding execution".to_string(),
        ));
    };

    // Update payment_status to PROTECTED, status to CONFIRMED, escrow_state to FUNDED atomically
    let update_res = query_as::<_, Order>(
        r#"
        UPDATE orders
        SET payment_status = 'PROTECTED', status = 'CONFIRMED', escrow_state = 'FUNDED', funding_tx_hash = $1, blockchain_tx_hash = $1, updated_at = CURRENT_TIMESTAMP
        WHERE id = $2 AND payment_status = 'PENDING'
        RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
        "#
    )
    .bind(&tx_hash)
    .bind(order.id)
    .fetch_optional(&state.db)
    .await;

    let updated_order: Order = match update_res {
        Ok(Some(ord)) => ord,
        Ok(None) => {
            let re_fetch: Order = query_as::<_, Order>(
                "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
            )
            .bind(order.id)
            .fetch_one(&state.db)
            .await?;
            if re_fetch.payment_status == "PROTECTED" {
                return Ok(Json(ApiResponse {
                    success: true,
                    message: "Order payment is already protected in escrow.".to_string(),
                    data: Some(re_fetch.to_response()),
                }));
            } else {
                return Err(AppError::Conflict(
                    "Order state transition conflict or concurrent funding update.".to_string(),
                ));
            }
        }
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            return Err(AppError::Conflict(
                "Funding transaction hash has already been used.".to_string(),
            ));
        }
        Err(e) => return Err(e.into()),
    };

    // Record history
    record_order_status_history(&state.db, order.id, "PAYMENT_PROTECTED", None).await?;

    // Notify seller
    let _ = send_notification(
        &state.db,
        order.seller_id,
        "Order Funded & Protected",
        &format!(
            "Buyer funded escrow for order {}. Funds are locked in Soroban contract.",
            order.id
        ),
        "PAYMENT_PROTECTED",
    )
    .await;

    Ok(Json(ApiResponse {
        success: true,
        message: "Escrow funded successfully. Payment is now PROTECTED in contract.".to_string(),
        data: Some(updated_order.to_response()),
    }))
}

/// GET /api/v1/orders
/// List orders for the authenticated user (either as buyer or seller)
pub async fn list_orders_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ApiResponse<Vec<OrderResponse>>>, AppError> {
    let orders: Vec<Order> = query_as::<_, Order>(
        r#"
        SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
        FROM orders
        WHERE buyer_id = $1 OR seller_id = $1
        ORDER BY created_at DESC
        "#
    )
    .bind(claims.sub)
    .fetch_all(&state.db)
    .await?;

    let response_data = orders.into_iter().map(|o| o.to_response()).collect();

    Ok(Json(ApiResponse {
        success: true,
        message: "Orders retrieved successfully".to_string(),
        data: Some(response_data),
    }))
}

/// GET /api/v1/orders/:id
/// Gets details for a specific order with ownership verification
pub async fn get_order_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<OrderResponse>>, AppError> {
    let order: Order = query_as::<_, Order>(
        r#"
        SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
        FROM orders
        WHERE id = $1
        "#
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    // Authorization check
    if order.buyer_id != claims.sub
        && order.seller_id != claims.sub
        && claims.role != UserRole::Admin
        && claims.role != UserRole::FieldAgent
        && claims.role != UserRole::Rider
    {
        return Err(AppError::Forbidden(
            "You are not authorized to view this order".to_string(),
        ));
    }

    Ok(Json(ApiResponse {
        success: true,
        message: "Order details retrieved successfully".to_string(),
        data: Some(order.to_response()),
    }))
}

/// GET /api/v1/orders/:id/history
/// Retrieves order status lifecycle audit trail
pub async fn get_order_history_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<OrderStatusHistoryResponse>>>, AppError> {
    let order: Order = query_as::<_, Order>(
        "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    if order.buyer_id != claims.sub
        && order.seller_id != claims.sub
        && claims.role != UserRole::Admin
        && claims.role != UserRole::FieldAgent
        && claims.role != UserRole::Rider
    {
        return Err(AppError::Forbidden(
            "Not authorized to view order history".to_string(),
        ));
    }

    let history: Vec<OrderStatusHistory> = query_as::<_, OrderStatusHistory>(
        r#"
        SELECT id, order_id, status, metadata, created_at
        FROM order_status_history
        WHERE order_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let response: Vec<OrderStatusHistoryResponse> = history
        .into_iter()
        .map(|h| OrderStatusHistoryResponse {
            id: h.id,
            order_id: h.order_id,
            status: h.status,
            metadata: h.metadata,
            created_at: h.created_at,
        })
        .collect();

    Ok(Json(ApiResponse {
        success: true,
        message: "Fetched order status timeline history".to_string(),
        data: Some(response),
    }))
}

/// PATCH /api/v1/orders/:id/status
/// Update order status through lifecycle (CONFIRMED, READY_FOR_PICKUP, OUT_FOR_DELIVERY, DELIVERED)
pub async fn update_order_status_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateOrderStatusRequest>,
) -> Result<Json<ApiResponse<OrderResponse>>, AppError> {
    let new_status = payload.status.to_uppercase();
    let valid_statuses = [
        "CONFIRMED",
        "READY_FOR_PICKUP",
        "OUT_FOR_DELIVERY",
        "DELIVERED",
        "COMPLETED",
        "CANCELLED",
    ];

    if !valid_statuses.contains(&new_status.as_str()) {
        return Err(AppError::ValidationError(format!(
            "Invalid status '{}'. Must be one of: {:?}",
            new_status, valid_statuses
        )));
    }

    let existing_order: Order = query_as::<_, Order>(
        "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    // Authorization: seller, rider, field agent, or admin can update delivery lifecycle statuses
    if existing_order.seller_id != claims.sub
        && claims.role != UserRole::Rider
        && claims.role != UserRole::Admin
        && claims.role != UserRole::FieldAgent
    {
        return Err(AppError::Forbidden(
            "Not authorized to update order status".to_string(),
        ));
    }

    let delivery_status_update = match new_status.as_str() {
        "READY_FOR_PICKUP" => "PICKUP_READY",
        "OUT_FOR_DELIVERY" => "IN_TRANSIT",
        "DELIVERED" => "DELIVERED",
        "COMPLETED" => "DELIVERED",
        _ => &existing_order.delivery_status,
    };

    let updated_order: Order = query_as::<_, Order>(
        r#"
        UPDATE orders
        SET status = $1, delivery_status = $2, updated_at = CURRENT_TIMESTAMP
        WHERE id = $3
        RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
        "#
    )
    .bind(&new_status)
    .bind(delivery_status_update)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    // Audit status lifecycle entry
    record_order_status_history(&state.db, updated_order.id, &new_status, None).await?;

    // Notify buyer of status transition
    let _ = send_notification(
        &state.db,
        updated_order.buyer_id,
        "Order Status Updated",
        &format!("Your order is now {}", new_status),
        "ORDER_STATUS_UPDATE",
    )
    .await;

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Order status updated to {}", new_status),
        data: Some(updated_order.to_response()),
    }))
}

/// POST /api/v1/orders/:id/pickup-inspection
/// Records rider/agent pickup inspection for the order
pub async fn create_pickup_inspection_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CreatePickupInspectionRequest>,
) -> Result<Json<ApiResponse<PickupInspection>>, AppError> {
    let order: Order = query_as::<_, Order>(
        "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    let inspection_status = payload
        .inspection_status
        .unwrap_or_else(|| "PASSED".to_string())
        .to_uppercase();
    if inspection_status != "PASSED"
        && inspection_status != "FAILED"
        && inspection_status != "PENDING"
    {
        return Err(AppError::ValidationError(
            "Inspection status must be PASSED, FAILED, or PENDING".to_string(),
        ));
    }

    let inspection: PickupInspection = query_as::<_, PickupInspection>(
        r#"
        INSERT INTO pickup_inspections (order_id, inspector_id, condition, notes, inspection_status)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, order_id, inspector_id, condition, notes, inspection_status, created_at
        "#,
    )
    .bind(order.id)
    .bind(claims.sub)
    .bind(payload.condition.trim())
    .bind(payload.notes.as_deref().map(|n| n.trim()))
    .bind(&inspection_status)
    .fetch_one(&state.db)
    .await?;

    // Record inspection status in history
    record_order_status_history(
        &state.db,
        order.id,
        "PRODUCT_INSPECTED",
        Some(serde_json::json!({ "inspection_status": inspection_status, "condition": payload.condition })),
    )
    .await?;

    // If inspection passed, update order status to READY_FOR_PICKUP
    if inspection_status == "PASSED" {
        sqlx::query("UPDATE orders SET status = 'READY_FOR_PICKUP', delivery_status = 'PICKUP_READY', updated_at = CURRENT_TIMESTAMP WHERE id = $1")
            .bind(order.id)
            .execute(&state.db)
            .await?;

        record_order_status_history(&state.db, order.id, "READY_FOR_PICKUP", None).await?;
    }

    Ok(Json(ApiResponse {
        success: true,
        message: format!(
            "Pickup inspection recorded with status {}",
            inspection_status
        ),
        data: Some(inspection),
    }))
}

/// POST /api/v1/orders/:id/confirm-delivery
/// Buyer confirms delivery -> Verifies/Executes Soroban release transaction on-chain -> Order becomes COMPLETED & Payment status RELEASED, triggers Trust Engine
pub async fn confirm_delivery_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    payload: Option<Json<ConfirmDeliveryRequest>>,
) -> Result<Json<ApiResponse<OrderResponse>>, AppError> {
    let order: Order = query_as::<_, Order>(
        "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    // Must be the buyer who owns the order
    if order.buyer_id != claims.sub {
        return Err(AppError::Forbidden(
            "Only the buyer who placed this order can confirm delivery".to_string(),
        ));
    }

    // Idempotency check: Prevent double-confirmation / double release if already COMPLETED & RELEASED
    if order.status == "COMPLETED"
        && order.payment_status == "RELEASED"
        && order.escrow_state == "RELEASED"
    {
        return Ok(Json(ApiResponse {
            success: true,
            message: "Delivery already confirmed and payment released.".to_string(),
            data: Some(order.to_response()),
        }));
    }

    // Must be in DELIVERED status
    if order.status != "DELIVERED" {
        return Err(AppError::BadRequest(format!(
            "Cannot confirm delivery for order in '{}' status. Order must be DELIVERED.",
            order.status
        )));
    }

    // Escrow payment must be in PROTECTED state
    if order.payment_status != "PROTECTED" {
        return Err(AppError::BadRequest(format!(
            "Cannot release payment for order in '{}' payment status. Escrow must be PROTECTED.",
            order.payment_status
        )));
    }

    let escrow_id = order
        .escrow_id
        .ok_or_else(|| AppError::BadRequest("Order lacks valid escrow_id mapping".to_string()))?;

    let req = payload.map(|Json(p)| p).unwrap_or_default();

    let tx_hash = if let Some(hash) = req.tx_hash {
        if hash.trim().is_empty()
            || hash.starts_with("tx-fund-")
            || hash.starts_with("tx-release-")
            || hash.starts_with("tx-test-")
        {
            return Err(AppError::BadRequest(
                "Mock transaction hashes are not accepted for delivery release.".to_string(),
            ));
        }

        // Verify existing transaction on ledger
        let confirmation_status = state
            .blockchain
            .verify_transaction_semantics(&hash, "release_escrow", escrow_id as u64, None)
            .await?;

        if confirmation_status != TransactionStatus::Success {
            return Err(AppError::BlockchainError(
                "Escrow release transaction was not confirmed on Stellar ledger.".to_string(),
            ));
        }
        hash
    } else if state.blockchain.executor().is_ok() {
        // Execute real release_escrow on Soroban Testnet via backend signer
        let exec_res = state
            .blockchain
            .execute_release_escrow(escrow_id as u64)
            .await?;
        exec_res.hash
    } else {
        return Err(AppError::BlockchainError(
            "Blockchain service executor unconfigured for release execution".to_string(),
        ));
    };

    // Transition order to COMPLETED, payment to RELEASED, escrow_state to RELEASED atomically
    let update_res = query_as::<_, Order>(
        r#"
        UPDATE orders
        SET status = 'COMPLETED', payment_status = 'RELEASED', escrow_state = 'RELEASED', release_tx_hash = $1, blockchain_tx_hash = $1, updated_at = CURRENT_TIMESTAMP
        WHERE id = $2 AND status = 'DELIVERED' AND payment_status = 'PROTECTED'
        RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
        "#
    )
    .bind(&tx_hash)
    .bind(order.id)
    .fetch_optional(&state.db)
    .await;

    let updated_order: Order = match update_res {
        Ok(Some(ord)) => ord,
        Ok(None) => {
            let re_fetch: Order = query_as::<_, Order>(
                "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
            )
            .bind(order.id)
            .fetch_one(&state.db)
            .await?;
            if re_fetch.status == "COMPLETED" && re_fetch.payment_status == "RELEASED" {
                return Ok(Json(ApiResponse {
                    success: true,
                    message: "Delivery already confirmed and payment released.".to_string(),
                    data: Some(re_fetch.to_response()),
                }));
            } else {
                return Err(AppError::Conflict(
                    "Order state transition conflict or concurrent release update.".to_string(),
                ));
            }
        }
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            return Err(AppError::Conflict(
                "Release transaction hash has already been used.".to_string(),
            ));
        }
        Err(e) => return Err(e.into()),
    };

    // Audit status history
    record_order_status_history(&state.db, order.id, "COMPLETED", None).await?;

    // Trigger Trust Engine evaluation for the seller exactly once per completed transaction
    let trust_res = record_successful_transaction(&state.db, order.seller_id, order.id).await?;

    // Send Notifications
    let _ = send_notification(
        &state.db,
        order.seller_id,
        "Payment Released & Order Completed",
        &format!(
            "Buyer confirmed delivery for order {}. Protected escrow payment has been released on-chain. Total successful transactions: {}",
            order.id, trust_res.successful_transactions
        ),
        "PAYMENT_RELEASED",
    )
    .await;

    if trust_res.level_changed {
        let _ = send_notification(
            &state.db,
            order.seller_id,
            "Seller Trust Level Promoted!",
            &format!(
                "Congratulations! Your Troit seller trust level has advanced to {}!",
                trust_res.new_trust_level
            ),
            "TRUST_PROMOTED",
        )
        .await;
    }

    let _ = send_notification(
        &state.db,
        order.buyer_id,
        "Order Completed",
        "Thank you for confirming delivery! Your order is now officially complete.",
        "ORDER_COMPLETED",
    )
    .await;

    Ok(Json(ApiResponse {
        success: true,
        message: "Delivery confirmed. Order COMPLETED, protected payment RELEASED on-chain, and seller trust score updated."
            .to_string(),
        data: Some(updated_order.to_response()),
    }))
}

/// POST /api/v1/orders/:id/refund
/// Process refund for an order on Soroban Testnet
pub async fn refund_order_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    payload: Option<Json<RefundOrderRequest>>,
) -> Result<Json<ApiResponse<OrderResponse>>, AppError> {
    let order: Order = query_as::<_, Order>(
        "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    if order.buyer_id != claims.sub
        && order.seller_id != claims.sub
        && claims.role != UserRole::Admin
    {
        return Err(AppError::Forbidden(
            "Not authorized to refund this order".to_string(),
        ));
    }

    if order.payment_status == "REFUNDED" || order.escrow_state == "REFUNDED" {
        return Ok(Json(ApiResponse {
            success: true,
            message: "Order has already been refunded.".to_string(),
            data: Some(order.to_response()),
        }));
    }

    let escrow_id = order
        .escrow_id
        .ok_or_else(|| AppError::BadRequest("Order lacks valid escrow_id mapping".to_string()))?;

    let req = payload.map(|Json(p)| p);
    let reason = req
        .as_ref()
        .and_then(|r| r.reason.clone())
        .unwrap_or_else(|| "Order refund requested".to_string());

    let tx_hash = if let Some(r) = req.as_ref().and_then(|r| r.tx_hash.clone()) {
        if r.starts_with("tx-") {
            return Err(AppError::BadRequest(
                "Mock transaction hashes are not allowed".to_string(),
            ));
        }
        state
            .blockchain
            .verify_transaction_semantics(&r, "refund_escrow", escrow_id as u64, None)
            .await?;
        r
    } else if state.blockchain.executor().is_ok() {
        let exec_res = state
            .blockchain
            .execute_refund_escrow(escrow_id as u64)
            .await?;
        exec_res.hash
    } else {
        return Err(AppError::BlockchainError(
            "Blockchain service executor unconfigured for refund execution".to_string(),
        ));
    };

    let updated_order: Order = query_as::<_, Order>(
        r#"
        UPDATE orders
        SET status = 'CANCELLED', payment_status = 'REFUNDED', escrow_state = 'REFUNDED', refund_tx_hash = $1, blockchain_tx_hash = $1, updated_at = CURRENT_TIMESTAMP
        WHERE id = $2
        RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
        "#
    )
    .bind(&tx_hash)
    .bind(order.id)
    .fetch_one(&state.db)
    .await?;

    record_order_status_history(
        &state.db,
        order.id,
        "ORDER_REFUNDED",
        Some(serde_json::json!({ "reason": reason })),
    )
    .await?;

    let _ = send_notification(
        &state.db,
        order.buyer_id,
        "Order Refunded",
        &format!("Your order {} has been refunded on-chain.", order.id),
        "ORDER_REFUNDED",
    )
    .await;

    Ok(Json(ApiResponse {
        success: true,
        message: "Order refunded successfully.".to_string(),
        data: Some(updated_order.to_response()),
    }))
}

/// POST /api/v1/orders/:id/dispute
/// Raise a dispute on an order on Soroban Testnet
pub async fn dispute_order_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    payload: Option<Json<DisputeOrderRequest>>,
) -> Result<Json<ApiResponse<OrderResponse>>, AppError> {
    let order: Order = query_as::<_, Order>(
        "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    if order.buyer_id != claims.sub && order.seller_id != claims.sub {
        return Err(AppError::Forbidden(
            "Only buyer or seller can dispute this order".to_string(),
        ));
    }

    if order.status == "DISPUTED" || order.escrow_state == "DISPUTED" {
        return Ok(Json(ApiResponse {
            success: true,
            message: "Order is already in DISPUTED state.".to_string(),
            data: Some(order.to_response()),
        }));
    }

    if order.payment_status != "PROTECTED" && order.escrow_state != "FUNDED" {
        return Err(AppError::BadRequest(
            "Only funded/protected orders can be disputed".to_string(),
        ));
    }

    let escrow_id = order
        .escrow_id
        .ok_or_else(|| AppError::BadRequest("Order lacks valid escrow_id mapping".to_string()))?;

    let req = payload.map(|Json(p)| p);
    let tx_hash = if let Some(r) = req.as_ref().and_then(|r| r.tx_hash.clone()) {
        if r.starts_with("tx-") {
            return Err(AppError::BadRequest(
                "Mock transaction hashes are not allowed".to_string(),
            ));
        }
        state
            .blockchain
            .verify_transaction_semantics(&r, "dispute_escrow", escrow_id as u64, None)
            .await?;
        r
    } else if state.blockchain.executor().is_ok() {
        let caller_secret = if order.buyer_id == claims.sub {
            &state.config.soroban_buyer_secret_key
        } else {
            &state.config.soroban_seller_secret_key
        };
        let caller_pubkey = crate::blockchain::signer::derive_public_key_from_secret(caller_secret)
            .map_err(|e| {
                AppError::BlockchainError(format!("Invalid caller signer configuration: {}", e))
            })?;
        let exec_res = state
            .blockchain
            .execute_dispute_escrow(escrow_id as u64, &caller_pubkey)
            .await?;
        exec_res.hash
    } else {
        return Err(AppError::BlockchainError(
            "Blockchain service executor unconfigured for dispute execution".to_string(),
        ));
    };

    let updated_order: Order = query_as::<_, Order>(
        r#"
        UPDATE orders
        SET status = 'DISPUTED', escrow_state = 'DISPUTED', blockchain_tx_hash = $1, updated_at = CURRENT_TIMESTAMP
        WHERE id = $2
        RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
        "#
    )
    .bind(&tx_hash)
    .bind(order.id)
    .fetch_one(&state.db)
    .await?;

    let reason = req
        .map(|r| r.reason)
        .unwrap_or_else(|| "Escrow dispute raised".to_string());

    record_order_status_history(
        &state.db,
        order.id,
        "ORDER_DISPUTED",
        Some(serde_json::json!({ "reason": reason })),
    )
    .await?;

    let _ = send_notification(
        &state.db,
        if order.buyer_id == claims.sub {
            order.seller_id
        } else {
            order.buyer_id
        },
        "Order Disputed",
        &format!("A dispute was raised for order {}: {}", order.id, reason),
        "ORDER_DISPUTED",
    )
    .await;

    Ok(Json(ApiResponse {
        success: true,
        message: "Dispute raised successfully on-chain.".to_string(),
        data: Some(updated_order.to_response()),
    }))
}

/// POST /api/v1/orders/:id/resolve-dispute
/// Admin resolves order dispute on Soroban Testnet
pub async fn resolve_dispute_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ResolveDisputeRequest>,
) -> Result<Json<ApiResponse<OrderResponse>>, AppError> {
    if claims.role != UserRole::Admin {
        return Err(AppError::Forbidden(
            "Only admins can resolve order disputes".to_string(),
        ));
    }

    let order: Order = query_as::<_, Order>(
        "SELECT id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at FROM orders WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    if order.status != "DISPUTED" && order.escrow_state != "DISPUTED" {
        return Err(AppError::BadRequest(
            "Order is not in DISPUTED state".to_string(),
        ));
    }

    let escrow_id = order
        .escrow_id
        .ok_or_else(|| AppError::BadRequest("Order lacks valid escrow_id mapping".to_string()))?;

    let tx_hash = if let Some(r) = payload.tx_hash.clone() {
        if r.starts_with("tx-") {
            return Err(AppError::BadRequest(
                "Mock transaction hashes are not allowed".to_string(),
            ));
        }
        state
            .blockchain
            .verify_transaction_semantics(&r, "resolve_dispute", escrow_id as u64, None)
            .await?;
        r
    } else if state.blockchain.executor().is_ok() {
        let exec_res = state
            .blockchain
            .execute_resolve_dispute(escrow_id as u64, payload.release_to_seller)
            .await?;
        exec_res.hash
    } else {
        return Err(AppError::BlockchainError(
            "Blockchain service executor unconfigured for dispute resolution".to_string(),
        ));
    };

    let updated_order: Order = if payload.release_to_seller {
        let ord: Order = query_as::<_, Order>(
            r#"
            UPDATE orders
            SET status = 'COMPLETED', payment_status = 'RELEASED', escrow_state = 'RELEASED', release_tx_hash = $1, blockchain_tx_hash = $1, updated_at = CURRENT_TIMESTAMP
            WHERE id = $2
            RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
            "#
        )
        .bind(&tx_hash)
        .bind(order.id)
        .fetch_one(&state.db)
        .await?;

        record_order_status_history(&state.db, order.id, "DISPUTE_RESOLVED_RELEASED", None).await?;
        let _ = record_successful_transaction(&state.db, order.seller_id, order.id).await;

        let _ = send_notification(
            &state.db,
            order.seller_id,
            "Dispute Resolved - Payment Released",
            &format!(
                "Admin resolved dispute for order {}. Funds released to seller.",
                order.id
            ),
            "DISPUTE_RESOLVED",
        )
        .await;

        ord
    } else {
        let ord: Order = query_as::<_, Order>(
            r#"
            UPDATE orders
            SET status = 'CANCELLED', payment_status = 'REFUNDED', escrow_state = 'REFUNDED', refund_tx_hash = $1, blockchain_tx_hash = $1, updated_at = CURRENT_TIMESTAMP
            WHERE id = $2
            RETURNING id, buyer_id, seller_id, product_id, quantity, amount, status, payment_status, delivery_status, escrow_id, escrow_state, blockchain_tx_hash, funding_tx_hash, release_tx_hash, refund_tx_hash, created_at, updated_at
            "#
        )
        .bind(&tx_hash)
        .bind(order.id)
        .fetch_one(&state.db)
        .await?;

        record_order_status_history(&state.db, order.id, "DISPUTE_RESOLVED_REFUNDED", None).await?;

        let _ = send_notification(
            &state.db,
            order.buyer_id,
            "Dispute Resolved - Payment Refunded",
            &format!(
                "Admin resolved dispute for order {}. Funds refunded to buyer.",
                order.id
            ),
            "DISPUTE_RESOLVED",
        )
        .await;

        ord
    };

    Ok(Json(ApiResponse {
        success: true,
        message: format!(
            "Dispute resolved successfully. Escrow {}",
            if payload.release_to_seller {
                "RELEASED to seller"
            } else {
                "REFUNDED to buyer"
            }
        ),
        data: Some(updated_order.to_response()),
    }))
}
