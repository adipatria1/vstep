use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const CREDS_PATH: &str = "config/credentials.json";

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ApiCredentials {
    pub demo_api_key: String,
    pub demo_secret_key: String,
    pub real_api_key: String,
    pub real_secret_key: String,
}

pub fn get_credentials_path() -> &'static str {
    CREDS_PATH
}

pub fn load_credentials() -> ApiCredentials {
    if let Ok(content) = fs::read_to_string(CREDS_PATH) {
        if let Ok(creds) = serde_json::from_str::<ApiCredentials>(&content) {
            return creds;
        }
    }
    ApiCredentials::default()
}

pub fn save_credentials(creds: &ApiCredentials) -> Result<(), String> {
    if let Some(parent) = Path::new(CREDS_PATH).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(creds)
        .map_err(|e| format!("Gagal memformat JSON credentials: {}", e))?;
    fs::write(CREDS_PATH, json)
        .map_err(|e| format!("Gagal menulis file {}: {}", CREDS_PATH, e))?;
    Ok(())
}
