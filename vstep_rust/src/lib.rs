pub mod binance_api;
pub mod bot_runner;
pub mod credentials;
pub mod engine;
pub mod fetcher;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::thread;
use tiny_http::{Header, Method, Response, Server};
use url::Url;

static BASE_DATA_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);
static BASE_CACHE_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);
static ENGINE_STARTED: AtomicBool = AtomicBool::new(false);

pub fn set_data_dir<P: AsRef<Path>>(dir: P) {
    if let Ok(mut guard) = BASE_DATA_DIR.write() {
        *guard = Some(dir.as_ref().to_path_buf());
    }
}

pub fn get_data_dir() -> PathBuf {
    if let Ok(guard) = BASE_DATA_DIR.read() {
        if let Some(ref path) = *guard {
            return path.clone();
        }
    }
    PathBuf::from(".")
}

pub fn set_cache_dir<P: AsRef<Path>>(dir: P) {
    if let Ok(mut guard) = BASE_CACHE_DIR.write() {
        *guard = Some(dir.as_ref().to_path_buf());
    }
}

pub fn get_cache_dir() -> PathBuf {
    if let Ok(guard) = BASE_CACHE_DIR.read() {
        if let Some(ref path) = *guard {
            return path.clone();
        }
    }
    get_data_dir().join("cache")
}

use binance_api::{
    fetch_top_screened_coins, market_close_position, test_testnet_account,
};
use bot_runner::{BotConfig, SharedBotState};
use credentials::{load_credentials, save_credentials, ApiCredentials};
use engine::{get_all_strategy_keys, run_backtest};
use fetcher::{fetch_binance_fapi_klines, get_cache_path, load_cached_data, CandleData, FetchProgress};

const HOME_HTML: &str = include_str!("../templates/home.html");
const BACKTEST_HTML: &str = include_str!("../templates/backtest.html");
const DEMO_HTML: &str = include_str!("../templates/demo.html");
const SETTINGS_HTML: &str = include_str!("../templates/settings.html");
const PLACEHOLDER_HTML: &str = include_str!("../templates/placeholder.html");
const STYLE_CSS: &str = include_str!("../static/css/style.css");
const MAIN_JS: &str = include_str!("../static/js/main.js");

#[derive(Clone)]
struct AppState {
    last_data: Arc<Mutex<Option<CandleData>>>,
    bot: SharedBotState,
}

pub fn start_server(port: u16) {
    let addr = format!("0.0.0.0:{}", port);

    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("HTTP server info di {}: {}", addr, e);
            return;
        }
    };

    println!("============================================================");
    println!("⚡ VolStep Finder (Embedded Rust Engine) aktif di http://{}", addr);
    println!("   Akses di browser / app: http://localhost:{}", port);
    println!("============================================================");

    let state = AppState {
        last_data: Arc::new(Mutex::new(None)),
        bot: SharedBotState::new(),
    };

    for request in server.incoming_requests() {
        let state = state.clone();
        thread::spawn(move || {
            handle_request(request, state);
        });
    }
}


fn handle_request(mut request: tiny_http::Request, state: AppState) {
    let raw_url = format!("http://localhost{}", request.url());
    let parsed_url = match Url::parse(&raw_url) {
        Ok(u) => u,
        Err(_) => {
            let _ = request.respond(Response::from_string("Bad Request").with_status_code(400));
            return;
        }
    };

    let path = parsed_url.path();
    let method = request.method().clone();

    match (&method, path) {
        // Halaman Utama: 4 Kotak seperti Aplikasi HP
        (&Method::Get, "/") => {
            let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let response = Response::from_string(HOME_HTML).with_header(header);
            let _ = request.respond(response);
        }

        // Halaman Backtest Strategy Finder
        (&Method::Get, "/backtest") => {
            let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let response = Response::from_string(BACKTEST_HTML).with_header(header);
            let _ = request.respond(response);
        }

        // Halaman 2: Demo Binance (Paper Trading Testnet Bot)
        (&Method::Get, "/demo") => {
            let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let response = Response::from_string(DEMO_HTML).with_header(header);
            let _ = request.respond(response);
        }

        // Halaman Kosong: 3. Real Akun Binance
        (&Method::Get, "/real") => {
            let html = PLACEHOLDER_HTML
                .replace("{{ title }}", "3. Real Akun Binance")
                .replace("{{ icon }}", "💎")
                .replace("{{ description }}", "Modul eksekusi trading otomatis di akun riil Binance Futures USDT-M dengan margin protection.");
            let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let response = Response::from_string(html).with_header(header);
            let _ = request.respond(response);
        }

        // Halaman 4: Setting API Binance
        (&Method::Get, "/settings") => {
            let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let response = Response::from_string(SETTINGS_HTML).with_header(header);
            let _ = request.respond(response);
        }

        // Static CSS
        (&Method::Get, "/static/css/style.css") => {
            let header = Header::from_bytes(&b"Content-Type"[..], &b"text/css; charset=utf-8"[..]).unwrap();
            let response = Response::from_string(STYLE_CSS).with_header(header);
            let _ = request.respond(response);
        }

        // Static JS
        (&Method::Get, "/static/js/main.js") => {
            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/javascript; charset=utf-8"[..]).unwrap();
            let response = Response::from_string(MAIN_JS).with_header(header);
            let _ = request.respond(response);
        }

        // PWA Web App Manifest (Untuk instalasi instan di HP Android tanpa APK)
        (&Method::Get, "/manifest.json") => {
            let manifest = serde_json::json!({
                "name": "VolStep Futures Terminal",
                "short_name": "VolStep",
                "start_url": "/",
                "display": "standalone",
                "background_color": "#0b0e14",
                "theme_color": "#12161f",
                "orientation": "portrait-primary",
                "icons": [
                    {
                        "src": "/static/img/icon.png",
                        "sizes": "192x192",
                        "type": "image/png"
                    }
                ]
            });
            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let response = Response::from_string(manifest.to_string()).with_header(header);
            let _ = request.respond(response);
        }

        // API: Strategy Keys
        (&Method::Get, "/api/strategy_keys") => {
            let keys = get_all_strategy_keys();
            let json = serde_json::json!({
                "keys": keys,
                "default": "Top #1 Sesuai Filter Leaderboard (Otomatis)"
            });
            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let response = Response::from_string(json.to_string()).with_header(header);
            let _ = request.respond(response);
        }

        // API: Check Cache
        (&Method::Get, "/api/check_cache") => {
            let query_map: HashMap<_, _> = parsed_url.query_pairs().into_owned().collect();
            let symbol = query_map.get("symbol").cloned().unwrap_or_else(|| "BTCUSDT".into());
            let interval = query_map.get("interval").cloned().unwrap_or_else(|| "1h".into());
            let days: u32 = query_map.get("days").and_then(|s| s.parse().ok()).unwrap_or(7);

            let path = get_cache_path(&symbol, &interval, days);
            let exists = path.exists();
            let size_kb = if exists {
                let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                (bytes as f64 / 1024.0 * 10.0).round() / 10.0
            } else {
                0.0
            };

            let json = serde_json::json!({
                "cached": exists,
                "size_kb": size_kb,
                "symbol": symbol.to_uppercase(),
                "interval": interval,
                "days": days
            });
            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let response = Response::from_string(json.to_string()).with_header(header);
            let _ = request.respond(response);
        }

        // API: SSE Stream Backtest
        (&Method::Get, "/api/stream_backtest") => {
            handle_stream_backtest(request, parsed_url, state);
        }

        // API: Recalculate from Cache / Memory
        (&Method::Post, "/api/recalculate") => {
            handle_recalculate(request, state);
        }

        // ========================================================
        // API SETTINGS & CREDENTIALS
        // ========================================================
        (&Method::Get, "/api/settings/get") => {
            let creds = load_credentials();
            let json = serde_json::json!({
                "demo_api_key": creds.demo_api_key,
                "demo_secret_key": creds.demo_secret_key,
                "real_api_key": creds.real_api_key,
                "real_secret_key": creds.real_secret_key,
            });
            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let _ = request.respond(Response::from_string(json.to_string()).with_header(header));
        }

        (&Method::Post, "/api/settings/save") => {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            if let Ok(creds) = serde_json::from_str::<ApiCredentials>(&body) {
                if let Err(e) = save_credentials(&creds) {
                    let err = serde_json::json!({"status": "error", "message": e});
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(err.to_string()).with_header(header));
                    return;
                }
                let ok = serde_json::json!({"status": "ok", "message": "Kredensial berhasil disimpan."});
                let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                let _ = request.respond(Response::from_string(ok.to_string()).with_header(header));
            } else {
                let err = serde_json::json!({"status": "error", "message": "Format JSON tidak valid"});
                let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                let _ = request.respond(Response::from_string(err.to_string()).with_header(header));
            }
        }

        (&Method::Post, "/api/settings/test_demo") => {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let val: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            let key = val["api_key"].as_str().unwrap_or("");
            let sec = val["secret_key"].as_str().unwrap_or("");

            match test_testnet_account(key, sec) {
                Ok(info) => {
                    let res = serde_json::json!({
                        "status": "ok",
                        "account": info
                    });
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(res.to_string()).with_header(header));
                }
                Err(e) => {
                    let res = serde_json::json!({
                        "status": "error",
                        "message": e
                    });
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(res.to_string()).with_header(header));
                }
            }
        }

        // ========================================================
        // API CACHE MANAGEMENT
        // ========================================================
        (&Method::Get, "/api/cache/info") => {
            let dir = get_cache_dir();
            let mut count = 0;
            let mut total_bytes: u64 = 0;
            if dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        if let Ok(meta) = entry.metadata() {
                            if meta.is_file() {
                                count += 1;
                                total_bytes += meta.len();
                            }
                        }
                    }
                }
            }
            let mb = (total_bytes as f64) / (1024.0 * 1024.0);
            let res = serde_json::json!({
                "status": "ok",
                "count": count,
                "total_bytes": total_bytes,
                "size_formatted": format!("{:.2} MB", mb)
            });
            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let _ = request.respond(Response::from_string(res.to_string()).with_header(header));
        }

        (&Method::Post, "/api/cache/clear") => {
            let dir = get_cache_dir();
            let mut deleted_count = 0;
            if dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        if let Ok(meta) = entry.metadata() {
                            if meta.is_file() {
                                if std::fs::remove_file(entry.path()).is_ok() {
                                    deleted_count += 1;
                                }
                            }
                        }
                    }
                }
            }
            let res = serde_json::json!({
                "status": "ok",
                "message": format!("{} file cache kline berhasil dibersihkan", deleted_count),
                "deleted_count": deleted_count
            });
            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let _ = request.respond(Response::from_string(res.to_string()).with_header(header));
        }

        // ========================================================
        // API DEMO BOT CONTROLLER
        // ========================================================
        (&Method::Get, "/api/demo/status") => {
            let lock = state.bot.0.lock().unwrap();
            let res = serde_json::json!({
                "is_running": lock.config.is_running,
                "config": lock.config,
                "balance": lock.cached_balance,
                "positions": lock.cached_positions,
                "logs": lock.logs,
                "active_trade": lock.active_trade,
                "current_sr": lock.current_sr,
                "current_vol": lock.current_vol,
                "strategy": lock.last_selected_strategy,
                "warmup_symbol": lock.last_warmup_symbol,
            });
            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let _ = request.respond(Response::from_string(res.to_string()).with_header(header));
        }

        (&Method::Post, "/api/demo/start") => {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            match serde_json::from_str::<BotConfig>(&body) {
                Ok(cfg) => {
                    match state.bot.start(cfg) {
                        Ok(_) => {
                            let ok = serde_json::json!({"status": "ok", "message": "Bot Demo berhasil dijalankan."});
                            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                            let _ = request.respond(Response::from_string(ok.to_string()).with_header(header));
                        }
                        Err(e) => {
                            let err = serde_json::json!({"status": "error", "message": e});
                            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                            let _ = request.respond(Response::from_string(err.to_string()).with_header(header));
                        }
                    }
                }
                Err(e) => {
                    let err = serde_json::json!({"status": "error", "message": format!("Invalid JSON body: {}", e)});
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(err.to_string()).with_header(header));
                }
            }
        }

        (&Method::Post, "/api/demo/stop") => {
            state.bot.stop();
            let ok = serde_json::json!({"status": "ok", "message": "Bot Demo dihentikan."});
            let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let _ = request.respond(Response::from_string(ok.to_string()).with_header(header));
        }

        (&Method::Post, "/api/demo/panic_close") => {
            match state.bot.panic_close_all() {
                Ok(n) => {
                    let ok = serde_json::json!({"status": "ok", "message": format!("Panic Close selesai: {} posisi ditutup.", n)});
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(ok.to_string()).with_header(header));
                }
                Err(e) => {
                    let err = serde_json::json!({"status": "error", "message": e});
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(err.to_string()).with_header(header));
                }
            }
        }

        (&Method::Post, "/api/demo/close_position") => {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let val: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            let sym = val["symbol"].as_str().unwrap_or("");
            let side = val["side"].as_str().unwrap_or("");
            let qty = val["qty"].as_f64().unwrap_or(0.0);

            let creds = load_credentials();
            match market_close_position(sym, side, qty, &creds.demo_api_key, &creds.demo_secret_key) {
                Ok(msg) => {
                    {
                        let mut lock = state.bot.0.lock().unwrap();
                        if let Some(ref trade) = lock.active_trade {
                            if trade.symbol == sym {
                                lock.active_trade = None;
                            }
                        }
                    }
                    let ok = serde_json::json!({"status": "ok", "message": msg});
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(ok.to_string()).with_header(header));
                }
                Err(e) => {
                    let err = serde_json::json!({"status": "error", "message": e});
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(err.to_string()).with_header(header));
                }
            }
        }

        (&Method::Get, "/api/demo/screened_coins") => {
            let query_map: HashMap<_, _> = parsed_url.query_pairs().into_owned().collect();
            let sort_by = query_map.get("sort_by").cloned().unwrap_or_else(|| "top_gainers".into());
            let limit: usize = query_map.get("limit").and_then(|s| s.parse().ok()).unwrap_or(5);

            match fetch_top_screened_coins(limit, &sort_by) {
                Ok(coins) => {
                    let res = serde_json::json!({"status": "ok", "coins": coins});
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(res.to_string()).with_header(header));
                }
                Err(e) => {
                    let err = serde_json::json!({"status": "error", "message": e});
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(err.to_string()).with_header(header));
                }
            }
        }

        _ => {
            let response = Response::from_string("404 Not Found").with_status_code(404);
            let _ = request.respond(response);
        }
    }
}

fn handle_stream_backtest(request: tiny_http::Request, parsed_url: Url, state: AppState) {
    let query_map: HashMap<_, _> = parsed_url.query_pairs().into_owned().collect();

    let symbol = query_map.get("symbol").cloned().unwrap_or_else(|| "BTCUSDT".into());
    let interval = query_map.get("interval").cloned().unwrap_or_else(|| "1h".into());
    let days: u32 = query_map.get("days").and_then(|s| s.parse().ok()).unwrap_or(7);
    let force_fetch = query_map.get("force_fetch").map(|v| v.to_lowercase() == "true").unwrap_or(false);

    let line_mode = query_map.get("lineMode").cloned().unwrap_or_else(|| "Anchor".into());
    let vol_type = query_map.get("volatilityType").cloned().unwrap_or_else(|| "Stdev".into());
    let vol_len: usize = query_map.get("volatilityLength").and_then(|s| s.parse().ok()).unwrap_or(200);
    let vol_mult: f64 = query_map.get("volMult").and_then(|s| s.parse().ok()).unwrap_or(4.0);
    let realtime_update = query_map.get("realtimeUpdate").map(|v| v.to_lowercase() != "false").unwrap_or(true);

    let init_cap: f64 = query_map.get("initCap").and_then(|s| s.parse().ok()).unwrap_or(1000.0);
    let margin_pct: f64 = query_map.get("marginPct").and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let margin_basis = query_map.get("marginBasis").cloned().unwrap_or_else(|| "Current Balance".into());
    let leverage: i32 = query_map.get("leverage").and_then(|s| s.parse().ok()).unwrap_or(10);
    let fee_entry: f64 = query_map.get("feeEntry").and_then(|s| s.parse().ok()).unwrap_or(0.05);
    let fee_exit: f64 = query_map.get("feeExit").and_then(|s| s.parse().ok()).unwrap_or(0.05);
    let mmr_pct: f64 = query_map.get("mmrPct").and_then(|s| s.parse().ok()).unwrap_or(0.4);
    let one_way_rule = query_map.get("oneWayRule").cloned().unwrap_or_else(|| "Tutup Posisi Lama & Buka Baru di SR Baru".into());
    let max_open: usize = query_map.get("maxOpen").and_then(|s| s.parse().ok()).unwrap_or(20);
    let bt_days: i32 = query_map.get("btDays").and_then(|s| s.parse().ok()).unwrap_or(0);

    let inc_hedge = query_map.get("incHedge").map(|v| v.to_lowercase() == "true").unwrap_or(false);
    let inc_trend = query_map.get("incTrend").map(|v| v.to_lowercase() != "false").unwrap_or(true);
    let inc_counter = query_map.get("incCounter").map(|v| v.to_lowercase() != "false").unwrap_or(true);
    let inc_bar_tp = query_map.get("incBarTp").map(|v| v.to_lowercase() != "false").unwrap_or(true);
    let inc_bar_sl = query_map.get("incBarSl").map(|v| v.to_lowercase() != "false").unwrap_or(true);

    let fixed1: f64 = query_map.get("fixed1").and_then(|s| s.parse().ok()).unwrap_or(0.3);
    let fixed2: f64 = query_map.get("fixed2").and_then(|s| s.parse().ok()).unwrap_or(0.6);
    let fixed3: f64 = query_map.get("fixed3").and_then(|s| s.parse().ok()).unwrap_or(1.0);

    let bar_tp1: i32 = query_map.get("barTp1").and_then(|s| s.parse().ok()).unwrap_or(3);
    let bar_tp2: i32 = query_map.get("barTp2").and_then(|s| s.parse().ok()).unwrap_or(6);
    let bar_tp_dir: i32 = query_map.get("barTpDir").and_then(|s| s.parse().ok()).unwrap_or(3);

    let sl_bar1: i32 = query_map.get("slBar1").and_then(|s| s.parse().ok()).unwrap_or(4);
    let sl_bar2: i32 = query_map.get("slBar2").and_then(|s| s.parse().ok()).unwrap_or(8);
    let sl_bar_adv: i32 = query_map.get("slBarAdv").and_then(|s| s.parse().ok()).unwrap_or(3);

    let min_levels: usize = query_map.get("minLevels").and_then(|s| s.parse().ok()).unwrap_or(10);
    let rank_by = query_map.get("rankBy").cloned().unwrap_or_else(|| "Net / Max DD".into());
    let strat_filter = query_map.get("stratFilter").cloned().unwrap_or_else(|| "Semua Strategi".into());
    let top_n: usize = query_map.get("topN").and_then(|s| s.parse().ok()).unwrap_or(15);
    let sel_strategy = query_map.get("selStrategy").cloned().unwrap_or_else(|| "Top #1 Sesuai Filter Leaderboard (Otomatis)".into());

    let (tx, rx) = mpsc::channel::<String>();

    thread::spawn(move || {
        let progress_tx = tx.clone();
        let progress_cb = move |p: FetchProgress| {
            let json = serde_json::json!({
                "type": "progress",
                "data": p
            });
            let _ = progress_tx.send(format!("data: {}\n\n", json));
        };

        match fetch_binance_fapi_klines(&symbol, &interval, days, !force_fetch, progress_cb) {
            Ok(data) => {
                let _ = tx.send(format!("data: {}\n\n", serde_json::json!({
                    "type": "progress",
                    "data": {
                        "status": "simulating",
                        "message": format!("Menjalankan simulasi 105 strategi Rust: {} candles...", data.times.len()),
                        "percent": 98.0
                    }
                })));

                // Save to state for fast recalculate
                *state.last_data.lock().unwrap() = Some(data.clone());

                let result = run_backtest(
                    &data.times,
                    &data.open,
                    &data.high,
                    &data.low,
                    &data.close,
                    &line_mode,
                    &vol_type,
                    vol_len,
                    vol_mult,
                    realtime_update,
                    init_cap,
                    margin_pct,
                    &margin_basis,
                    leverage,
                    fee_entry,
                    fee_exit,
                    mmr_pct,
                    &one_way_rule,
                    max_open,
                    bt_days,
                    inc_hedge,
                    inc_trend,
                    inc_counter,
                    inc_bar_tp,
                    inc_bar_sl,
                    fixed1,
                    fixed2,
                    fixed3,
                    bar_tp1,
                    bar_tp2,
                    bar_tp_dir,
                    sl_bar1,
                    sl_bar2,
                    sl_bar_adv,
                    min_levels,
                    &rank_by,
                    &strat_filter,
                    top_n,
                    &sel_strategy,
                );

                let _ = tx.send(format!("data: {}\n\n", serde_json::json!({
                    "type": "complete",
                    "result": result
                })));
            }
            Err(e) => {
                let _ = tx.send(format!("data: {}\n\n", serde_json::json!({
                    "type": "error",
                    "message": e
                })));
            }
        }
    });

    // Stream SSE to client
    let mut writer = request.into_writer();
    let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
    if writer.write_all(response_headers.as_bytes()).is_ok() {
        let _ = writer.flush();
        while let Ok(msg) = rx.recv() {
            if writer.write_all(msg.as_bytes()).is_err() {
                break;
            }
            let _ = writer.flush();
            if msg.contains(r#""type": "complete""#) || msg.contains(r#""type": "error""#) {
                break;
            }
        }
    }
}

fn handle_recalculate(mut request: tiny_http::Request, state: AppState) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        let _ = request.respond(Response::from_string(r#"{"error":"Bad body"}"#).with_status_code(400));
        return;
    }

    let val: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            let _ = request.respond(Response::from_string(r#"{"error":"Invalid JSON"}"#).with_status_code(400));
            return;
        }
    };

    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("BTCUSDT");
    let interval = val.get("interval").and_then(|v| v.as_str()).unwrap_or("1h");
    let days = val.get("days").and_then(|v| v.as_u64()).unwrap_or(7) as u32;

    let data_opt = {
        let lock = state.last_data.lock().unwrap();
        if let Some(ref d) = *lock {
            if d.symbol.eq_ignore_ascii_case(symbol) && d.interval == interval && d.days == days {
                Some(d.clone())
            } else {
                None
            }
        } else {
            None
        }
    };

    let data = match data_opt {
        Some(d) => d,
        None => match load_cached_data(symbol, interval, days) {
            Some(d) => {
                *state.last_data.lock().unwrap() = Some(d.clone());
                d
            }
            None => {
                let err = serde_json::json!({
                    "error": format!("Data untuk {} {} {} hari belum ada di cache. Silakan klik 'Fetch & Jalankan'.", symbol, interval, days)
                });
                let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                let _ = request.respond(Response::from_string(err.to_string()).with_status_code(400).with_header(header));
                return;
            }
        },
    };

    let line_mode = val.get("lineMode").and_then(|v| v.as_str()).unwrap_or("Anchor");
    let vol_type = val.get("volatilityType").and_then(|v| v.as_str()).unwrap_or("Stdev");
    let vol_len = val.get("volatilityLength").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
    let vol_mult = val.get("volMult").and_then(|v| v.as_f64()).unwrap_or(4.0);
    let realtime_update = val.get("realtimeUpdate").and_then(|v| v.as_bool()).unwrap_or(true);

    let init_cap = val.get("initCap").and_then(|v| v.as_f64()).unwrap_or(1000.0);
    let margin_pct = val.get("marginPct").and_then(|v| v.as_f64()).unwrap_or(5.0);
    let margin_basis = val.get("marginBasis").and_then(|v| v.as_str()).unwrap_or("Current Balance");
    let leverage = val.get("leverage").and_then(|v| v.as_i64()).unwrap_or(10) as i32;
    let fee_entry = val.get("feeEntry").and_then(|v| v.as_f64()).unwrap_or(0.05);
    let fee_exit = val.get("feeExit").and_then(|v| v.as_f64()).unwrap_or(0.05);
    let mmr_pct = val.get("mmrPct").and_then(|v| v.as_f64()).unwrap_or(0.4);
    let one_way_rule = val.get("oneWayRule").and_then(|v| v.as_str()).unwrap_or("Tutup Posisi Lama & Buka Baru di SR Baru");
    let max_open = val.get("maxOpen").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let bt_days = val.get("btDays").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

    let inc_hedge = val.get("incHedge").and_then(|v| v.as_bool()).unwrap_or(false);
    let inc_trend = val.get("incTrend").and_then(|v| v.as_bool()).unwrap_or(true);
    let inc_counter = val.get("incCounter").and_then(|v| v.as_bool()).unwrap_or(true);
    let inc_bar_tp = val.get("incBarTp").and_then(|v| v.as_bool()).unwrap_or(true);
    let inc_bar_sl = val.get("incBarSl").and_then(|v| v.as_bool()).unwrap_or(true);

    let fixed1 = val.get("fixed1").and_then(|v| v.as_f64()).unwrap_or(0.3);
    let fixed2 = val.get("fixed2").and_then(|v| v.as_f64()).unwrap_or(0.6);
    let fixed3 = val.get("fixed3").and_then(|v| v.as_f64()).unwrap_or(1.0);

    let bar_tp1 = val.get("barTp1").and_then(|v| v.as_i64()).unwrap_or(3) as i32;
    let bar_tp2 = val.get("barTp2").and_then(|v| v.as_i64()).unwrap_or(6) as i32;
    let bar_tp_dir = val.get("barTpDir").and_then(|v| v.as_i64()).unwrap_or(3) as i32;

    let sl_bar1 = val.get("slBar1").and_then(|v| v.as_i64()).unwrap_or(4) as i32;
    let sl_bar2 = val.get("slBar2").and_then(|v| v.as_i64()).unwrap_or(8) as i32;
    let sl_bar_adv = val.get("slBarAdv").and_then(|v| v.as_i64()).unwrap_or(3) as i32;

    let min_levels = val.get("minLevels").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let rank_by = val.get("rankBy").and_then(|v| v.as_str()).unwrap_or("Net / Max DD");
    let strat_filter = val.get("stratFilter").and_then(|v| v.as_str()).unwrap_or("Semua Strategi");
    let top_n = val.get("topN").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
    let sel_strategy = val.get("selStrategy").and_then(|v| v.as_str()).unwrap_or("Top #1 Sesuai Filter Leaderboard (Otomatis)");

    let result = run_backtest(
        &data.times,
        &data.open,
        &data.high,
        &data.low,
        &data.close,
        line_mode,
        vol_type,
        vol_len,
        vol_mult,
        realtime_update,
        init_cap,
        margin_pct,
        margin_basis,
        leverage,
        fee_entry,
        fee_exit,
        mmr_pct,
        one_way_rule,
        max_open,
        bt_days,
        inc_hedge,
        inc_trend,
        inc_counter,
        inc_bar_tp,
        inc_bar_sl,
        fixed1,
        fixed2,
        fixed3,
        bar_tp1,
        bar_tp2,
        bar_tp_dir,
        sl_bar1,
        sl_bar2,
        sl_bar_adv,
        min_levels,
        rank_by,
        strat_filter,
        top_n,
        sel_strategy,
    );

    let res_json = serde_json::json!({
        "status": "ok",
        "result": result
    });

    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    let response = Response::from_string(res_json.to_string()).with_header(header);
    let _ = request.respond(response);
}

#[no_mangle]
pub extern "system" fn Java_com_volstep_terminal_MainActivity_startNativeEngine(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    port: jni::sys::jint,
    data_dir: jni::objects::JString,
    cache_dir: jni::objects::JString,
) -> jni::sys::jint {
    if let Ok(s) = env.get_string(&data_dir) {
        let path_str: String = s.into();
        if !path_str.trim().is_empty() {
            set_data_dir(path_str);
        }
    }
    if let Ok(s) = env.get_string(&cache_dir) {
        let path_str: String = s.into();
        if !path_str.trim().is_empty() {
            set_cache_dir(path_str);
        }
    }

    if ENGINE_STARTED.swap(true, Ordering::SeqCst) {
        return 1;
    }

    let p = if port > 0 { port as u16 } else { 5050 };
    thread::spawn(move || {
        start_server(p);
    });

    1
}

#[no_mangle]
pub extern "system" fn Java_com_volstep_terminal_MainActivity_isNativeEngineRunning(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) -> jni::sys::jint {
    if ENGINE_STARTED.load(Ordering::SeqCst) { 1 } else { 0 }
}
