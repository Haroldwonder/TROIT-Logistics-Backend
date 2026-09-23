use crate::blockchain::SharedBlockchainService;
use crate::config::AppConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};
use uuid::Uuid;

pub mod inspection;
pub mod notification;
pub mod order_history;
pub mod seller;
pub mod subscription;
pub mod wishlist;

use crate::services::storage::SharedStorageService;

/// Application Shared State
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: AppConfig,
    pub blockchain: SharedBlockchainService,
    pub storage: SharedStorageService,
}

/// Platform user roles supported by TROIT Logistics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "user_role", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Buyer,
    Seller,
    Rider,
    FieldAgent,
    Admin,
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::Buyer => write!(f, "buyer"),
            UserRole::Seller => write!(f, "seller"),
            UserRole::Rider => write!(f, "rider"),
            UserRole::FieldAgent => write!(f, "field_agent"),
            UserRole::Admin => write!(f, "admin"),
        }
    }
}

/// Core User entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub full_name: String,
    pub phone_number: Option<String>,
    pub role: UserRole,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// JWT Token Claims Structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub email: String,
    pub role: UserRole,
    pub exp: i64,
    pub iat: i64,
}
