#![allow(dead_code)]

use crate::models::BalanceSummary;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type HmacSha256 = Hmac<Sha256>;

pub const MARKET_DATA_REST_URL: &str = "https://fapi.binance.com";

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub qty_precision: usize,
    pub price_precision: usize,
    pub step_size: f64,
    pub min_qty: f64,
    pub min_notional: f64,
}

impl Default for SymbolInfo {
    fn default() -> Self {
        Self {
            qty_precision: 3,
            price_precision: 2,
            step_size: 0.001,
            min_qty: 0.001,
            min_notional: 5.0,
        }
    }
}

#[derive(Clone)]
pub struct BinanceFuturesClient {
    pub api_key: Arc<RwLock<String>>,
    pub secret_key: Arc<RwLock<String>>,
    pub use_testnet: Arc<RwLock<bool>>,
    pub exec_base_url: Arc<RwLock<String>>,
    pub market_base_url: String,
    pub session: reqwest::Client,
    pub symbols_info: Arc<RwLock<HashMap<String, SymbolInfo>>>,
    pub max_leverage_cache: Arc<RwLock<HashMap<String, u32>>>,
    pub time_offset_ms: Arc<RwLock<i64>>,
}

impl BinanceFuturesClient {
    pub fn new(api_key: String, secret_key: String, use_testnet: bool) -> Self {
        let exec_base = if use_testnet {
            "https://testnet.binancefuture.com".to_string()
        } else {
            "https://fapi.binance.com".to_string()
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        let s = Self {
            api_key: Arc::new(RwLock::new(api_key)),
            secret_key: Arc::new(RwLock::new(secret_key)),
            use_testnet: Arc::new(RwLock::new(use_testnet)),
            exec_base_url: Arc::new(RwLock::new(exec_base)),
            market_base_url: MARKET_DATA_REST_URL.to_string(),
            session: client,
            symbols_info: Arc::new(RwLock::new(HashMap::new())),
            max_leverage_cache: Arc::new(RwLock::new(HashMap::new())),
            time_offset_ms: Arc::new(RwLock::new(0)),
        };

        s
    }

    pub async fn init(&self) {
        self.sync_time().await;
        self.load_exchange_info().await;
        self.load_leverage_brackets().await;
    }

    pub async fn update_keys(&self, api_key: String, secret_key: String, use_testnet: bool) {
        *self.api_key.write().await = api_key;
        *self.secret_key.write().await = secret_key;
        *self.use_testnet.write().await = use_testnet;
        let exec_base = if use_testnet {
            "https://testnet.binancefuture.com".to_string()
        } else {
            "https://fapi.binance.com".to_string()
        };
        *self.exec_base_url.write().await = exec_base;

        self.sync_time().await;
        self.load_exchange_info().await;
        self.load_leverage_brackets().await;
    }

    pub async fn sync_time(&self) {
        let exec_url = self.exec_base_url.read().await.clone();
        let url = format!("{}/fapi/v1/time", exec_url);
        if let Ok(resp) = self.session.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(v) = resp.json::<serde_json::Value>().await {
                    if let Some(st) = v.get("serverTime").and_then(|x| x.as_i64()) {
                        let now = chrono::Utc::now().timestamp_millis();
                        *self.time_offset_ms.write().await = st - now;
                    }
                }
            }
        }
    }

    pub async fn get_server_time_ms(&self) -> i64 {
        let now = chrono::Utc::now().timestamp_millis();
        now + *self.time_offset_ms.read().await
    }

    pub async fn load_exchange_info(&self) {
        let exec_url = self.exec_base_url.read().await.clone();
        let mut target_url = format!("{}/fapi/v1/exchangeInfo", exec_url);

        let mut data_opt = None;
        if let Ok(resp) = self.session.get(&target_url).send().await {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    data_opt = Some(val);
                }
            }
        }

        if data_opt.is_none() {
            target_url = format!("{}/fapi/v1/exchangeInfo", self.market_base_url);
            if let Ok(resp) = self.session.get(&target_url).send().await {
                if resp.status().is_success() {
                    if let Ok(val) = resp.json::<serde_json::Value>().await {
                        data_opt = Some(val);
                    }
                }
            }
        }

        if let Some(val) = data_opt {
            let mut map = self.symbols_info.write().await;
            if let Some(symbols) = val.get("symbols").and_then(|x| x.as_array()) {
                for s in symbols {
                    let sym = s.get("symbol").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    if sym.is_empty() { continue; }

                    let qty_prec = s.get("quantityPrecision").and_then(|x| x.as_u64()).unwrap_or(3) as usize;
                    let price_prec = s.get("pricePrecision").and_then(|x| x.as_u64()).unwrap_or(2) as usize;

                    let mut step_size = 0.001;
                    let mut min_qty = 0.001;
                    let mut min_notional = 5.0;

                    if let Some(filters) = s.get("filters").and_then(|x| x.as_array()) {
                        for f in filters {
                            let ftype = f.get("filterType").and_then(|x| x.as_str()).unwrap_or("");
                            if ftype == "LOT_SIZE" {
                                if let Some(ss) = f.get("stepSize").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()) {
                                    step_size = ss;
                                }
                                if let Some(mq) = f.get("minQty").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()) {
                                    min_qty = mq;
                                }
                            } else if ftype == "MIN_NOTIONAL" {
                                if let Some(notional) = f.get("notional").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()) {
                                    min_notional = notional;
                                }
                            }
                        }
                    }

                    map.insert(sym.to_uppercase(), SymbolInfo {
                        qty_precision: qty_prec,
                        price_precision: price_prec,
                        step_size,
                        min_qty,
                        min_notional,
                    });
                }
            }
        }
    }

    pub async fn round_qty(&self, symbol: &str, qty: f64) -> f64 {
        let map = self.symbols_info.read().await;
        if let Some(info) = map.get(&symbol.to_uppercase()) {
            if info.step_size > 0.0 {
                let rounded = (qty / info.step_size).floor() * info.step_size;
                let factor = 10f64.powi(info.qty_precision as i32);
                return (rounded * factor).round() / factor;
            }
        }
        (qty * 1000.0).round() / 1000.0
    }

    pub async fn round_price(&self, symbol: &str, price: f64) -> f64 {
        let map = self.symbols_info.read().await;
        if let Some(info) = map.get(&symbol.to_uppercase()) {
            let factor = 10f64.powi(info.price_precision as i32);
            return (price * factor).round() / factor;
        }
        (price * 100.0).round() / 100.0
    }

    pub async fn signed_request(
        &self,
        method: &str,
        endpoint: &str,
        mut params: Vec<(&str, String)>,
    ) -> Result<serde_json::Value, String> {
        let ts = self.get_server_time_ms().await;
        params.push(("timestamp", ts.to_string()));

        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let sec = self.secret_key.read().await.clone();
        let mut mac = HmacSha256::new_from_slice(sec.as_bytes())
            .map_err(|e| format!("HMAC init error: {}", e))?;
        mac.update(query_string.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let signed_query = format!("{}&signature={}", query_string, signature);
        let exec_base = self.exec_base_url.read().await.clone();
        let url = format!("{}{}?{}", exec_base, endpoint, signed_query);

        let apikey = self.api_key.read().await.clone();
        let req = match method.to_uppercase().as_str() {
            "GET" => self.session.get(&url),
            "POST" => self.session.post(&url),
            "DELETE" => self.session.delete(&url),
            _ => return Err(format!("Unsupported method {}", method)),
        };

        let resp = req
            .header("X-MBX-APIKEY", apikey)
            .send()
            .await
            .map_err(|e| format!("HTTP request error: {}", e))?;

        let status = resp.status();
        let body_text = resp
            .text()
            .await
            .map_err(|e| format!("Read text error: {}", e))?;

        if status.is_success() {
            serde_json::from_str(&body_text).map_err(|e| format!("JSON parse error: {}", e))
        } else {
            Err(format!("HTTP {}: {}", status, body_text))
        }
    }

    pub async fn get_account(&self) -> Result<serde_json::Value, String> {
        self.signed_request("GET", "/fapi/v2/account", vec![]).await
    }

    pub async fn get_balance_summary(&self) -> Result<BalanceSummary, String> {
        let acc = self.get_account().await?;
        let wallet = acc.get("totalWalletBalance").and_then(|x| x.as_str()).and_then(|x| x.parse().ok()).unwrap_or(0.0);
        let avail = acc.get("availableBalance").and_then(|x| x.as_str()).and_then(|x| x.parse().ok()).unwrap_or(0.0);
        let unpnl = acc.get("totalUnrealizedProfit").and_then(|x| x.as_str()).and_then(|x| x.parse().ok()).unwrap_or(0.0);
        let margin = acc.get("totalInitialMargin").and_then(|x| x.as_str()).and_then(|x| x.parse().ok()).unwrap_or(0.0);
        Ok(BalanceSummary {
            wallet_balance: wallet,
            available_balance: avail,
            unrealized_pnl: unpnl,
            margin_used: margin,
            equity: wallet + unpnl,
        })
    }

    pub async fn get_position_risk(&self, symbol: Option<&str>) -> Vec<serde_json::Value> {
        let mut p = Vec::new();
        if let Some(s) = symbol {
            p.push(("symbol", s.to_uppercase()));
        }
        if let Ok(v) = self.signed_request("GET", "/fapi/v2/positionRisk", p).await {
            if let Some(arr) = v.as_array() {
                return arr.clone();
            }
        }
        Vec::new()
    }

    pub async fn get_leverage_bracket(&self, symbol: Option<&str>) -> Vec<serde_json::Value> {
        let mut p = Vec::new();
        if let Some(s) = symbol {
            p.push(("symbol", s.to_uppercase()));
        }
        if let Ok(v) = self.signed_request("GET", "/fapi/v1/leverageBracket", p).await {
            if let Some(arr) = v.as_array() {
                return arr.clone();
            }
        }
        Vec::new()
    }

    pub async fn load_leverage_brackets(&self) {
        let brackets = self.get_leverage_bracket(None).await;
        let mut cache = self.max_leverage_cache.write().await;
        for item in brackets {
            let sym = item.get("symbol").and_then(|x| x.as_str()).unwrap_or("").to_uppercase();
            if sym.is_empty() { continue; }
            if let Some(b_arr) = item.get("brackets").and_then(|x| x.as_array()) {
                let mut max_l = 20;
                for b in b_arr {
                    if let Some(l) = b.get("initialLeverage").and_then(|x| x.as_u64()) {
                        if l as u32 > max_l {
                            max_l = l as u32;
                        }
                    }
                }
                cache.insert(sym, max_l);
            }
        }
    }

    pub async fn get_max_leverage(&self, symbol: &str) -> u32 {
        let sym_up = symbol.to_uppercase();
        {
            let cache = self.max_leverage_cache.read().await;
            if let Some(&lev) = cache.get(&sym_up) {
                return lev;
            }
        }
        let brackets = self.get_leverage_bracket(Some(&sym_up)).await;
        if let Some(item) = brackets.first() {
            if let Some(b_arr) = item.get("brackets").and_then(|x| x.as_array()) {
                let mut max_l = 20;
                for b in b_arr {
                    if let Some(l) = b.get("initialLeverage").and_then(|x| x.as_u64()) {
                        if l as u32 > max_l {
                            max_l = l as u32;
                        }
                    }
                }
                self.max_leverage_cache.write().await.insert(sym_up, max_l);
                return max_l;
            }
        }
        20
    }

    pub async fn set_leverage(&self, symbol: &str, leverage: u32) -> Result<serde_json::Value, String> {
        let params = vec![
            ("symbol", symbol.to_uppercase()),
            ("leverage", leverage.to_string()),
        ];
        self.signed_request("POST", "/fapi/v1/leverage", params).await
    }

    pub async fn get_ticker_24hr(&self) -> Vec<serde_json::Value> {
        let url = format!("{}/fapi/v1/ticker/24hr", self.market_base_url);
        if let Ok(resp) = self.session.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(arr) = val.as_array() {
                        return arr.clone();
                    }
                }
            }
        }
        Vec::new()
    }

    pub async fn fetch_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, String> {
        let url = format!("{}/fapi/v1/klines", self.market_base_url);
        let resp = self
            .session
            .get(&url)
            .query(&[
                ("symbol", symbol.to_uppercase().as_str()),
                ("interval", interval),
                ("limit", &limit.to_string()),
            ])
            .send()
            .await
            .map_err(|e| format!("Klines fetch error: {}", e))?;

        if resp.status().is_success() {
            let val = resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
            if let Some(arr) = val.as_array() {
                return Ok(arr.clone());
            }
        }
        Err("Failed to parse klines array".to_string())
    }

    pub async fn place_order(
        &self,
        symbol: &str,
        side: &str,
        order_type: &str,
        quantity: f64,
        price: Option<f64>,
        stop_price: Option<f64>,
        time_in_force: Option<&str>,
        reduce_only: bool,
    ) -> Result<serde_json::Value, String> {
        let sym = symbol.to_uppercase();
        let qty_clean = self.round_qty(&sym, quantity).await;

        let mut params = vec![
            ("symbol", sym.clone()),
            ("side", side.to_uppercase()),
            ("type", order_type.to_uppercase()),
            ("quantity", qty_clean.to_string()),
        ];

        if reduce_only {
            params.push(("reduceOnly", "true".to_string()));
        }

        if order_type.to_uppercase() == "LIMIT" {
            let px = price.ok_or("Price required for LIMIT")?;
            let px_clean = self.round_price(&sym, px).await;
            params.push(("price", px_clean.to_string()));
            params.push(("timeInForce", time_in_force.unwrap_or("GTC").to_string()));
        } else if order_type.to_uppercase() == "STOP_MARKET" {
            let sp = stop_price.ok_or("stop_price required for STOP_MARKET")?;
            let sp_clean = self.round_price(&sym, sp).await;
            params.push(("stopPrice", sp_clean.to_string()));
        }

        self.signed_request("POST", "/fapi/v1/order", params).await
    }

    pub async fn place_trailing_stop_algo(
        &self,
        symbol: &str,
        side: &str,
        quantity: f64,
        callback_rate: f64,
        activation_price: Option<f64>,
        reduce_only: bool,
    ) -> Result<serde_json::Value, String> {
        let sym = symbol.to_uppercase();
        let qty_clean = self.round_qty(&sym, quantity).await;
        let cb_clamped = callback_rate.max(0.1).min(5.0);
        let cb_str = format!("{:.1}", cb_clamped);

        let mut params = vec![
            ("algoType", "CONDITIONAL".to_string()),
            ("type", "TRAILING_STOP_MARKET".to_string()),
            ("symbol", sym.clone()),
            ("side", side.to_uppercase()),
            ("quantity", qty_clean.to_string()),
            ("callbackRate", cb_str),
        ];

        if reduce_only {
            params.push(("reduceOnly", "true".to_string()));
        }

        if let Some(act) = activation_price {
            if act > 0.0 {
                let act_clean = self.round_price(&sym, act).await;
                params.push(("activationPrice", act_clean.to_string()));
            }
        }

        self.signed_request("POST", "/fapi/v1/algoOrder", params).await
    }

    pub async fn cancel_order(&self, symbol: &str, order_id: i64) -> Result<serde_json::Value, String> {
        let params = vec![
            ("symbol", symbol.to_uppercase()),
            ("orderId", order_id.to_string()),
        ];
        self.signed_request("DELETE", "/fapi/v1/order", params).await
    }

    pub async fn cancel_all_orders(&self, symbol: &str) -> Result<serde_json::Value, String> {
        let params = vec![("symbol", symbol.to_uppercase())];
        self.signed_request("DELETE", "/fapi/v1/allOpenOrders", params).await
    }

    pub async fn cancel_algo_order(&self, algo_id: i64) -> Result<serde_json::Value, String> {
        let params = vec![("algoId", algo_id.to_string())];
        self.signed_request("DELETE", "/fapi/v1/algoOrder", params).await
    }

    pub async fn cancel_all_algo_orders(&self, symbol: &str) -> Vec<serde_json::Value> {
        let mut res = Vec::new();
        let p = vec![("symbol", symbol.to_uppercase())];
        if let Ok(v) = self.signed_request("GET", "/fapi/v1/openAlgoOrders", p).await {
            if let Some(arr) = v.as_array() {
                for item in arr {
                    if let Some(aid) = item.get("algoId").and_then(|x| x.as_i64()) {
                        if let Ok(c_res) = self.cancel_algo_order(aid).await {
                            res.push(c_res);
                        }
                    }
                }
            }
        }
        res
    }

    pub async fn get_user_trades(&self, symbol: &str, limit: usize) -> Vec<serde_json::Value> {
        let params = vec![
            ("symbol", symbol.to_uppercase()),
            ("limit", limit.to_string()),
        ];
        if let Ok(v) = self.signed_request("GET", "/fapi/v1/userTrades", params).await {
            if let Some(arr) = v.as_array() {
                return arr.clone();
            }
        }
        Vec::new()
    }
}
