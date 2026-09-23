use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct BootstrapResponse {
    pub success: bool,
    pub message: String,
}
