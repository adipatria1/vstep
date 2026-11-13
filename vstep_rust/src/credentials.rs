use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ApiCredentials {
    pub demo_api_key: String,
    pub demo_secret_key: String,
    pub real_api_key: String,
    pub real_secret_key: String,
}

pub fn get_credentials_path() -> PathBuf {
    crate::get_data_dir().join("config").join("credentials.json")
}

pub fn load_credentials() -> ApiCredentials {
    let path = get_credentials_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(creds) = serde_json::from_str::<ApiCredentials>(&content) {
            return creds;
        }
    }
    ApiCredentials::default()
}

pub fn save_credentials(creds: &ApiCredentials) -> Result<(), String> {
    let path = get_credentials_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(creds)
        .map_err(|e| format!("Gagal memformat JSON credentials: {}", e))?;
    fs::write(&path, json)
        .map_err(|e| format!("Gagal menulis file {:?}: {}", path, e))?;
    Ok(())
}
