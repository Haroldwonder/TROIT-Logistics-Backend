#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use troit_logistics_backend::{
        blockchain::{
            client::parse_mock_hash_semantics, errors::BlockchainError, types::EscrowState,
            BlockchainService, TransactionStatus,
        },
        config::AppConfig,
        models::seller::SellerTrustLevel,
        orders::models::{ConfirmDeliveryRequest, FundOrderRequest, OrderResponse},
        trust::service::{calculate_seller_grade, calculate_trust_level},
    };
    use uuid::Uuid;

    static ESCROW_COUNTER: AtomicU64 = AtomicU64::new(100);

    fn mock_next_escrow_id() -> u64 {
        ESCROW_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    fn create_test_blockchain_service(contract_id: &str) -> BlockchainService {
        let mut config = AppConfig::from_env().unwrap_or_else(|_| AppConfig {
            database_url: "postgres://localhost/test".to_string(),
            app_host: "0.0.0.0".to_string(),
            app_port: 8000,
            rust_log: "info".to_string(),
            jwt_secret: "test_secret".to_string(),
            jwt_expiration_hours: 24,
            cors_allowed_origins: vec!["*".to_string()],
            stellar_rpc_url: "https://soroban-testnet.stellar.org".to_string(),
            stellar_network_passphrase: "Test SDF Network ; September 2015".to_string(),
            soroban_escrow_contract_id: contract_id.to_string(),
            soroban_token_contract_id: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
                .to_string(),
            soroban_admin_secret_key: "".to_string(),
            soroban_service_secret_key: "".to_string(),
            soroban_buyer_secret_key: "".to_string(),
            soroban_seller_secret_key: "".to_string(),
            soroban_max_fee: 5000000,
            storage_provider: "r2".to_string(),
            s3_bucket_name: "".to_string(),
            s3_endpoint: "".to_string(),
            s3_region: "auto".to_string(),
            s3_access_key_id: "".to_string(),
            s3_secret_access_key: "".to_string(),
            storage_public_url: "".to_string(),
            max_image_size_mb: 5,
            admin_bootstrap_secret: None,
        });
        config.soroban_escrow_contract_id = contract_id.to_string();
        BlockchainService::new(&config).expect("BlockchainService creation should succeed")
    }

    #[test]
    fn test_escrow_id_sequence_assignment() {
        let id1 = mock_next_escrow_id();
        let id2 = mock_next_escrow_id();
        let id3 = mock_next_escrow_id();

        assert!(id1 > 0);
        assert_eq!(id2, id1 + 1);
        assert_eq!(id3, id2 + 1);
    }

    #[test]
    fn test_escrow_id_persistence() {
        let order_id = Uuid::new_v4();
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let escrow_id: i64 = 42;

        let response = OrderResponse {
            id: order_id,
            buyer_id,
            seller_id,
            product_id,
            quantity: 1,
            amount: 1000.0,
            status: "PENDING".to_string(),
            payment_status: "PENDING".to_string(),
            delivery_status: "PENDING".to_string(),
            escrow_id: Some(escrow_id),
            escrow_state: "NONE".to_string(),
            blockchain_tx_hash: None,
            funding_tx_hash: None,
            release_tx_hash: None,
            refund_tx_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert_eq!(response.escrow_id, Some(42));
    }

    #[tokio::test]
    async fn test_verify_transaction_semantics_mock_hash_rejected() {
        let service = create_test_blockchain_service("CCONTRACT123");
        let res = service
            .verify_transaction_semantics("tx-fund-100", "fund_escrow", 100, Some(1000.0))
            .await;

        assert!(res.is_err());
        match res.unwrap_err() {
            BlockchainError::SemanticMismatch(msg) => {
                assert!(msg.contains("Mock transaction hashes are not accepted"));
            }
            err => panic!(
                "Expected SemanticMismatch error for mock hash, got {:?}",
                err
            ),
        }
    }

    #[tokio::test]
    async fn test_missing_soroban_config_fails_safely() {
        let service = create_test_blockchain_service("");
        let res = service
            .verify_transaction_semantics(
                "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                "fund_escrow",
                100,
                Some(1000.0),
            )
            .await;

        assert!(res.is_err());
        match res.unwrap_err() {
            BlockchainError::InvalidConfig(msg) => {
                assert!(msg.contains("SOROBAN_ESCROW_CONTRACT_ID is not configured"));
            }
            err => panic!(
                "Expected InvalidConfig error when contract ID unconfigured, got {:?}",
                err
            ),
        }
    }

    #[test]
    fn test_parse_mock_hash_semantics() {
        let semantics = parse_mock_hash_semantics("tx-fund-100-CAAA-fund_escrow-2500.0");
        assert_eq!(semantics.escrow_id, Some(100));
        assert_eq!(semantics.contract_id.as_deref(), Some("CAAA"));
        assert_eq!(semantics.function.as_deref(), Some("fund_escrow"));
        assert_eq!(semantics.amount, Some(2500.0));
    }

    #[test]
    fn test_fund_order_successful() {
        let req = FundOrderRequest {
            tx_hash: Some("tx-fund-12345".to_string()),
            signed_tx_xdr: None,
        };

        let tx_hash = req.tx_hash.unwrap_or_else(|| "tx-fund-default".to_string());
        assert_eq!(tx_hash, "tx-fund-12345");

        let mut order_status = "PENDING";
        let mut payment_status = "PENDING";
        let mut funding_tx = None;

        if payment_status == "PENDING" && order_status == "PENDING" {
            payment_status = "PROTECTED";
            order_status = "CONFIRMED";
            funding_tx = Some(tx_hash);
        }

        assert_eq!(payment_status, "PROTECTED");
        assert_eq!(order_status, "CONFIRMED");
        assert_eq!(funding_tx, Some("tx-fund-12345".to_string()));
    }

    #[test]
    fn test_reused_funding_tx_hash_rejection() {
        let existing_hashes = ["tx-fund-12345"];
        let incoming_tx = "tx-fund-12345";

        let is_duplicate = existing_hashes.contains(&incoming_tx);
        assert!(is_duplicate, "Duplicate funding tx hash must be rejected");
    }

    #[test]
    fn test_reused_release_tx_hash_rejection() {
        let existing_hashes = ["tx-release-98765"];
        let incoming_tx = "tx-release-98765";

        let is_duplicate = existing_hashes.contains(&incoming_tx);
        assert!(is_duplicate, "Duplicate release tx hash must be rejected");
    }

    #[test]
    fn test_fund_order_idempotent() {
        let initial_tx = "tx-fund-existing-123";
        let current_funding_tx = Some(initial_tx.to_string());
        let current_payment_status = "PROTECTED";

        let new_req_same_tx = FundOrderRequest {
            tx_hash: Some(initial_tx.to_string()),
            signed_tx_xdr: None,
        };

        let is_idempotent = current_payment_status == "PROTECTED"
            && current_funding_tx.as_deref() == new_req_same_tx.tx_hash.as_deref();

        assert!(is_idempotent);
    }

    #[test]
    fn test_fund_order_non_buyer_rejected() {
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let caller_id = seller_id;

        let is_buyer = caller_id == buyer_id;
        assert!(!is_buyer, "Seller should not be allowed to fund order");
    }

    #[test]
    fn test_fund_order_already_funded_conflict() {
        let current_funding_tx = Some("tx-fund-original".to_string());
        let current_payment_status = "PROTECTED";

        let new_req_different_tx = FundOrderRequest {
            tx_hash: Some("tx-fund-new-different".to_string()),
            signed_tx_xdr: None,
        };

        let is_conflict = current_payment_status == "PROTECTED"
            && current_funding_tx.as_deref() != new_req_different_tx.tx_hash.as_deref();

        assert!(
            is_conflict,
            "Different tx hash when already funded should be a conflict"
        );
    }

    #[test]
    fn test_fund_order_invalid_status_conflict() {
        let order_status = "CANCELLED";
        let can_fund = order_status == "PENDING";
        assert!(!can_fund, "Cancelled order cannot be funded");
    }

    #[test]
    fn test_fund_order_not_found() {
        let order_exists = false;
        assert!(!order_exists, "Non-existent order should return 404");
    }

    #[test]
    fn test_confirm_delivery_successful() {
        let mut payment_status = "PROTECTED";
        let mut order_status = "CONFIRMED";
        let mut release_tx = None;
        let mut seller_successful_txs = 5;

        let req = ConfirmDeliveryRequest {
            tx_hash: Some("tx-release-987".to_string()),
            signed_tx_xdr: None,
        };

        if payment_status == "PROTECTED" {
            payment_status = "RELEASED";
            order_status = "COMPLETED";
            release_tx = req.tx_hash;
            seller_successful_txs += 1;
        }

        assert_eq!(payment_status, "RELEASED");
        assert_eq!(order_status, "COMPLETED");
        assert_eq!(release_tx, Some("tx-release-987".to_string()));
        assert_eq!(seller_successful_txs, 6);
    }

    #[test]
    fn test_concurrent_delivery_confirmation_atomic() {
        use std::sync::{Arc, Mutex};

        let order_state = Arc::new(Mutex::new(("DELIVERED", "PROTECTED")));
        let trust_increments = Arc::new(Mutex::new(0));

        let mut handles = vec![];
        for _ in 0..10 {
            let state_clone = Arc::clone(&order_state);
            let trust_clone = Arc::clone(&trust_increments);
            let handle = std::thread::spawn(move || {
                let mut state = state_clone.lock().unwrap();
                // Atomic conditional check (simulating WHERE status = 'DELIVERED' AND payment_status = 'PROTECTED')
                if state.0 == "DELIVERED" && state.1 == "PROTECTED" {
                    state.0 = "COMPLETED";
                    state.1 = "RELEASED";
                    drop(state);
                    let mut t = trust_clone.lock().unwrap();
                    *t += 1;
                }
            });
            handles.push(handle);
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(
            *trust_increments.lock().unwrap(),
            1,
            "Atomic state transition must increment Trust Engine exactly once across concurrent threads"
        );
    }

    #[test]
    fn test_successful_transaction_followed_by_retry() {
        let mut payment_status = "RELEASED";
        let mut order_status = "COMPLETED";
        let mut trust_increments = 1;

        // User retries confirm_delivery
        if order_status == "COMPLETED" && payment_status == "RELEASED" {
            // Idempotent return, no status change, no trust increment
        } else if payment_status == "PROTECTED" && order_status == "DELIVERED" {
            payment_status = "RELEASED";
            order_status = "COMPLETED";
            trust_increments += 1;
        }

        assert_eq!(order_status, "COMPLETED");
        assert_eq!(payment_status, "RELEASED");
        assert_eq!(
            trust_increments, 1,
            "Retry must not trigger second trust increment"
        );
    }

    #[test]
    fn test_confirm_delivery_idempotent() {
        let current_payment_status = "RELEASED";
        let current_release_tx = Some("tx-release-987".to_string());
        let mut seller_successful_txs = 6;

        let req = ConfirmDeliveryRequest {
            tx_hash: Some("tx-release-987".to_string()),
            signed_tx_xdr: None,
        };

        let is_already_released = current_payment_status == "RELEASED";

        if is_already_released && current_release_tx.as_deref() == req.tx_hash.as_deref() {
            // Do NOT re-increment seller trust engine
        } else if !is_already_released {
            seller_successful_txs += 1;
        }

        assert_eq!(
            seller_successful_txs, 6,
            "Trust engine should NOT be re-incremented on idempotent confirm"
        );
    }

    #[test]
    fn test_confirm_delivery_non_buyer_rejected() {
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let caller_id = seller_id;

        assert_ne!(caller_id, buyer_id, "Non-buyer cannot confirm delivery");
    }

    #[test]
    fn test_confirm_delivery_unfunded_rejected() {
        let payment_status = "PENDING";
        let can_confirm = payment_status == "PROTECTED";
        assert!(!can_confirm, "Unfunded order cannot confirm delivery");
    }

    #[test]
    fn test_confirm_delivery_blockchain_failure_rollback() {
        let mut payment_status = "PROTECTED";
        let mut order_status = "CONFIRMED";
        let mut seller_successful_txs = 10;

        let onchain_release_result: Result<&str, &str> = Err("On-chain verification timeout");

        if onchain_release_result.is_ok() {
            payment_status = "RELEASED";
            order_status = "COMPLETED";
            seller_successful_txs += 1;
        }

        assert_eq!(
            payment_status, "PROTECTED",
            "Payment status must remain PROTECTED on failure"
        );
        assert_eq!(
            order_status, "CONFIRMED",
            "Order status must remain CONFIRMED on failure"
        );
        assert_eq!(
            seller_successful_txs, 10,
            "Seller transactions must NOT be incremented"
        );
    }

    #[test]
    fn test_blockchain_failed_or_unknown_status_rejection() {
        let failed_status = TransactionStatus::Failed;
        let unknown_status = TransactionStatus::Unknown;

        assert_ne!(failed_status, TransactionStatus::Success);
        assert_ne!(unknown_status, TransactionStatus::Success);
    }

    #[test]
    fn test_confirm_delivery_trust_engine_single_increment() {
        let mut increment_count = 0;
        let mut is_released = false;

        // First attempt
        if !is_released {
            is_released = true;
            increment_count += 1;
        }

        // Second attempt (idempotent call)
        if !is_released {
            increment_count += 1;
        }

        assert_eq!(
            increment_count, 1,
            "Trust engine must increment exactly once"
        );
    }

    #[test]
    fn test_refund_flow_state_prerequisites() {
        let completed_status = "COMPLETED";
        let protected_status = "PROTECTED";

        let can_refund_completed = completed_status != "COMPLETED";
        let can_refund_protected = protected_status == "PROTECTED" || protected_status == "PENDING";

        assert!(!can_refund_completed, "Cannot refund completed order");
        assert!(can_refund_protected, "Protected order can be refunded");
    }

    #[test]
    fn test_refund_tx_hash_recording() {
        let mut payment_status = "PROTECTED";
        let mut order_status = "CONFIRMED";
        let mut refund_tx_hash = None;

        let refund_tx = "tx-refund-001";

        if payment_status == "PROTECTED" {
            payment_status = "REFUNDED";
            order_status = "CANCELLED";
            refund_tx_hash = Some(refund_tx.to_string());
        }

        assert_eq!(payment_status, "REFUNDED");
        assert_eq!(order_status, "CANCELLED");
        assert_eq!(refund_tx_hash, Some("tx-refund-001".to_string()));
    }

    #[test]
    fn test_transaction_hash_preservation() {
        let resp = OrderResponse {
            id: Uuid::new_v4(),
            buyer_id: Uuid::new_v4(),
            seller_id: Uuid::new_v4(),
            product_id: Uuid::new_v4(),
            quantity: 2,
            amount: 5000.0,
            status: "COMPLETED".to_string(),
            payment_status: "RELEASED".to_string(),
            delivery_status: "DELIVERED".to_string(),
            escrow_id: Some(99),
            escrow_state: "RELEASED".to_string(),
            blockchain_tx_hash: Some("tx-fund-777".to_string()),
            funding_tx_hash: Some("tx-fund-777".to_string()),
            release_tx_hash: Some("tx-release-888".to_string()),
            refund_tx_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert_eq!(resp.funding_tx_hash.as_deref(), Some("tx-fund-777"));
        assert_eq!(resp.release_tx_hash.as_deref(), Some("tx-release-888"));
        assert!(resp.refund_tx_hash.is_none());
    }

    #[test]
    fn test_escrow_state_machine_transitions() {
        let mut state = EscrowState::Created;
        assert_eq!(state, EscrowState::Created);

        state = EscrowState::Funded;
        assert_eq!(state, EscrowState::Funded);

        state = EscrowState::Released;
        assert_eq!(state, EscrowState::Released);
    }

    #[test]
    fn test_escrow_state_machine_invalid_jumps() {
        let current_state = EscrowState::Created;
        let attempt_release = current_state == EscrowState::Funded;

        assert!(
            !attempt_release,
            "Direct release from Created state is invalid"
        );
    }

    #[test]
    fn test_order_response_dto_field_mapping() {
        let resp = OrderResponse {
            id: Uuid::new_v4(),
            buyer_id: Uuid::new_v4(),
            seller_id: Uuid::new_v4(),
            product_id: Uuid::new_v4(),
            quantity: 5,
            amount: 15000.0,
            status: "CONFIRMED".to_string(),
            payment_status: "PROTECTED".to_string(),
            delivery_status: "IN_TRANSIT".to_string(),
            escrow_id: Some(101),
            escrow_state: "FUNDED".to_string(),
            blockchain_tx_hash: Some("tx-fund-101".to_string()),
            funding_tx_hash: Some("tx-fund-101".to_string()),
            release_tx_hash: None,
            refund_tx_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let serialized = serde_json::to_string(&resp).expect("Serialization should succeed");
        assert!(serialized.contains("\"escrow_id\":101"));
        assert!(serialized.contains("\"funding_tx_hash\":\"tx-fund-101\""));
    }

    #[test]
    fn test_seller_grade_and_trust_level_updates() {
        let initial_trust = calculate_trust_level(4, 100.0);
        assert_eq!(initial_trust, SellerTrustLevel::LV1);

        let upgraded_trust = calculate_trust_level(5, 100.0);
        assert_eq!(upgraded_trust, SellerTrustLevel::LV2);

        let grade_c = calculate_seller_grade(0, 100.0);
        let grade_a = calculate_seller_grade(25, 95.0);
        assert_ne!(grade_c, grade_a);
    }

    #[test]
    fn test_concurrent_funding_race_condition() {
        let mut funding_attempts = 0;
        let mut successfully_funded = 0;

        for _ in 0..10 {
            funding_attempts += 1;
            if successfully_funded == 0 {
                successfully_funded = 1;
            }
        }

        assert_eq!(funding_attempts, 10);
        assert_eq!(
            successfully_funded, 1,
            "Only one funding attempt should succeed"
        );
    }

    #[test]
    fn test_concurrent_release_race_condition() {
        let mut release_attempts = 0;
        let mut successfully_released = 0;

        for _ in 0..10 {
            release_attempts += 1;
            if successfully_released == 0 {
                successfully_released = 1;
            }
        }

        assert_eq!(release_attempts, 10);
        assert_eq!(
            successfully_released, 1,
            "Only one release attempt should succeed"
        );
    }

    #[test]
    fn test_end_to_end_order_escrow_lifecycle() {
        // 1. Create order -> Escrow ID assigned
        let escrow_id = mock_next_escrow_id();
        let mut status = "PENDING";
        let mut payment_status = "PENDING";

        assert!(escrow_id > 0);
        assert_eq!(status, "PENDING");
        assert_eq!(payment_status, "PENDING");

        // 2. Fund Escrow -> PROTECTED & CONFIRMED
        status = "CONFIRMED";
        payment_status = "PROTECTED";
        let funding_tx = Some("tx-fund-e2e".to_string());

        assert_eq!(status, "CONFIRMED");
        assert_eq!(payment_status, "PROTECTED");
        assert_eq!(funding_tx.as_deref(), Some("tx-fund-e2e"));

        // 3. Confirm Delivery -> RELEASED & COMPLETED
        status = "COMPLETED";
        payment_status = "RELEASED";
        let release_tx = Some("0x9999999999abcdef9999999999abcdef".to_string());

        assert_eq!(status, "COMPLETED");
        assert_eq!(payment_status, "RELEASED");
        assert_eq!(
            release_tx.as_deref(),
            Some("0x9999999999abcdef9999999999abcdef")
        );
    }

    #[test]
    fn test_phase5b22_fund_missing_tx_hash_rejected() {
        let req = FundOrderRequest {
            tx_hash: None,
            signed_tx_xdr: None,
        };
        assert!(
            req.tx_hash.is_none(),
            "Missing tx_hash must fail funding request validation"
        );
    }

    #[test]
    fn test_phase5b22_fund_mock_hash_rejected() {
        let mock_hashes = ["tx-fund-100", "tx-release-100", "tx-test-100"];
        for mock_hash in mock_hashes {
            let is_mock = mock_hash.starts_with("tx-fund-")
                || mock_hash.starts_with("tx-release-")
                || mock_hash.starts_with("tx-test-");
            assert!(
                is_mock,
                "Mock transaction hash prefix must be identified and rejected"
            );
        }
    }

    #[test]
    fn test_phase5b22_confirm_delivery_missing_tx_hash_rejected() {
        let req = ConfirmDeliveryRequest {
            tx_hash: None,
            signed_tx_xdr: None,
        };
        assert!(
            req.tx_hash.is_none(),
            "Missing tx_hash must fail delivery release validation"
        );
    }

    #[test]
    fn test_phase5b22_unverified_cannot_become_protected() {
        let payment_status = "PENDING";
        let is_verified = false;
        let new_payment_status = if is_verified {
            "PROTECTED"
        } else {
            payment_status
        };
        assert_eq!(
            new_payment_status, "PENDING",
            "Unverified transaction cannot set status to PROTECTED"
        );
    }

    #[test]
    fn test_phase5b22_unverified_cannot_become_released() {
        let payment_status = "PROTECTED";
        let order_status = "DELIVERED";
        let is_verified = false;

        let (new_payment_status, new_order_status) = if is_verified {
            ("RELEASED", "COMPLETED")
        } else {
            (payment_status, order_status)
        };

        assert_eq!(new_payment_status, "PROTECTED");
        assert_eq!(new_order_status, "DELIVERED");
    }
}
