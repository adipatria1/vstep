pub mod binance_client;
pub mod engine;
pub mod models;
pub mod presets;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use binance_client::BinanceFuturesClient;
use engine::{get_interval_ms, BotManager};
use models::{BalanceSummary, BotConfig, LogEntry, ScreenerItem, TradeRecord};

#[derive(Clone)]
pub struct AppState {
    pub bot: Arc<BotManager>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/api/status", get(get_status))
        .route("/api/bot/start", post(start_bot))
        .route("/api/bot/stop", post(stop_bot))
        .route("/api/presets", get(get_presets))
        .route("/api/leverage_bracket", get(get_leverage_bracket))
        .route("/api/screener_preview", get(screener_preview))
        .route("/api/settings", get(get_settings).post(post_settings))
        .route("/api/test_connection", post(test_connection))
        .route("/api/close_position", post(close_position))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../templates/index.html"))
}

pub async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let interval = state.bot.config.read().await.timeframe.clone();
    let interval_ms = get_interval_ms(&interval);
    let now_ms = state.bot.client.get_server_time_ms().await;

    let engines = state.bot.engines.read().await;
    let positions = state.bot.active_positions.read().await;

    let mut sym_keys: Vec<String> = engines.keys().cloned().collect();
    sym_keys.sort();

    let mut screener_list = Vec::with_capacity(sym_keys.len());
    for sym in sym_keys {
        if let Some(eng) = engines.get(&sym) {
            let expected_open = (now_ms / interval_ms) * interval_ms;
            let expected_close = expected_open + interval_ms;
            let rem_ms = (expected_close - now_ms).max(0);
            let rem_sec = rem_ms / 1000;
            let mins = rem_sec / 60;
            let secs = rem_sec % 60;
            let countdown_str = format!("{:02}:{:02}s", mins, secs);

            screener_list.push(ScreenerItem {
                symbol: sym.clone(),
                live_price: eng.live_close,
                live_price_str: eng.fmt(eng.live_close),
                tick_dir: eng.tick_dir,
                countdown: countdown_str,
                rem_ms,
                confirmed_sr: eng.cur_level,
                confirmed_sr_str: eng.fmt(eng.cur_level),
                confirmed_pct: eng.cur_pct.map(|p| (p * 100.0).round() / 100.0),
                confirmed_dir: eng.cur_dir.clone(),
                next_sr_active: eng.next_sr_active,
                next_sr_dir: eng.next_sr_dir.clone(),
                next_sr_price: eng.next_sr_price,
                next_sr_price_str: eng.fmt(eng.next_sr_price),
                next_sr_pct: eng.next_sr_pct.map(|p| (p * 100.0).round() / 100.0),
                effective_leverage: state.bot.get_effective_leverage(&sym).await,
                position: positions.get(&sym).cloned(),
            });
        }
    }

    let mut balance = BalanceSummary::default();
    if state.bot.is_running.load(Ordering::SeqCst) {
        if let Ok(b) = state.bot.client.get_balance_summary().await {
            balance = b;
        }
    }

    let trades: Vec<TradeRecord> = state
        .bot
        .trade_history
        .read()
        .await
        .iter()
        .take(25)
        .cloned()
        .collect();

    let logs: Vec<LogEntry> = state
        .bot
        .logs
        .read()
        .await
        .iter()
        .rev()
        .take(40)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let server_time = chrono::Local::now().format("%H:%M:%S").to_string();
    let config = state.bot.config.read().await.clone();
    let status_str = state.bot.status.read().await.clone();
    let is_running = state.bot.is_running.load(Ordering::SeqCst);
    let active_pos_map = positions.clone();

    Json(json!({
        "status": status_str,
        "is_running": is_running,
        "server_time": server_time,
        "config": config,
        "balance": balance,
        "screener": screener_list,
        "positions": active_pos_map,
        "trades": trades,
        "logs": logs,
    }))
}

pub async fn start_bot(State(state): State<AppState>) -> impl IntoResponse {
    if state.bot.is_running.load(Ordering::SeqCst) {
        return Json(json!({
            "success": false,
            "message": "Bot sudah berjalan!",
        }));
    }

    state.bot.load_config().await;
    state.bot.clone().start().await;

    Json(json!({
        "success": true,
        "message": "Bot berhasil dimulai di background!",
    }))
}

pub async fn stop_bot(State(state): State<AppState>) -> impl IntoResponse {
    if !state.bot.is_running.load(Ordering::SeqCst) {
        return Json(json!({
            "success": false,
            "message": "Bot sedang tidak berjalan.",
        }));
    }

    state.bot.stop().await;

    Json(json!({
        "success": true,
        "message": "Bot telah dihentikan.",
    }))
}

pub async fn get_presets() -> impl IntoResponse {
    Json(json!({
        "categories": presets::get_categories(),
        "presets": presets::get_all_presets(),
    }))
}

#[derive(Deserialize)]
pub struct LeverageQuery {
    pub symbol: Option<String>,
}

pub async fn get_leverage_bracket(
    State(state): State<AppState>,
    Query(query): Query<LeverageQuery>,
) -> impl IntoResponse {
    let symbol = query
        .symbol
        .unwrap_or_else(|| "BTCUSDT".to_string())
        .to_uppercase();
    let max_lev = state.bot.client.get_max_leverage(&symbol).await;
    Json(json!({
        "success": true,
        "symbol": symbol,
        "max_leverage": max_lev,
    }))
}

pub async fn screener_preview(State(state): State<AppState>) -> impl IntoResponse {
    let symbols = state.bot.resolve_active_symbols().await;
    let cfg = state.bot.config.read().await;
    Json(json!({
        "success": true,
        "mode": cfg.coin_source_mode,
        "max_coins": cfg.max_screener_coins,
        "symbols": symbols,
    }))
}

pub async fn get_settings(State(state): State<AppState>) -> impl IntoResponse {
    state.bot.load_config().await;
    let cfg = state.bot.config.read().await.clone();
    Json(cfg)
}

pub async fn post_settings(
    State(state): State<AppState>,
    Json(payload): Json<BotConfig>,
) -> impl IntoResponse {
    state.bot.save_config(payload).await;
    state
        .bot
        .log("Pengaturan diperbarui dari Web UI.", "INFO")
        .await;
    let cfg = state.bot.config.read().await.clone();
    Json(json!({
        "success": true,
        "message": "Pengaturan berhasil disimpan!",
        "config": cfg,
    }))
}

#[derive(Deserialize)]
pub struct TestConnPayload {
    pub api_key: Option<String>,
    pub secret_key: Option<String>,
    pub use_testnet: Option<bool>,
}

pub async fn test_connection(
    State(state): State<AppState>,
    Json(payload): Json<TestConnPayload>,
) -> impl IntoResponse {
    let cfg = state.bot.config.read().await;
    let api_key = payload
        .api_key
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cfg.api_key.clone());
    let secret_key = payload
        .secret_key
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cfg.secret_key.clone());
    let use_testnet = payload.use_testnet.unwrap_or(cfg.use_testnet);
    drop(cfg);

    let test_client = BinanceFuturesClient::new(api_key, secret_key, use_testnet);
    match test_client.get_balance_summary().await {
        Ok(bal) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "message": format!("Koneksi Berhasil! Wallet Balance: {:.2} USDT", bal.wallet_balance),
                "balance": bal,
            })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "message": format!("Koneksi Gagal: {}", e),
            })),
        ),
    }
}

#[derive(Deserialize)]
pub struct ClosePositionPayload {
    pub symbol: Option<String>,
}

pub async fn close_position(
    State(state): State<AppState>,
    Json(payload): Json<ClosePositionPayload>,
) -> impl IntoResponse {
    let symbol = payload.symbol.unwrap_or_default().trim().to_uppercase();
    if symbol.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "message": "Symbol tidak boleh kosong."})),
        );
    }

    let has_pos = state
        .bot
        .active_positions
        .read()
        .await
        .contains_key(&symbol);
    if !has_pos {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"success": false, "message": "Posisi tidak ditemukan."})),
        );
    }

    let bot = state.bot.clone();
    let sym = symbol.clone();
    tokio::spawn(async move {
        bot.close_active_position(&sym, "MANUAL WEB CLOSE").await;
    });

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "message": format!("Perintah tutup posisi {} dikirim.", symbol),
        })),
    )
}

// ── ANDROID JNI NATIVE BRIDGE ────────────────────────────────────────────────
static RUNTIME_STARTED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub extern "system" fn Java_com_volstep_terminal_MainActivity_startNativeEngine(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    port: jni::sys::jint,
    data_dir: jni::objects::JString,
    _cache_dir: jni::objects::JString,
) -> jni::sys::jint {
    if RUNTIME_STARTED.swap(true, Ordering::SeqCst) {
        return 1;
    }

    let config_dir = if let Ok(s) = env.get_string(&data_dir) {
        let p: String = s.into();
        p
    } else {
        ".".to_string()
    };

    let p = if port > 0 { port as u16 } else { 5000 };

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to initialize tokio runtime for Android: {}", e);
                return;
            }
        };

        rt.block_on(async move {
            let config_path = format!("{}/config.json", config_dir);
            let bot = Arc::new(BotManager::new(&config_path));
            let state = AppState { bot };
            let app = create_router(state);

            let addr = format!("0.0.0.0:{}", p);
            if let Ok(listener) = tokio::net::TcpListener::bind(&addr).await {
                println!("-> Native Android Rust Engine listening on {}", addr);
                let _ = axum::serve(listener, app).await;
            } else {
                eprintln!("Failed to bind Axum to {}", addr);
            }
        });
    });

    1
}

#[no_mangle]
pub extern "system" fn Java_com_volstep_terminal_MainActivity_isNativeEngineRunning(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) -> jni::sys::jint {
    if RUNTIME_STARTED.load(Ordering::SeqCst) {
        1
    } else {
        0
    }
}
