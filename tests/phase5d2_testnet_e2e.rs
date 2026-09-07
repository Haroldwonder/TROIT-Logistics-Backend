#[cfg(test)]
mod tests {
    use troit_logistics_backend::{
        blockchain::{errors::BlockchainError, BlockchainService, TransactionStatus},
        config::AppConfig,
    };

    const TESTNET_CONTRACT_ID: &str = "CBJB3R7RZSXXA5IDZRDMXEBRTWEZKRUSEVIA5C3M5D7F4H3QDM4MJ42P";

    fn create_testnet_service() -> BlockchainService {
        let mut config = AppConfig::from_env().expect("Config loading failed");
        config.stellar_rpc_url = "https://soroban-testnet.stellar.org".to_string();
        config.stellar_network_passphrase = "Test SDF Network ; September 2015".to_string();
        config.soroban_escrow_contract_id = TESTNET_CONTRACT_ID.to_string();
        BlockchainService::new(&config).expect("BlockchainService creation failed")
    }

    #[tokio::test]
    async fn test_real_testnet_rpc_health() {
        let service = create_testnet_service();
        let health = service.check_health().await;
        assert!(
            health.is_ok(),
            "Stellar Testnet RPC health check must succeed"
        );
        assert_eq!(health.unwrap().status, "healthy");
    }

    #[tokio::test]
    async fn test_real_testnet_escrow_contract_id_configured() {
        let service = create_testnet_service();
        let client = service
            .escrow_client()
            .expect("Escrow client should be configured");
        assert_eq!(client.contract_id(), TESTNET_CONTRACT_ID);
    }

    #[tokio::test]
    async fn test_real_testnet_tx_funding_semantic_verification() {
        let service = create_testnet_service();
        // Real fund_escrow transaction executed on Stellar Testnet for escrow_id 1
        let real_tx_hash = "c3b6cba44b89d9eb3dc9bb2e75b96e9eb907ea0aa162c1b40f9d45dd130d92dc";

        let res = service
            .verify_transaction_semantics(real_tx_hash, "fund_escrow", 1, Some(10.0))
            .await;

        assert!(
            res.is_ok(),
            "Real Stellar Testnet funding transaction must pass semantic verification: {:?}",
            res
        );
        assert_eq!(res.unwrap(), TransactionStatus::Success);
    }

    #[tokio::test]
    async fn test_real_testnet_tx_release_semantic_verification() {
        let service = create_testnet_service();
        // Real release_escrow transaction executed on Stellar Testnet for escrow_id 1
        let real_tx_hash = "327a76df217c764386362c7fe7d97173084319352cb914ce37b78e826c54607f";

        let res = service
            .verify_transaction_semantics(real_tx_hash, "release_escrow", 1, Some(10.0))
            .await;

        assert!(
            res.is_ok(),
            "Real Stellar Testnet release transaction must pass semantic verification: {:?}",
            res
        );
        assert_eq!(res.unwrap(), TransactionStatus::Success);
    }

    #[tokio::test]
    async fn test_real_testnet_tx_refund_semantic_verification() {
        let service = create_testnet_service();
        // Real refund_escrow transaction executed on Stellar Testnet for escrow_id 2
        let real_tx_hash = "623af4811360ddd3dbb68ad134dcb7c7c8f76e4343a045b596aa57b3c6043fcf";

        let res = service
            .verify_transaction_semantics(real_tx_hash, "refund_escrow", 2, Some(5.0))
            .await;

        assert!(
            res.is_ok(),
            "Real Stellar Testnet refund transaction must pass semantic verification: {:?}",
            res
        );
        assert_eq!(res.unwrap(), TransactionStatus::Success);
    }

    #[tokio::test]
    async fn test_real_testnet_mismatched_escrow_id_rejected() {
        let service = create_testnet_service();
        // Transaction for escrow_id 1 checked against expected escrow_id 999
        let real_tx_hash = "c3b6cba44b89d9eb3dc9bb2e75b96e9eb907ea0aa162c1b40f9d45dd130d92dc";

        let res = service
            .verify_transaction_semantics(real_tx_hash, "fund_escrow", 999, Some(10.0))
            .await;

        assert!(res.is_err());
        match res.unwrap_err() {
            BlockchainError::SemanticMismatch(msg) => {
                assert!(msg.contains("Escrow ID mismatch"));
            }
            err => panic!(
                "Expected SemanticMismatch for wrong escrow ID, got {:?}",
                err
            ),
        }
    }

    #[tokio::test]
    async fn test_real_testnet_mismatched_function_rejected() {
        let service = create_testnet_service();
        // Funding transaction checked against expected function "release_escrow"
        let real_tx_hash = "c3b6cba44b89d9eb3dc9bb2e75b96e9eb907ea0aa162c1b40f9d45dd130d92dc";

        let res = service
            .verify_transaction_semantics(real_tx_hash, "release_escrow", 1, Some(10.0))
            .await;

        assert!(res.is_err());
        match res.unwrap_err() {
            BlockchainError::SemanticMismatch(msg) => {
                assert!(msg.contains("Function mismatch"));
            }
            err => panic!(
                "Expected SemanticMismatch for wrong function, got {:?}",
                err
            ),
        }
    }

    #[tokio::test]
    async fn test_mock_hash_prefix_rejected_in_testnet() {
        let service = create_testnet_service();
        let res = service
            .verify_transaction_semantics("tx-fund-100", "fund_escrow", 100, Some(10.0))
            .await;

        assert!(res.is_err());
        match res.unwrap_err() {
            BlockchainError::SemanticMismatch(msg) => {
                assert!(msg.contains("Mock transaction hashes are not accepted"));
            }
            err => panic!("Expected SemanticMismatch for mock hash, got {:?}", err),
        }
    }
}
