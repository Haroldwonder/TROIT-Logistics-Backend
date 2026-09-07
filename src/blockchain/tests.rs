#![cfg(test)]

use super::*;
use crate::config::AppConfig;
use crate::errors::AppError;
use types::EscrowState;

#[test]
fn test_blockchain_service_config_parsing() {
    let mut config = AppConfig::from_env().unwrap();
    config.stellar_rpc_url = "https://soroban-testnet.stellar.org".to_string();
    config.soroban_escrow_contract_id =
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAOLZM".to_string();

    let service = BlockchainService::new(&config).unwrap();
    assert_eq!(
        service.escrow_client().unwrap().contract_id(),
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAOLZM"
    );
    assert_eq!(
        service.network_passphrase(),
        "Test SDF Network ; September 2015"
    );
}

#[test]
fn test_missing_escrow_contract_id_returns_error() {
    let mut config = AppConfig::from_env().unwrap();
    config.soroban_escrow_contract_id = "".to_string();

    let service = BlockchainService::new(&config).unwrap();
    assert!(service.escrow_client().is_err());
}

#[test]
fn test_invalid_rpc_url_returns_error() {
    let res = SorobanRpcClient::new("".to_string());
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err(),
        BlockchainError::InvalidConfig("Stellar RPC URL cannot be empty".to_string())
    );
}

#[test]
fn test_escrow_contract_params_building() {
    let client = EscrowContractClient::new("CONTRACT123".to_string()).unwrap();

    let create_args = CreateEscrowArgs {
        escrow_id: 10,
        order_id: 100,
        buyer: "BUYER_ADDR".to_string(),
        seller: "SELLER_ADDR".to_string(),
        token: "TOKEN_ADDR".to_string(),
        amount: 5000,
    };

    let params = client.build_create_escrow_params(&create_args);
    assert_eq!(params["function"], "create_escrow");
    assert_eq!(params["args"][0], 10);
    assert_eq!(params["args"][1], 100);

    let release_params = client.build_release_escrow_params(10);
    assert_eq!(release_params["function"], "release_escrow");
    assert_eq!(release_params["args"][0], 10);
}

#[test]
fn test_escrow_record_dto_parsing() {
    let json_val = serde_json::json!({
        "escrow_id": 1,
        "order_id": 100,
        "buyer": "GBUYER123",
        "seller": "GSELLER123",
        "token": "CTOKEN123",
        "amount": 2500,
        "state": 2,
        "created_at": 1000,
        "funded_at": 1005,
        "released_at": 0,
        "refunded_at": 0
    });

    let record = EscrowContractClient::parse_escrow_record(json_val).unwrap();
    assert_eq!(record.escrow_id, 1);
    assert_eq!(record.order_id, 100);
    assert_eq!(record.amount, 2500);
    assert_eq!(record.state, EscrowState::Funded);
}

#[test]
fn test_blockchain_error_conversion_to_app_error() {
    let bc_err = BlockchainError::TransactionFailed("Low reserve balance".to_string());
    let app_err: AppError = bc_err.into();

    match app_err {
        AppError::BlockchainError(msg) => {
            assert!(msg.contains("Transaction Failed: Low reserve balance"));
        }
        _ => panic!("Expected AppError::BlockchainError"),
    }
}

#[test]
fn test_soroban_signer_key_derivation_and_signing() {
    use stellar_strkey::{ed25519, Strkey};
    let secret_key_str = Strkey::PrivateKeyEd25519(ed25519::PrivateKey([7u8; 32])).to_string();
    let signer_res = SorobanSigner::from_secret_key(&secret_key_str);
    assert!(signer_res.is_ok());

    let signer = signer_res.unwrap();
    assert!(signer.public_key().starts_with('G'));
    assert_eq!(signer.public_key().len(), 56);

    let payload = b"TROIT logistics transaction payload signature test";
    let sig = signer.sign_payload(payload);
    assert_eq!(sig.len(), 64);

    let debug_str = format!("{:?}", signer);
    assert!(debug_str.contains("[REDACTED]"));
    assert!(!debug_str.contains(&secret_key_str));
}

#[test]
fn test_soroban_signer_invalid_key_handling() {
    let invalid_res = SorobanSigner::from_secret_key("INVALID_KEY_STRING");
    assert!(invalid_res.is_err());
    let empty_res = SorobanSigner::from_secret_key("  ");
    assert!(empty_res.is_err());
}
