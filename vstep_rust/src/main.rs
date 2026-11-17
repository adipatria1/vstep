use std::sync::Arc;
use vstep_engine::engine::BotManager;
use vstep_engine::{create_router, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═════════════════════════════════════════════════════════════════════╗");
    println!("║       VolStep S/R Trading System - 100% Native Rust Edition         ║");
    println!("║       Pine Script v6 Math Engine | Dual-Engine Sniping (:59s)       ║");
    println!("║       Dynamic Screener + Auto Leverage Bracket + Axum 0.8           ║");
    println!("╚═════════════════════════════════════════════════════════════════════╝");

    let config_path = if std::path::Path::new("config.json").exists() {
        "config.json".to_string()
    } else if std::path::Path::new("volstep_rust/config.json").exists() {
        "volstep_rust/config.json".to_string()
    } else if let Ok(mut exe) = std::env::current_exe() {
        exe.pop();
        let p = exe.join("config.json");
        if p.exists() {
            p.to_string_lossy().to_string()
        } else {
            exe.pop();
            exe.pop();
            let root_cfg = exe.join("config.json");
            if root_cfg.exists() {
                root_cfg.to_string_lossy().to_string()
            } else {
                "config.json".to_string()
            }
        }
    } else {
        "config.json".to_string()
    };

    println!("-> Using config file: {}", config_path);
    let bot = Arc::new(BotManager::new(&config_path));
    let state = AppState { bot };
    let app = create_router(state);

    let host = "0.0.0.0:5000";
    let listener = tokio::net::TcpListener::bind(host).await?;
    println!("-> Web Server running at http://localhost:5000 (bind 0.0.0.0:5000)");

    axum::serve(listener, app).await?;
    Ok(())
}
