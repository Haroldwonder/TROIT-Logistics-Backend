use std::env;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AppConfig {
    pub database_url: String,
    pub app_host: String,
    pub app_port: u16,
    pub rust_log: String,
    pub jwt_secret: String,
    pub jwt_expiration_hours: i64,
    pub stellar_rpc_url: String,
    pub stellar_network_passphrase: String,
    pub soroban_escrow_contract_id: String,
    pub soroban_token_contract_id: String,
    pub soroban_admin_secret_key: String,
    pub soroban_service_secret_key: String,
    pub soroban_buyer_secret_key: String,
    pub soroban_seller_secret_key: String,
    pub soroban_max_fee: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        // Attempt to load .env file if available
        let _ = dotenvy::dotenv();

        let database_url = env::var("DATABASE_URL")
            .map_err(|_| "DATABASE_URL environment variable must be set".to_string())?;

        let app_host = env::var("APP_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let app_port = env::var("PORT")
            .or_else(|_| env::var("APP_PORT"))
            .unwrap_or_else(|_| "8000".to_string())
            .parse::<u16>()
            .map_err(|e| format!("Invalid port configuration: {}", e))?;

        let rust_log = env::var("RUST_LOG")
            .unwrap_or_else(|_| "info,troit_logistics_backend=debug".to_string());

        let jwt_secret = env::var("JWT_SECRET")
            .map_err(|_| "JWT_SECRET environment variable must be set".to_string())?;

        let jwt_expiration_hours = env::var("JWT_EXPIRATION_HOURS")
            .unwrap_or_else(|_| "24".to_string())
            .parse::<i64>()
            .map_err(|e| format!("Invalid JWT_EXPIRATION_HOURS configuration: {}", e))?;

        let stellar_rpc_url = env::var("STELLAR_RPC_URL")
            .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string());

        let stellar_network_passphrase = env::var("STELLAR_NETWORK_PASSPHRASE")
            .unwrap_or_else(|_| "Test SDF Network ; September 2015".to_string());

        let soroban_escrow_contract_id =
            env::var("SOROBAN_ESCROW_CONTRACT_ID").unwrap_or_else(|_| {
                "CBJB3R7RZSXXA5IDZRDMXEBRTWEZKRUSEVIA5C3M5D7F4H3QDM4MJ42P".to_string()
            });

        // Testnet native XLM Stellar Asset Contract, used as the escrow's payment
        // token unless overridden. Verified against the deployed passphrase above.
        let soroban_token_contract_id =
            env::var("SOROBAN_TOKEN_CONTRACT_ID").unwrap_or_else(|_| {
                "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC".to_string()
            });

        let soroban_admin_secret_key = env::var("SOROBAN_ADMIN_SECRET_KEY").unwrap_or_default();
        let soroban_service_secret_key = env::var("SOROBAN_SERVICE_SECRET_KEY").unwrap_or_default();
        let soroban_buyer_secret_key = env::var("SOROBAN_BUYER_SECRET_KEY").unwrap_or_default();
        let soroban_seller_secret_key = env::var("SOROBAN_SELLER_SECRET_KEY").unwrap_or_default();

        let soroban_max_fee = env::var("SOROBAN_MAX_FEE")
            .unwrap_or_else(|_| "5000000".to_string())
            .parse::<u64>()
            .map_err(|e| format!("Invalid SOROBAN_MAX_FEE configuration: {}", e))?;

        Ok(Self {
            database_url,
            app_host,
            app_port,
            rust_log,
            jwt_secret,
            jwt_expiration_hours,
            stellar_rpc_url,
            stellar_network_passphrase,
            soroban_escrow_contract_id,
            soroban_token_contract_id,
            soroban_admin_secret_key,
            soroban_service_secret_key,
            soroban_buyer_secret_key,
            soroban_seller_secret_key,
            soroban_max_fee,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_port_resolution_priority() {
        let orig_port = env::var("PORT").ok();
        let orig_app_port = env::var("APP_PORT").ok();
        let orig_database_url = env::var("DATABASE_URL").ok();
        let orig_jwt_secret = env::var("JWT_SECRET").ok();

        // Required configuration must be present for from_env() to succeed.
        env::set_var("DATABASE_URL", "postgres://test:test@localhost:5432/test");
        env::set_var("JWT_SECRET", "test_jwt_secret_for_unit_tests_only");

        // 1. PORT takes priority over APP_PORT and default
        env::set_var("PORT", "9000");
        env::set_var("APP_PORT", "7000");
        let config = AppConfig::from_env().expect("Config loading failed");
        assert_eq!(config.app_port, 9000);

        // 2. APP_PORT takes priority when PORT is not set
        env::remove_var("PORT");
        env::set_var("APP_PORT", "7000");
        let config = AppConfig::from_env().expect("Config loading failed");
        assert_eq!(config.app_port, 7000);

        // 3. Default 8000 is used when neither PORT nor APP_PORT is set
        env::remove_var("PORT");
        env::remove_var("APP_PORT");
        let config = AppConfig::from_env().expect("Config loading failed");
        assert_eq!(config.app_port, 8000);

        // Clean up environment variables
        if let Some(val) = orig_port {
            env::set_var("PORT", val);
        } else {
            env::remove_var("PORT");
        }
        if let Some(val) = orig_app_port {
            env::set_var("APP_PORT", val);
        } else {
            env::remove_var("APP_PORT");
        }
        if let Some(val) = orig_database_url {
            env::set_var("DATABASE_URL", val);
        } else {
            env::remove_var("DATABASE_URL");
        }
        if let Some(val) = orig_jwt_secret {
            env::set_var("JWT_SECRET", val);
        } else {
            env::remove_var("JWT_SECRET");
        }
    }
}
