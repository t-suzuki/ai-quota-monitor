pub mod pkce;
pub mod callback_server;
pub mod codex;
pub mod claude;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Epoch milliseconds when the access token expires
    pub expires_at: Option<i64>,
}
