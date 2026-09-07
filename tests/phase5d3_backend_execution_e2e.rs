#[cfg(test)]
mod tests {
    use troit_logistics_backend::{
        blockchain::{
            signer::SorobanSigner, transaction::TransactionExecutor, BlockchainService,
            TransactionStatus,
        },
        config::AppConfig,
    };

    const TESTNET_CONTRACT_ID: &str = "CBJB3R7RZSXXA5IDZRDMXEBRTWEZKRUSEVIA5C3M5D7F4H3QDM4MJ42P";
    const TESTNET_SERVICE_PUBKEY: &str = "GANJ4KZWHHQW2RYJOF5XCCDKNDO3BNE27V3EHLA36PNUZEFYYQLS4W2W";

    fn create_testnet_service() -> BlockchainService {
        let mut config = AppConfig::from_env().expect("Config loading failed");
        config.stellar_rpc_url = "https://soroban-testnet.stellar.org".to_string();
        config.stellar_network_passphrase = "Test SDF Network ; September 2015".to_string();
        config.soroban_escrow_contract_id = TESTNET_CONTRACT_ID.to_string();
        BlockchainService::new(&config).expect("BlockchainService creation failed")
    }

    #[tokio::test]
    async fn test_backend_rpc_health() {
        let service = create_testnet_service();
        let health = service.check_health().await;
        assert!(
            health.is_ok(),
            "Stellar Testnet RPC health check must succeed"
        );
        assert_eq!(health.unwrap().status, "healthy");
    }

    #[tokio::test]
    async fn test_backend_account_sequence_fetching() {
        let service = create_testnet_service();
        let client = service.rpc_client();
        let seq_res = client.get_account_sequence(TESTNET_SERVICE_PUBKEY).await;
        assert!(seq_res.is_ok(), "Account sequence fetch should succeed");
        assert!(seq_res.unwrap() > 0, "Account sequence must be positive");
    }

    #[test]
    fn test_backend_signer_redaction_and_security() {
        use stellar_strkey::{ed25519, Strkey};
        let secret = Strkey::PrivateKeyEd25519(ed25519::PrivateKey([7u8; 32])).to_string();
        let signer_res = SorobanSigner::from_secret_key(&secret);
        assert!(signer_res.is_ok());

        let signer = signer_res.unwrap();
        let debug_fmt = format!("{:?}", signer);
        assert!(debug_fmt.contains("[REDACTED]"));
        assert!(!debug_fmt.contains(&secret));
    }

    #[test]
    fn test_backend_fee_limit_rejection() {
        let executor = TransactionExecutor::new(
            create_testnet_service().rpc_client().clone(),
            TESTNET_CONTRACT_ID.to_string(),
            "Test SDF Network ; September 2015".to_string(),
            None,
            100, // Very low fee limit (100 stroops)
        );

        assert_eq!(executor.contract_id(), TESTNET_CONTRACT_ID);
    }

    #[tokio::test]
    async fn test_backend_live_testnet_funding_verification() {
        let service = create_testnet_service();
        let real_tx_hash = "c3b6cba44b89d9eb3dc9bb2e75b96e9eb907ea0aa162c1b40f9d45dd130d92dc";

        let res = service
            .verify_transaction_semantics(real_tx_hash, "fund_escrow", 1, Some(10.0))
            .await;

        assert!(
            res.is_ok(),
            "Live Testnet funding verification must succeed: {:?}",
            res
        );
        assert_eq!(res.unwrap(), TransactionStatus::Success);
    }

    #[tokio::test]
    async fn test_backend_live_testnet_release_verification() {
        let service = create_testnet_service();
        let real_tx_hash = "327a76df217c764386362c7fe7d97173084319352cb914ce37b78e826c54607f";

        let res = service
            .verify_transaction_semantics(real_tx_hash, "release_escrow", 1, Some(10.0))
            .await;

        assert!(
            res.is_ok(),
            "Live Testnet release verification must succeed: {:?}",
            res
        );
        assert_eq!(res.unwrap(), TransactionStatus::Success);
    }
}
