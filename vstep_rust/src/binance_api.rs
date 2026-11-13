use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

pub const TESTNET_BASE_URL: &str = "https://testnet.binancefuture.com";
pub const REAL_BASE_URL: &str = "https://fapi.binance.com";

pub fn sign_query(query: &str, secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key valid");
    mac.update(query.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn get_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountBalanceInfo {
    pub wallet_balance: f64,
    pub unrealized_pnl: f64,
    pub equity: f64,
    pub available_balance: f64,
    pub margin_ratio: f64,
    pub update_time: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LivePositionInfo {
    pub symbol: String,
    pub side: String, // "BUY" / "SELL"
    pub position_amt: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_profit: f64,
    pub leverage: i32,
    pub margin_used: f64,
    pub pnl_percent: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoinTickerInfo {
    pub symbol: String,
    pub last_price: f64,
    pub price_change_percent: f64,
    pub quote_volume: f64,
}

/// Test koneksi ke Binance Futures Testnet menggunakan API Key & Secret Key
pub fn test_testnet_account(api_key: &str, secret_key: &str) -> Result<AccountBalanceInfo, String> {
    if api_key.trim().is_empty() || secret_key.trim().is_empty() {
        return Err("API Key dan Secret Key tidak boleh kosong.".to_string());
    }

    let ts = get_timestamp_ms();
    let query = format!("timestamp={}&recvWindow=5000", ts);
    let signature = sign_query(&query, secret_key.trim());
    let url = format!("{}/fapi/v2/account?{}&signature={}", TESTNET_BASE_URL, query, signature);

    let res = ureq::get(&url)
        .set("X-MBX-APIKEY", api_key.trim())
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_default();
                format!("Binance API Error (Status {}): {}", code, body)
            }
            ureq::Error::Transport(t) => format!("Koneksi ke Binance Testnet gagal: {}", t),
        })?;

    let val: serde_json::Value = res
        .into_json()
        .map_err(|e| format!("Gagal membaca JSON respon akun: {}", e))?;

    let wallet_bal: f64 = val["totalWalletBalance"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let un_pnl: f64 = val["totalUnrealizedProfit"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let margin_bal: f64 = val["totalMarginBalance"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let avail_bal: f64 = val["availableBalance"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let maint_mrg: f64 = val["totalMaintMargin"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    let m_ratio = if margin_bal > 0.0 {
        (maint_mrg / margin_bal * 100.0 * 100.0).round() / 100.0
    } else {
        0.0
    };

    Ok(AccountBalanceInfo {
        wallet_balance: (wallet_bal * 100.0).round() / 100.0,
        unrealized_pnl: (un_pnl * 100.0).round() / 100.0,
        equity: (margin_bal * 100.0).round() / 100.0,
        available_balance: (avail_bal * 100.0).round() / 100.0,
        margin_ratio: m_ratio,
        update_time: Utc::now().format("%H:%M:%S UTC").to_string(),
    })
}

/// Ambil Max Leverage suatu koin dari endpoint /fapi/v1/leverageBracket
pub fn get_max_leverage(symbol: &str, api_key: &str, secret_key: &str) -> Result<i32, String> {
    let sym = symbol.to_uppercase();
    let ts = get_timestamp_ms();
    let query = format!("symbol={}&timestamp={}&recvWindow=5000", sym, ts);
    let signature = sign_query(&query, secret_key.trim());
    let url = format!("{}/fapi/v1/leverageBracket?{}&signature={}", TESTNET_BASE_URL, query, signature);

    let res = ureq::get(&url)
        .set("X-MBX-APIKEY", api_key.trim())
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| format!("Gagal memanggil leverageBracket untuk {}: {}", sym, e))?;

    let val: serde_json::Value = res.into_json().map_err(|e| format!("Format JSON error: {}", e))?;

    if let Some(arr) = val.as_array() {
        if let Some(first_obj) = arr.first() {
            if let Some(brackets) = first_obj["brackets"].as_array() {
                if let Some(b1) = brackets.first() {
                    if let Some(lev) = b1["initialLeverage"].as_i64() {
                        return Ok(lev as i32);
                    }
                }
            }
        }
    }

    Ok(20) // Default fallback aman
}

/// Set Leverage untuk koin di Binance Testnet
pub fn set_symbol_leverage(symbol: &str, leverage: i32, api_key: &str, secret_key: &str) -> Result<i32, String> {
    let sym = symbol.to_uppercase();
    let ts = get_timestamp_ms();
    let query = format!("symbol={}&leverage={}&timestamp={}&recvWindow=5000", sym, leverage, ts);
    let signature = sign_query(&query, secret_key.trim());
    let url = format!("{}/fapi/v1/leverage?{}&signature={}", TESTNET_BASE_URL, query, signature);

    let res = ureq::post(&url)
        .set("X-MBX-APIKEY", api_key.trim())
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| format!("Gagal mengatur leverage {}x untuk {}: {}", leverage, sym, e))?;

    let val: serde_json::Value = res.into_json().map_err(|e| format!("Format JSON error: {}", e))?;
    let applied = val["leverage"].as_i64().unwrap_or(leverage as i64) as i32;
    Ok(applied)
}

/// Mengambil daftar posisi yang sedang terbuka di akun Binance Testnet
pub fn get_live_positions(api_key: &str, secret_key: &str) -> Result<Vec<LivePositionInfo>, String> {
    let ts = get_timestamp_ms();
    let query = format!("timestamp={}&recvWindow=5000", ts);
    let signature = sign_query(&query, secret_key.trim());
    let url = format!("{}/fapi/v2/positionRisk?{}&signature={}", TESTNET_BASE_URL, query, signature);

    let res = ureq::get(&url)
        .set("X-MBX-APIKEY", api_key.trim())
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| format!("Gagal mengambil posisi aktif: {}", e))?;

    let arr: Vec<serde_json::Value> = res.into_json().map_err(|e| format!("Format JSON error: {}", e))?;
    let mut positions = Vec::new();

    for item in arr {
        let amt: f64 = item["positionAmt"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        if amt.abs() > 0.000001 {
            let symbol = item["symbol"].as_str().unwrap_or("").to_string();
            let entry_px: f64 = item["entryPrice"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let mark_px: f64 = item["markPrice"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let un_pnl: f64 = item["unRealizedProfit"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let lev: i32 = item["leverage"].as_str().and_then(|s| s.parse().ok()).unwrap_or(10);
            let notional = (amt.abs()) * mark_px;
            let margin_used = if lev > 0 { notional / (lev as f64) } else { 0.0 };
            let pnl_pct = if margin_used > 0.0 { un_pnl / margin_used * 100.0 } else { 0.0 };

            positions.push(LivePositionInfo {
                symbol,
                side: if amt > 0.0 { "BUY".to_string() } else { "SELL".to_string() },
                position_amt: amt.abs(),
                entry_price: entry_px,
                mark_price: mark_px,
                unrealized_profit: (un_pnl * 100.0).round() / 100.0,
                leverage: lev,
                margin_used: (margin_used * 100.0).round() / 100.0,
                pnl_percent: (pnl_pct * 100.0).round() / 100.0,
            });
        }
    }

    Ok(positions)
}

/// Auto-Screener Publik: Mengambil Top Gainers atau Top Volume dari Binance Futures
pub fn fetch_top_screened_coins(limit: usize, sort_by: &str) -> Result<Vec<CoinTickerInfo>, String> {
    let url = format!("{}/fapi/v1/ticker/24hr", REAL_BASE_URL);

    let res = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| format!("Gagal menghubungi ticker 24hr Binance: {}", e))?;

    let arr: Vec<serde_json::Value> = res.into_json().map_err(|e| format!("Format JSON ticker error: {}", e))?;
    let mut tickers: Vec<CoinTickerInfo> = Vec::new();

    for item in arr {
        let sym = item["symbol"].as_str().unwrap_or("");
        // Hanya ambil pair USDT Perpetual standar (misal BTCUSDT, hindari indeks atau USDC)
        if sym.ends_with("USDT") && !sym.contains('_') {
            let last_px: f64 = item["lastPrice"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let chg_pct: f64 = item["priceChangePercent"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let vol: f64 = item["quoteVolume"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);

            tickers.push(CoinTickerInfo {
                symbol: sym.to_string(),
                last_price: last_px,
                price_change_percent: (chg_pct * 100.0).round() / 100.0,
                quote_volume: (vol / 1_000_000.0 * 10.0).round() / 10.0, // dalam Juta USDT
            });
        }
    }

    if sort_by == "top_gainers" {
        tickers.sort_by(|a, b| b.price_change_percent.partial_cmp(&a.price_change_percent).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        // top_volume
        tickers.sort_by(|a, b| b.quote_volume.partial_cmp(&a.quote_volume).unwrap_or(std::cmp::Ordering::Equal));
    }

    tickers.truncate(limit);
    Ok(tickers)
}

use std::collections::HashMap;
use std::sync::Mutex;

static LOT_CACHE: Mutex<Option<HashMap<String, (usize, f64, f64)>>> = Mutex::new(None);

/// Mengambil precision, stepSize, dan minQty untuk symbol dari exchangeInfo Binance
pub fn get_symbol_lot_precision(symbol: &str) -> (usize, f64, f64) {
    let sym = symbol.to_uppercase();
    {
        let mut lock = LOT_CACHE.lock().unwrap();
        if lock.is_none() {
            *lock = Some(HashMap::new());
        }
        if let Some(ref map) = *lock {
            if let Some(entry) = map.get(&sym) {
                return *entry;
            }
        }
    }

    // Default fallback jika network gagal
    let mut precision: usize = 3;
    let mut step_size: f64 = 0.001;
    let mut min_qty: f64 = 0.001;

    let url = format!("{}/fapi/v1/exchangeInfo?symbol={}", REAL_BASE_URL, sym);
    if let Ok(res) = ureq::get(&url).timeout(std::time::Duration::from_secs(5)).call() {
        if let Ok(val) = res.into_json::<serde_json::Value>() {
            if let Some(symbols) = val["symbols"].as_array() {
                if let Some(s) = symbols.first() {
                    if let Some(p) = s["quantityPrecision"].as_u64() {
                        precision = p as usize;
                    }
                    if let Some(filters) = s["filters"].as_array() {
                        for f in filters {
                            if f["filterType"] == "LOT_SIZE" {
                                if let Some(ss) = f["stepSize"].as_str().and_then(|v| v.parse().ok()) {
                                    step_size = ss;
                                }
                                if let Some(mq) = f["minQty"].as_str().and_then(|v| v.parse().ok()) {
                                    min_qty = mq;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    {
        let mut lock = LOT_CACHE.lock().unwrap();
        if let Some(ref mut map) = *lock {
            map.insert(sym, (precision, step_size, min_qty));
        }
    }

    (precision, step_size, min_qty)
}

/// Format quantity sesuai lot size dan precision Binance
pub fn format_quantity(symbol: &str, raw_qty: f64) -> (f64, String) {
    let (precision, step_size, min_qty) = get_symbol_lot_precision(symbol);
    let mut q = raw_qty;
    if q < min_qty {
        q = min_qty;
    }
    if step_size > 0.0 {
        q = (q / step_size).floor() * step_size;
    }
    let formatted = format!("{:.1$}", q, precision);
    let final_qty = formatted.parse::<f64>().unwrap_or(q);
    (final_qty, formatted)
}

/// Eksekusi Buka Posisi secara Market di Binance Testnet
pub fn market_open_position(
    symbol: &str,
    side: &str,
    qty: f64,
    api_key: &str,
    secret_key: &str,
) -> Result<(i64, f64, String), String> {
    let sym = symbol.to_uppercase();
    let order_side = if side.eq_ignore_ascii_case("BUY") { "BUY" } else { "SELL" };
    let (actual_qty, qty_str) = format_quantity(&sym, qty);
    let ts = get_timestamp_ms();

    let query = format!(
        "symbol={}&side={}&type=MARKET&quantity={}&timestamp={}&recvWindow=5000",
        sym, order_side, qty_str, ts
    );
    let signature = sign_query(&query, secret_key.trim());
    let url = format!("{}/fapi/v1/order?{}&signature={}", TESTNET_BASE_URL, query, signature);

    let res = ureq::post(&url)
        .set("X-MBX-APIKEY", api_key.trim())
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_default();
                format!("Binance API Error (Status {}): {}", code, body)
            }
            ureq::Error::Transport(t) => format!("Koneksi ke Binance Testnet gagal: {}", t),
        })?;

    let val: serde_json::Value = res.into_json().map_err(|e| format!("Format JSON error: {}", e))?;
    let order_id = val["orderId"].as_i64().unwrap_or(0);
    Ok((order_id, actual_qty, qty_str))
}

/// Eksekusi Tutup Posisi secara Market di Binance Testnet
pub fn market_close_position(symbol: &str, side: &str, qty: f64, api_key: &str, secret_key: &str) -> Result<String, String> {
    let sym = symbol.to_uppercase();
    let close_side = if side.eq_ignore_ascii_case("BUY") { "SELL" } else { "BUY" };
    let ts = get_timestamp_ms();

    let query = format!(
        "symbol={}&side={}&type=MARKET&quantity={}&reduceOnly=true&timestamp={}&recvWindow=5000",
        sym, close_side, qty, ts
    );
    let signature = sign_query(&query, secret_key.trim());
    let url = format!("{}/fapi/v1/order?{}&signature={}", TESTNET_BASE_URL, query, signature);

    let res = ureq::post(&url)
        .set("X-MBX-APIKEY", api_key.trim())
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| format!("Gagal menutup order {} {}: {}", sym, close_side, e))?;

    let val: serde_json::Value = res.into_json().map_err(|e| format!("Format JSON error: {}", e))?;
    let order_id = val["orderId"].as_i64().unwrap_or(0);
    Ok(format!("Order Tutup #{} berhasil dieksekusi", order_id))
}

/// Panic Button: Tutup SEMUA posisi yang terbuka di Binance Testnet secara instan
pub fn panic_close_all_positions(api_key: &str, secret_key: &str) -> Result<usize, String> {
    let positions = get_live_positions(api_key, secret_key)?;
    let mut closed_count = 0;

    for p in positions {
        if p.position_amt > 0.0 {
            if let Ok(_) = market_close_position(&p.symbol, &p.side, p.position_amt, api_key, secret_key) {
                closed_count += 1;
            }
        }
    }

    Ok(closed_count)
}
