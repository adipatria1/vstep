#![allow(dead_code)]

use crate::binance_client::BinanceFuturesClient;
use crate::models::{
    BotConfig, LogEntry, PositionInfo, TradeRecord,
};
use crate::presets::find_preset;
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::connect_async;

pub const WSS_ENDPOINTS: &[&str] = &[
    "wss://fstream.binance.com/ws",
    "wss://fstream.binance.com:9443/ws",
];

pub fn get_interval_ms(interval: &str) -> i64 {
    match interval {
        "1m" => 60_000,
        "3m" => 180_000,
        "5m" => 300_000,
        "15m" => 900_000,
        "30m" => 1_800_000,
        "1h" => 3_600_000,
        "2h" => 7_200_000,
        "4h" => 14_400_000,
        "6h" => 21_600_000,
        "8h" => 28_800_000,
        "12h" => 43_200_000,
        "1d" => 86_400_000,
        _ => 60_000,
    }
}

// =====================================================================
// VOLSTEP ENGINE (PURE MATH PINE SCRIPT v6 REPLICATION)
// =====================================================================
#[derive(Debug, Clone)]
pub struct VolStepEngine {
    pub symbol: String,
    pub line_mode: String,
    pub vol_type: String,
    pub length: usize,
    pub vol_mult: f64,
    pub stdev_bessel: bool,

    pub closed_closes: VecDeque<f64>,
    pub tr_warmup: Vec<f64>,
    pub confirmed_rma: Option<f64>,
    pub prev_closed_close: Option<f64>,
    pub closed_bars_count: usize,
    pub last_closed_open_time: i64,

    pub price_decimals: usize,

    pub confirmed_sr: Option<f64>,
    pub cur_level: Option<f64>,
    pub prev_level: Option<f64>,
    pub cur_pct: Option<f64>,
    pub cur_dir: Option<String>,
    pub confirmed_vol_value: Option<f64>,

    pub sr_high: Option<f64>,
    pub sr_low: Option<f64>,
    pub hi_arr: Vec<f64>,
    pub lo_arr: Vec<f64>,

    pub live_open: Option<f64>,
    pub live_high: Option<f64>,
    pub live_low: Option<f64>,
    pub live_close: Option<f64>,
    pub tick_dir: i32,
    pub candle_open_time: i64,
    pub candle_close_time: i64,
    pub pending_tick: Option<(f64, Option<String>, i64)>,

    pub live_vol_value: Option<f64>,
    pub next_sr_active: bool,
    pub next_sr_dir: Option<String>,
    pub next_sr_price: Option<f64>,
    pub next_sr_pct: Option<f64>,
}

impl VolStepEngine {
    pub fn new(
        symbol: String,
        line_mode: String,
        vol_type: String,
        length: usize,
        vol_mult: f64,
        stdev_bessel: bool,
    ) -> Self {
        Self {
            symbol: symbol.to_uppercase(),
            line_mode,
            vol_type,
            length,
            vol_mult,
            stdev_bessel,
            closed_closes: VecDeque::with_capacity(length),
            tr_warmup: Vec::new(),
            confirmed_rma: None,
            prev_closed_close: None,
            closed_bars_count: 0,
            last_closed_open_time: -1,
            price_decimals: 2,
            confirmed_sr: None,
            cur_level: None,
            prev_level: None,
            cur_pct: None,
            cur_dir: None,
            confirmed_vol_value: None,
            sr_high: None,
            sr_low: None,
            hi_arr: Vec::new(),
            lo_arr: Vec::new(),
            live_open: None,
            live_high: None,
            live_low: None,
            live_close: None,
            tick_dir: 0,
            candle_open_time: 0,
            candle_close_time: 0,
            pending_tick: None,
            live_vol_value: None,
            next_sr_active: false,
            next_sr_dir: None,
            next_sr_price: None,
            next_sr_pct: None,
        }
    }

    pub fn detect_precision(&mut self, raw_str: &str) {
        if let Some(pos) = raw_str.find('.') {
            let trimmed = raw_str.trim_end_matches('0');
            if let Some(pos2) = trimmed.find('.') {
                let dec = trimmed[pos2 + 1..].len();
                if dec > self.price_decimals {
                    self.price_decimals = dec.min(8);
                }
            } else {
                let dec = raw_str[pos + 1..].len();
                if dec > self.price_decimals {
                    self.price_decimals = dec.min(8);
                }
            }
        }
    }

    pub fn fmt(&self, val: Option<f64>) -> String {
        match val {
            Some(v) => format!("{:.prec$}", v, prec = self.price_decimals),
            None => "-".to_string(),
        }
    }

    fn calc_stdev_manual(&self, values: &[f64]) -> Option<f64> {
        let n = values.len();
        if n <= 1 {
            return None;
        }
        let mean = values.iter().sum::<f64>() / (n as f64);
        let divisor = if self.stdev_bessel { (n - 1) as f64 } else { n as f64 };
        let variance = values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / divisor;
        Some(variance.sqrt())
    }

    fn calc_true_range(high: f64, low: f64, prev_close: Option<f64>) -> f64 {
        match prev_close {
            None => high - low,
            Some(pc) => (high - low).max((high - pc).abs()).max((low - pc).abs()),
        }
    }

    fn compute_volatility_closed(&mut self, high: f64, low: f64, close: f64) -> Option<f64> {
        let tr = Self::calc_true_range(high, low, self.prev_closed_close);
        self.prev_closed_close = Some(close);

        if self.closed_closes.len() == self.length {
            self.closed_closes.pop_front();
        }
        self.closed_closes.push_back(close);
        self.closed_bars_count += 1;

        if self.closed_bars_count < self.length {
            self.tr_warmup.push(tr);
            self.confirmed_rma = None;
        } else if self.closed_bars_count == self.length {
            self.tr_warmup.push(tr);
            self.confirmed_rma = Some(self.tr_warmup.iter().sum::<f64>() / (self.length as f64));
        } else {
            let prev_rma = self.confirmed_rma.unwrap_or(tr);
            self.confirmed_rma = Some((tr + ((self.length - 1) as f64) * prev_rma) / (self.length as f64));
        }

        if self.closed_bars_count < self.length {
            return None;
        }

        let raw_vol = if self.vol_type == "Stdev" {
            let closes: Vec<f64> = self.closed_closes.iter().copied().collect();
            self.calc_stdev_manual(&closes)
        } else {
            self.confirmed_rma
        };

        raw_vol.map(|v| v * self.vol_mult)
    }

    fn compute_volatility_live(&self, high: f64, low: f64, close: f64) -> Option<f64> {
        if self.closed_bars_count < self.length - 1 {
            return None;
        }

        let raw_vol = if self.vol_type == "Stdev" {
            let mut window: Vec<f64> = self
                .closed_closes
                .iter()
                .rev()
                .take(self.length - 1)
                .copied()
                .collect();
            window.reverse();
            window.push(close);
            if window.len() < self.length {
                return None;
            }
            self.calc_stdev_manual(&window)
        } else {
            let live_tr = Self::calc_true_range(high, low, self.prev_closed_close);
            if self.closed_bars_count == self.length - 1 {
                Some((self.tr_warmup.iter().sum::<f64>() + live_tr) / (self.length as f64))
            } else {
                let prev_rma = self.confirmed_rma.unwrap_or(live_tr);
                Some((live_tr + ((self.length - 1) as f64) * prev_rma) / (self.length as f64))
            }
        };

        raw_vol.map(|v| v * self.vol_mult)
    }

    pub fn process_closed_candle(
        &mut self,
        _open_p: f64,
        high_p: f64,
        low_p: f64,
        close_p: f64,
        raw_close_str: Option<&str>,
        open_time: Option<i64>,
        close_time: Option<i64>,
        interval_ms: Option<i64>,
    ) -> bool {
        if let Some(ot) = open_time {
            if ot <= self.last_closed_open_time {
                return false;
            }
            self.last_closed_open_time = ot;
        }

        if let Some(raw) = raw_close_str {
            self.detect_precision(raw);
        }

        if self.confirmed_sr.is_none() {
            self.confirmed_sr = Some(close_p);
        }

        let vol_value = self.compute_volatility_closed(high_p, low_p, close_p);
        self.confirmed_vol_value = vol_value;

        let active_sr = self.confirmed_sr.unwrap_or(close_p);
        let new_level = if let Some(vol) = vol_value {
            let step_level = if self.line_mode == "Anchor" {
                close_p
            } else {
                let direction = if close_p > active_sr { 1.0 } else { -1.0 };
                active_sr + direction * vol
            };

            if (close_p - active_sr).abs() > vol {
                step_level
            } else {
                active_sr
            }
        } else {
            active_sr
        };

        self.confirmed_sr = Some(new_level);
        let updated_sr = new_level;

        let is_price_step = match self.cur_level {
            None => true,
            Some(curr) => (updated_sr - curr).abs() > f64::EPSILON,
        };

        if is_price_step {
            if let (Some(h), Some(l), Some(lvl)) = (self.sr_high, self.sr_low, self.cur_level) {
                if lvl > 0.0 {
                    self.hi_arr.push(((h - lvl) / lvl * 100.0).max(0.0));
                    self.lo_arr.push(((lvl - l) / lvl * 100.0).max(0.0));
                }
            }

            if self.cur_level.is_some() {
                self.prev_level = self.cur_level;
            }
            self.cur_level = Some(updated_sr);
            self.sr_high = None;
            self.sr_low = None;

            if let Some(prev) = self.prev_level {
                if prev != 0.0 {
                    let pct = ((updated_sr - prev) / prev) * 100.0;
                    self.cur_pct = Some(pct);
                    self.cur_dir = Some(if pct >= 0.0 { "BULL".to_string() } else { "BEAR".to_string() });
                } else {
                    self.cur_pct = None;
                    self.cur_dir = None;
                }
            } else {
                self.cur_pct = None;
                self.cur_dir = None;
            }
        } else {
            self.sr_high = Some(match self.sr_high {
                Some(h) => h.max(high_p),
                None => high_p,
            });
            self.sr_low = Some(match self.sr_low {
                Some(l) => l.min(low_p),
                None => low_p,
            });
        }

        self.next_sr_active = false;
        self.next_sr_dir = None;
        self.next_sr_price = None;
        self.next_sr_pct = None;

        if let (Some(ct), Some(ims)) = (close_time, interval_ms) {
            let next_open = ct + 1;
            let next_close = ct + ims;
            self.candle_open_time = next_open;
            self.candle_close_time = next_close;
            self.live_open = Some(close_p);
            self.live_high = Some(close_p);
            self.live_low = Some(close_p);
            self.live_close = Some(close_p);

            if let Some((p_px, p_raw, p_ts)) = self.pending_tick.take() {
                if p_ts >= next_open {
                    self.update_live_tick(p_px, None, None, None, None, None, p_raw.as_deref(), Some(p_ts));
                }
            }
        }

        is_price_step
    }

    fn evaluate_live_sr(&mut self) {
        let (lh, ll, lc) = match (self.live_high, self.live_low, self.live_close) {
            (Some(h), Some(l), Some(c)) => (h, l, c),
            _ => return,
        };

        let vol_value = self.compute_volatility_live(lh, ll, lc);
        self.live_vol_value = vol_value;

        let conf_sr = match self.confirmed_sr {
            Some(s) => s,
            None => {
                self.next_sr_active = false;
                return;
            }
        };

        if let Some(vol) = vol_value {
            let diff = lc - conf_sr;
            if diff.abs() > vol {
                self.next_sr_active = true;
                self.next_sr_dir = Some(if diff > 0.0 { "BULL".to_string() } else { "BEAR".to_string() });
                let next_p = if self.line_mode == "Anchor" {
                    lc
                } else {
                    let dir = if diff > 0.0 { 1.0 } else { -1.0 };
                    conf_sr + dir * vol
                };
                self.next_sr_price = Some(next_p);
                self.next_sr_pct = if conf_sr != 0.0 {
                    Some(((next_p - conf_sr) / conf_sr) * 100.0)
                } else {
                    Some(0.0)
                };
            } else {
                self.next_sr_active = false;
                self.next_sr_dir = None;
                self.next_sr_price = None;
                self.next_sr_pct = None;
            }
        } else {
            self.next_sr_active = false;
        }
    }

    pub fn update_live_tick(
        &mut self,
        price: f64,
        high_p: Option<f64>,
        low_p: Option<f64>,
        open_p: Option<f64>,
        open_time: Option<i64>,
        close_time: Option<i64>,
        raw_price_str: Option<&str>,
        trade_time: Option<i64>,
    ) {
        if let Some(raw) = raw_price_str {
            self.detect_precision(raw);
        }

        if let Some(tt) = trade_time {
            if self.candle_close_time > 0 && tt > self.candle_close_time {
                self.pending_tick = Some((price, raw_price_str.map(|s| s.to_string()), tt));
                return;
            }
        }

        if let Some(ot) = open_time {
            if ot <= self.last_closed_open_time {
                return;
            }
            if ot > self.candle_open_time {
                self.candle_open_time = ot;
                if let Some(ct) = close_time {
                    self.candle_close_time = ct;
                }
                self.live_open = Some(open_p.unwrap_or(price));
                self.live_high = Some(high_p.unwrap_or(price));
                self.live_low = Some(low_p.unwrap_or(price));
            } else {
                if let Some(ct) = close_time {
                    self.candle_close_time = ct;
                }
                if let Some(op) = open_p {
                    self.live_open = Some(op);
                }
                if let Some(hp) = high_p {
                    self.live_high = Some(match self.live_high {
                        Some(curr_h) => curr_h.max(hp),
                        None => hp,
                    });
                }
                if let Some(lp) = low_p {
                    self.live_low = Some(match self.live_low {
                        Some(curr_l) => curr_l.min(lp),
                        None => lp,
                    });
                }
            }
        } else {
            if self.live_open.is_none() {
                self.live_open = Some(price);
            }
            self.live_high = Some(match self.live_high {
                Some(curr_h) => curr_h.max(price),
                None => price,
            });
            self.live_low = Some(match self.live_low {
                Some(curr_l) => curr_l.min(price),
                None => price,
            });
        }

        if let Some(prev_close) = self.live_close {
            if (price - prev_close).abs() > f64::EPSILON {
                self.tick_dir = if price > prev_close { 1 } else { -1 };
            }
        }
        self.live_close = Some(price);

        self.evaluate_live_sr();
    }

    pub fn get_median_excursion(&self) -> (Option<f64>, Option<f64>) {
        let mut med_h = None;
        let mut med_l = None;

        if !self.hi_arr.is_empty() {
            let mut s = self.hi_arr.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = s.len() / 2;
            med_h = Some(if s.len() % 2 != 0 {
                s[mid]
            } else {
                (s[mid - 1] + s[mid]) / 2.0
            });
        }

        if !self.lo_arr.is_empty() {
            let mut s = self.lo_arr.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = s.len() / 2;
            med_l = Some(if s.len() % 2 != 0 {
                s[mid]
            } else {
                (s[mid - 1] + s[mid]) / 2.0
            });
        }

        (med_h, med_l)
    }
}

// =====================================================================
// BOT MANAGER
// =====================================================================
#[derive(Debug, Clone)]
pub struct EffectiveStrategy {
    pub mode: String,
    pub tp_type: String,
    pub tp_val: f64,
    pub sl_type: String,
    pub sl_val: f64,
    pub use_trailing: bool,
    pub trailing_mode: String,
    pub trail_type: String,
    pub trail_act: f64,
    pub trail_dist: f64,
    pub min_levels_median: usize,
}

#[derive(Debug, Clone)]
pub struct PendingSnipingOrder {
    pub order_id: i64,
    pub side: String,
    pub qty: f64,
    pub price: f64,
    pub time: f64,
}

#[derive(Clone)]
pub struct BotManager {
    pub config_path: String,
    pub config: Arc<RwLock<BotConfig>>,
    pub client: BinanceFuturesClient,
    pub engines: Arc<RwLock<HashMap<String, VolStepEngine>>>,
    pub is_running: Arc<AtomicBool>,
    pub status: Arc<RwLock<String>>,
    pub status_message: Arc<RwLock<String>>,
    pub logs: Arc<RwLock<VecDeque<LogEntry>>>,
    pub active_positions: Arc<RwLock<HashMap<String, PositionInfo>>>,
    pub trade_history: Arc<RwLock<VecDeque<TradeRecord>>>,
    pub last_entered_candle: Arc<RwLock<HashMap<String, i64>>>,
    pub pending_sniping_orders: Arc<RwLock<HashMap<String, PendingSnipingOrder>>>,
    pub last_top_symbols: Arc<RwLock<HashSet<String>>>,
    pub ws_restart_notify: Arc<tokio::sync::Notify>,
}

impl BotManager {
    pub fn new(config_path: &str) -> Self {
        let mut cfg = BotConfig::default();
        if let Ok(content) = std::fs::read_to_string(config_path) {
            if let Ok(parsed) = serde_json::from_str::<BotConfig>(&content) {
                cfg = parsed;
            }
        }

        let client = BinanceFuturesClient::new(
            cfg.api_key.clone(),
            cfg.secret_key.clone(),
            cfg.use_testnet,
        );

        Self {
            config_path: config_path.to_string(),
            config: Arc::new(RwLock::new(cfg)),
            client,
            engines: Arc::new(RwLock::new(HashMap::new())),
            is_running: Arc::new(AtomicBool::new(false)),
            status: Arc::new(RwLock::new("STOPPED".to_string())),
            status_message: Arc::new(RwLock::new("Ready".to_string())),
            logs: Arc::new(RwLock::new(VecDeque::with_capacity(200))),
            active_positions: Arc::new(RwLock::new(HashMap::new())),
            trade_history: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
            last_entered_candle: Arc::new(RwLock::new(HashMap::new())),
            pending_sniping_orders: Arc::new(RwLock::new(HashMap::new())),
            last_top_symbols: Arc::new(RwLock::new(HashSet::new())),
            ws_restart_notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub async fn log(&self, text: &str, level: &str) {
        let ts = chrono::Local::now().format("%H:%M:%S").to_string();
        let entry = LogEntry {
            time: ts.clone(),
            level: level.to_string(),
            text: text.to_string(),
        };
        println!("[{}] [{}] {}", ts, level, text);
        let mut logs = self.logs.write().await;
        if logs.len() >= 200 {
            logs.pop_front();
        }
        logs.push_back(entry);
    }

    pub async fn load_config(&self) {
        if let Ok(content) = std::fs::read_to_string(&self.config_path) {
            if let Ok(parsed) = serde_json::from_str::<BotConfig>(&content) {
                *self.config.write().await = parsed;
            }
        }
    }

    pub async fn save_config(&self, new_cfg: BotConfig) {
        *self.config.write().await = new_cfg.clone();
        if let Ok(json_str) = serde_json::to_string_pretty(&new_cfg) {
            let _ = std::fs::write(&self.config_path, json_str);
        }
        self.client
            .update_keys(new_cfg.api_key, new_cfg.secret_key, new_cfg.use_testnet)
            .await;
    }

    pub async fn get_effective_strategy(&self, _symbol: &str) -> EffectiveStrategy {
        let cfg = self.config.read().await.clone();
        let p_opt = find_preset(&cfg.selected_preset);

        let mut mode = cfg.strategy_mode.clone();
        let mut tp_type = cfg.tp_type.clone();
        let mut tp_val = cfg.tp_val;
        let mut sl_type = cfg.sl_type.clone();
        let mut sl_val = cfg.sl_val;
        let mut use_trailing = cfg.use_trailing;
        let mut trail_type = "Fixed %".to_string();
        let mut trail_act = cfg.trail_act;
        let mut trail_dist = cfg.trail_dist;
        let trailing_mode = cfg.trailing_mode.clone();
        let min_sr = cfg.min_levels_median;

        if let Some(p) = p_opt {
            mode = p.mode;
            tp_type = p.tp_type.clone();
            sl_type = p.sl_type.clone();
            use_trailing = p.use_trail;

            if tp_type == "Median %" {
                tp_val = p.tp_mult.unwrap_or(1.0);
            } else if tp_type == "Fixed %" {
                tp_val = match p.tp_val_key.as_deref() {
                    Some("fixed1") => cfg.fixed1,
                    Some("fixed2") => cfg.fixed2,
                    Some("fixed3") => cfg.fixed3,
                    _ => cfg.fixed1,
                };
            } else if tp_type == "BarFix" || tp_type == "BarProf" || tp_type == "BarDir" {
                tp_val = match p.tp_bar_key.as_deref() {
                    Some("bar_tp_1") => cfg.bar_tp1 as f64,
                    Some("bar_tp_2") => cfg.bar_tp2 as f64,
                    Some("bar_tp_dir") => cfg.bar_tp_dir as f64,
                    _ => cfg.bar_tp1 as f64,
                };
            }

            if sl_type == "Median %" {
                sl_val = p.sl_mult.unwrap_or(1.0);
            } else if sl_type == "Fixed %" {
                sl_val = match p.sl_val_key.as_deref() {
                    Some("fixed1") => cfg.fixed1,
                    Some("fixed2") => cfg.fixed2,
                    Some("fixed3") => cfg.fixed3,
                    _ => cfg.fixed1,
                };
            } else if sl_type == "BarLoss" || sl_type == "MaxBar" || sl_type == "BarAdv" {
                sl_val = match p.sl_bar_key.as_deref() {
                    Some("bar_sl_1") => cfg.sl_bar1 as f64,
                    Some("bar_sl_2") => cfg.sl_bar2 as f64,
                    Some("bar_sl_adv") => cfg.sl_bar_adv as f64,
                    _ => cfg.sl_bar1 as f64,
                };
            }

            if use_trailing {
                trail_type = p.trail_type.unwrap_or_else(|| "Median %".to_string());
                if trail_type == "Median %" {
                    trail_act = p.trail_act_mult.unwrap_or(1.0);
                    trail_dist = p.trail_dist_mult.unwrap_or(0.5);
                } else {
                    trail_act = match p.trail_act_key.as_deref() {
                        Some("fixed1") => cfg.fixed1,
                        Some("fixed2") => cfg.fixed2,
                        Some("fixed3") => cfg.fixed3,
                        _ => cfg.fixed2,
                    };
                    trail_dist = match p.trail_dist_key.as_deref() {
                        Some("fixed1") => cfg.fixed1,
                        Some("fixed2") => cfg.fixed2,
                        Some("fixed3") => cfg.fixed3,
                        _ => cfg.fixed1,
                    };
                }
            }
        }

        EffectiveStrategy {
            mode,
            tp_type,
            tp_val,
            sl_type,
            sl_val,
            use_trailing,
            trailing_mode,
            trail_type,
            trail_act,
            trail_dist,
            min_levels_median: min_sr,
        }
    }

    pub async fn get_effective_leverage(&self, symbol: &str) -> u32 {
        let cfg = self.config.read().await;
        let max_lev = self.client.get_max_leverage(symbol).await;
        if cfg.leverage_mode == "Auto Max Leverage" {
            max_lev
        } else {
            cfg.leverage.min(max_lev)
        }
    }

    pub async fn resolve_active_symbols(&self) -> Vec<String> {
        let cfg = self.config.read().await.clone();
        if cfg.coin_source_mode == "Manual" {
            return cfg.symbols.iter().map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty()).collect();
        }

        let tickers = self.client.get_ticker_24hr().await;
        if tickers.is_empty() {
            self.log("Ticker 24hr kosong, fallback ke simbol manual", "WARN").await;
            return cfg.symbols.iter().map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty()).collect();
        }

        let is_testnet = *self.client.use_testnet.read().await;
        let sym_info = self.client.symbols_info.read().await;

        struct TickerItem {
            symbol: String,
            quote_vol: f64,
            pct_change: f64,
        }

        let mut valid_tickers: Vec<TickerItem> = Vec::new();

        for t in tickers {
            let sym = t.get("symbol").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if !sym.ends_with("USDT") { continue; }
            if is_testnet && !sym_info.contains_key(&sym.to_uppercase()) {
                continue;
            }

            let q_vol = t.get("quoteVolume").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
            let p_chg = t.get("priceChangePercent").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);

            if q_vol >= cfg.min_24h_volume_usdt {
                valid_tickers.push(TickerItem {
                    symbol: sym,
                    quote_vol: q_vol,
                    pct_change: p_chg,
                });
            }
        }

        if cfg.coin_source_mode == "Top Gainers" {
            valid_tickers.sort_by(|a, b| b.pct_change.partial_cmp(&a.pct_change).unwrap_or(std::cmp::Ordering::Equal));
        } else if cfg.coin_source_mode == "Top Losers" {
            valid_tickers.sort_by(|a, b| a.pct_change.partial_cmp(&b.pct_change).unwrap_or(std::cmp::Ordering::Equal));
        } else if cfg.coin_source_mode == "Top Volume" {
            valid_tickers.sort_by(|a, b| b.quote_vol.partial_cmp(&a.quote_vol).unwrap_or(std::cmp::Ordering::Equal));
        }

        let selected: Vec<String> = valid_tickers
            .into_iter()
            .take(cfg.max_screener_coins)
            .map(|x| x.symbol.to_uppercase())
            .collect();

        if !selected.is_empty() {
            self.log(
                &format!("Screener ({}): Terpilih {} koin -> {}", cfg.coin_source_mode, selected.len(), selected.join(", ")),
                "INFO",
            ).await;
            return selected;
        }

        cfg.symbols.iter().map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty()).collect()
    }

    pub async fn restart_websocket(&self) {
        self.ws_restart_notify.notify_waiters();
    }

    pub async fn init_engines(&self) {
        let symbols = self.resolve_active_symbols().await;
        *self.last_top_symbols.write().await = symbols.iter().cloned().collect();

        let cfg = self.config.read().await.clone();
        let mut map = self.engines.write().await;
        map.clear();
        for s in symbols {
            map.insert(
                s.clone(),
                VolStepEngine::new(
                    s,
                    cfg.line_mode.clone(),
                    cfg.vol_type.clone(),
                    cfg.lookback,
                    cfg.vol_mult,
                    cfg.stdev_bessel,
                ),
            );
        }
    }

    pub async fn warmup_single_symbol(&self, sym: &str, limit: usize, interval: &str) {
        let interval_ms = get_interval_ms(interval);
        if let Ok(klines) = self.client.fetch_klines(sym, interval, limit).await {
            let now_ms = self.client.get_server_time_ms().await;
            let mut engines = self.engines.write().await;
            if let Some(eng) = engines.get_mut(sym) {
                let n = klines.len();
                if n > 1 {
                    for k in &klines[..n - 1] {
                        if let (Some(ot), Some(o), Some(h), Some(l), Some(c), Some(ct)) = (
                            k.get(0).and_then(|x| x.as_i64()),
                            k.get(1).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
                            k.get(2).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
                            k.get(3).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
                            k.get(4).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
                            k.get(6).and_then(|x| x.as_i64()),
                        ) {
                            let raw_c = k.get(4).and_then(|x| x.as_str());
                            eng.process_closed_candle(o, h, l, c, raw_c, Some(ot), Some(ct), Some(interval_ms));
                        }
                    }

                    if let Some(last_k) = klines.last() {
                        if let (Some(ot), Some(o), Some(h), Some(l), Some(c), Some(ct)) = (
                            last_k.get(0).and_then(|x| x.as_i64()),
                            last_k.get(1).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
                            last_k.get(2).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
                            last_k.get(3).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
                            last_k.get(4).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
                            last_k.get(6).and_then(|x| x.as_i64()),
                        ) {
                            let raw_c = last_k.get(4).and_then(|x| x.as_str());
                            if ct <= now_ms {
                                eng.process_closed_candle(o, h, l, c, raw_c, Some(ot), Some(ct), Some(interval_ms));
                            } else {
                                eng.update_live_tick(c, Some(h), Some(l), Some(o), Some(ot), Some(ct), raw_c, None);
                            }
                        }
                    }
                }
                self.log(&format!("Selesai warmup {}: {} bars", sym, n), "INFO").await;
            }
        }
    }

    pub async fn warmup_all(&self) {
        *self.status.write().await = "WARMING_UP".to_string();
        let syms: Vec<String> = self.engines.read().await.keys().cloned().collect();
        let (interval, limit, delay) = {
            let cfg = self.config.read().await;
            (cfg.timeframe.clone(), cfg.warmup_limit, cfg.warmup_delay)
        };

        self.log(
            &format!("Memulai warmup {} klines ({}) untuk {} koin dari Live Binance...", limit, interval, syms.len()),
            "INFO",
        ).await;

        for sym in syms {
            if !self.is_running.load(Ordering::SeqCst) {
                return;
            }
            self.warmup_single_symbol(&sym, limit, &interval).await;
            tokio::time::sleep(tokio::time::Duration::from_millis((delay * 1000.0) as u64)).await;
        }

        self.sync_active_positions_from_exchange().await;
        self.sync_trades_from_exchange().await;
        *self.status.write().await = "RUNNING".to_string();
        self.log("Selesai warmup. Live Stream & Screener Aktif!", "INFO").await;
    }

    pub async fn sync_active_positions_from_exchange(&self) {
        let risks = self.client.get_position_risk(None).await;
        let mut active_symbols = HashSet::new();

        let mut positions = self.active_positions.write().await;
        let engines = self.engines.read().await;

        for r in risks {
            let sym = r.get("symbol").and_then(|x| x.as_str()).unwrap_or("").to_uppercase();
            let amt = r.get("positionAmt").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);

            if amt.abs() > 0.0 && engines.contains_key(&sym) {
                active_symbols.insert(sym.clone());
                let side = if amt > 0.0 { "BUY".to_string() } else { "SELL".to_string() };
                let entry_px = r.get("entryPrice").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                let unpnl = r.get("unRealizedProfit").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);

                if let Some(pos) = positions.get_mut(&sym) {
                    pos.unrealized_pnl = unpnl;
                    pos.entry = entry_px;
                    pos.qty = amt.abs();
                } else {
                    positions.insert(sym.clone(), PositionInfo {
                        side,
                        entry: entry_px,
                        qty: amt.abs(),
                        bars_held: 0,
                        dir_cnt: 0,
                        adv_cnt: 0,
                        unrealized_pnl: unpnl,
                        highest_px: entry_px,
                        lowest_px: entry_px,
                        trail_stop: None,
                        trail_active: false,
                        candle_time: 0,
                        algo_trailing_id: None,
                    });
                }
            }
        }
        drop(engines);

        let to_remove: Vec<String> = positions
            .keys()
            .filter(|k| !active_symbols.contains(*k))
            .cloned()
            .collect();

        for s in to_remove {
            if let Some(pos_info) = positions.remove(&s) {
                if let Some(aid) = pos_info.algo_trailing_id {
                    let _ = self.client.cancel_algo_order(aid).await;
                }
            }
            let _ = self.client.cancel_all_algo_orders(&s).await;

            let (mode, last_tops) = {
                (
                    self.config.read().await.coin_source_mode.clone(),
                    self.last_top_symbols.read().await.clone(),
                )
            };
            if mode != "Manual" && !last_tops.contains(&s) {
                self.engines.write().await.remove(&s);
                self.restart_websocket().await;
            }
        }
    }

    pub async fn sync_trades_from_exchange(&self) {
        let syms: Vec<String> = self.engines.read().await.keys().cloned().collect();
        let mut all_trades = Vec::new();

        for sym in syms {
            let raw = self.client.get_user_trades(&sym, 20).await;
            for t in raw {
                let ts = t.get("time").and_then(|x| x.as_i64()).unwrap_or(0);
                let dt_str = chrono::DateTime::from_timestamp_millis(ts)
                    .map(|d| d.format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "--:--:--".to_string());
                let buyer = t.get("buyer").and_then(|x| x.as_bool()).unwrap_or(false);
                let side = if buyer { "BUY" } else { "SELL" };
                let pnl = t.get("realizedPnl").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                let px = t.get("price").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                let qty = t.get("qty").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);

                all_trades.push(TradeRecord {
                    id: t.get("id").cloned(),
                    symbol: sym.clone(),
                    side: side.to_string(),
                    entry: px,
                    exit: px,
                    price: px,
                    qty,
                    pnl: (pnl * 100.0).round() / 100.0,
                    pct: 0.0,
                    reason: if pnl == 0.0 { "BINANCE TRADE".to_string() } else { "REALIZED PNL".to_string() },
                    time: dt_str,
                    raw_time: ts,
                });
            }
        }

        all_trades.sort_by(|a, b| b.raw_time.cmp(&a.raw_time));
        if !all_trades.is_empty() {
            let mut hist = self.trade_history.write().await;
            hist.clear();
            for item in all_trades.into_iter().take(50) {
                hist.push_back(item);
            }
        }
    }

    pub async fn calculate_order_qty(&self, symbol: &str, price: f64) -> f64 {
        let (margin_pct, leverage) = {
            let cfg = self.config.read().await;
            (cfg.margin_pct, self.get_effective_leverage(symbol).await)
        };

        let wallet_bal = match self.client.get_balance_summary().await {
            Ok(b) => if b.wallet_balance > 0.0 { b.wallet_balance } else { b.available_balance.max(1000.0) },
            Err(_) => 1000.0,
        };

        let margin_amount = wallet_bal * (margin_pct / 100.0);
        let notional = margin_amount * (leverage as f64);
        let raw_qty = if price > 0.0 { notional / price } else { 0.0 };
        let qty = self.client.round_qty(symbol, raw_qty).await;

        let info_map = self.client.symbols_info.read().await;
        let min_qty = info_map.get(&symbol.to_uppercase()).map(|x| x.min_qty).unwrap_or(0.001);

        qty.max(min_qty)
    }

    pub async fn execute_entry(&self, symbol: &str, signal_dir: &str, current_price: f64, candle_time: i64) {
        {
            let last_c = self.last_entered_candle.read().await;
            if let Some(&c_time) = last_c.get(symbol) {
                if c_time == candle_time {
                    return;
                }
            }
        }

        let strat = self.get_effective_strategy(symbol).await;
        let target_side = if strat.mode == "One-Way Trend" {
            if signal_dir == "BULL" { "BUY" } else { "SELL" }
        } else {
            if signal_dir == "BULL" { "SELL" } else { "BUY" }
        };

        {
            let engines = self.engines.read().await;
            if let Some(eng) = engines.get(symbol) {
                let need_med = (strat.tp_type == "Median %") || (strat.sl_type == "Median %") || (strat.use_trailing && strat.trail_type == "Median %");
                if need_med && eng.hi_arr.len() < strat.min_levels_median {
                    self.log(
                        &format!("[{}] Entry ditahan: Memerlukan min {} SR selesai untuk Median % (saat ini {}).", symbol, strat.min_levels_median, eng.hi_arr.len()),
                        "WARN",
                    ).await;
                    return;
                }
            }
        }

        let (one_way_rule, max_open) = {
            let cfg = self.config.read().await;
            (cfg.one_way_rule.clone(), cfg.max_open_positions)
        };

        let has_pos = self.active_positions.read().await.contains_key(symbol);

        if !has_pos {
            let cur_open = self.active_positions.read().await.len();
            if cur_open >= max_open {
                self.log(
                    &format!("[{}] Entry ditahan: Kuota posisi terbuka penuh ({}/{} posisi aktif).", symbol, cur_open, max_open),
                    "WARN",
                ).await;
                return;
            }
        } else {
            let cur_side = self.active_positions.read().await.get(symbol).map(|x| x.side.clone()).unwrap_or_default();
            if one_way_rule == "Tahan Posisi Lama (Skip SR Baru Jika Masih Open)" {
                self.log(&format!("[{}] Sinyal {} ({}), di-SKIP (Tahan Posisi Lama).", symbol, signal_dir, target_side), "INFO").await;
                return;
            } else if one_way_rule == "Reverse Jika Lawan Arah, Tahan Jika Searah" {
                if cur_side == target_side {
                    self.log(&format!("[{}] Sinyal {} ({}) searah dengan posisi aktif. SKIP.", symbol, signal_dir, target_side), "INFO").await;
                    return;
                } else {
                    self.log(&format!("[{}] REVERSE: Tutup {} untuk ganti ke {}...", symbol, cur_side, target_side), "INFO").await;
                    self.close_active_position(symbol, "REV SR").await;
                }
            } else if one_way_rule == "Tutup Posisi Lama & Buka Baru di SR Baru" {
                self.log(&format!("[{}] Tutup posisi lama {} untuk buka baru di SR baru...", symbol, cur_side), "INFO").await;
                self.close_active_position(symbol, "NEXT SR").await;
            }
        }

        let leverage = self.get_effective_leverage(symbol).await;
        let max_bursa = self.client.get_max_leverage(symbol).await;
        let _ = self.client.set_leverage(symbol, leverage).await;
        self.log(&format!("[{}] Leverage disetel ke {}x (Max bursa: {}x).", symbol, leverage, max_bursa), "INFO").await;

        let qty = self.calculate_order_qty(symbol, current_price).await;
        let entry_mode = self.config.read().await.entry_mode.clone();

        self.log(&format!("⚡ [{}] Membuka ENTRY {} {} @ ~{} ({})...", symbol, target_side, qty, current_price, entry_mode), "INFO").await;

        if entry_mode.contains("Limit") {
            match self.client.place_order(symbol, target_side, "LIMIT", qty, Some(current_price), None, Some("GTX"), false).await {
                Ok(res) => {
                    if let Some(oid) = res.get("orderId").and_then(|x| x.as_i64()) {
                        self.pending_sniping_orders.write().await.insert(symbol.to_string(), PendingSnipingOrder {
                            order_id: oid,
                            side: target_side.to_string(),
                            qty,
                            price: current_price,
                            time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                        });
                        self.last_entered_candle.write().await.insert(symbol.to_string(), candle_time);
                        self.log(&format!("[{}] Limit Post-Only dipasang (ID: {}).", symbol, oid), "INFO").await;
                    }
                }
                Err(e) => self.log(&format!("Gagal order {}: {}", symbol, e), "ERROR").await,
            }
        } else {
            match self.client.place_order(symbol, target_side, "MARKET", qty, None, None, None, false).await {
                Ok(res) => {
                    let fill_px = res.get("avgPrice").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(current_price);
                    let actual_px = if fill_px > 0.0 { fill_px } else { current_price };

                    self.active_positions.write().await.insert(symbol.to_string(), PositionInfo {
                        side: target_side.to_string(),
                        entry: actual_px,
                        qty,
                        bars_held: 0,
                        dir_cnt: 0,
                        adv_cnt: 0,
                        unrealized_pnl: 0.0,
                        highest_px: actual_px,
                        lowest_px: actual_px,
                        trail_stop: None,
                        trail_active: false,
                        candle_time,
                        algo_trailing_id: None,
                    });
                    self.last_entered_candle.write().await.insert(symbol.to_string(), candle_time);
                    self.log(&format!("✅ [{}] Berhasil ENTRY {} {} @ {}!", symbol, target_side, qty, actual_px), "INFO").await;

                    if strat.use_trailing && strat.trailing_mode == "Binance Endpoint" {
                        self.setup_binance_trailing_stop(symbol, target_side, qty, actual_px, &strat).await;
                    }
                }
                Err(e) => self.log(&format!("Gagal order {}: {}", symbol, e), "ERROR").await,
            }
        }
    }

    pub async fn setup_binance_trailing_stop(
        &self,
        symbol: &str,
        pos_side: &str,
        qty: f64,
        entry_price: f64,
        strat: &EffectiveStrategy,
    ) {
        let close_side = if pos_side == "BUY" { "SELL" } else { "BUY" };
        let act_price = if pos_side == "BUY" {
            entry_price * (1.0 + strat.trail_act / 100.0)
        } else {
            entry_price * (1.0 - strat.trail_act / 100.0)
        };

        match self.client.place_trailing_stop_algo(
            symbol,
            close_side,
            qty,
            strat.trail_dist,
            Some(act_price),
            true,
        ).await {
            Ok(res) => {
                let aid = res.get("algoId").and_then(|x| x.as_i64());
                if let Some(pos) = self.active_positions.write().await.get_mut(symbol) {
                    pos.algo_trailing_id = aid;
                }
                self.log(
                    &format!("⚡ [{}] Binance Endpoint Trailing dipasang (Algo ID: {:?}, Callback: {}%, Act: ~{:.2}).", symbol, aid, strat.trail_dist, act_price),
                    "INFO",
                ).await;
            }
            Err(e) => {
                self.log(
                    &format!("[{}] Gagal pasang Binance Trailing Stop: {} (Fallback ke Local).", symbol, e),
                    "WARN",
                ).await;
            }
        }
    }

    pub async fn close_active_position(&self, symbol: &str, reason: &str) {
        let pos = match self.active_positions.read().await.get(symbol).cloned() {
            Some(p) => p,
            None => return,
        };

        let close_side = if pos.side == "BUY" { "SELL" } else { "BUY" };
        self.log(&format!("🛑 [{}] Menutup Posisi {} {} ({})...", symbol, pos.side, pos.qty, reason), "INFO").await;

        match self.client.place_order(symbol, close_side, "MARKET", pos.qty, None, None, None, true).await {
            Ok(res) => {
                let exit_px_raw = res.get("avgPrice").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                let exit_px = if exit_px_raw > 0.0 {
                    exit_px_raw
                } else {
                    self.engines.read().await.get(symbol).and_then(|e| e.live_close).unwrap_or(pos.entry)
                };

                let pnl = if pos.side == "BUY" { (exit_px - pos.entry) * pos.qty } else { (pos.entry - exit_px) * pos.qty };
                let pct = if pos.entry > 0.0 {
                    if pos.side == "BUY" { (exit_px - pos.entry) / pos.entry * 100.0 } else { (pos.entry - exit_px) / pos.entry * 100.0 }
                } else {
                    0.0
                };

                let rec = TradeRecord {
                    id: None,
                    symbol: symbol.to_string(),
                    side: pos.side.clone(),
                    entry: pos.entry,
                    exit: exit_px,
                    price: exit_px,
                    qty: pos.qty,
                    pnl: (pnl * 100.0).round() / 100.0,
                    pct: (pct * 100.0).round() / 100.0,
                    reason: reason.to_string(),
                    time: chrono::Local::now().format("%H:%M:%S").to_string(),
                    raw_time: chrono::Utc::now().timestamp_millis(),
                };

                {
                    let mut hist = self.trade_history.write().await;
                    if hist.len() >= 100 {
                        hist.pop_back();
                    }
                    hist.push_front(rec);
                }

                if let Some(aid) = pos.algo_trailing_id {
                    let _ = self.client.cancel_algo_order(aid).await;
                }
                let _ = self.client.cancel_all_algo_orders(symbol).await;

                self.active_positions.write().await.remove(symbol);
                self.log(
                    &format!("🎉 [{}] Posisi ditutup ({}) @ {}. PnL: {:+.2} USDT ({:+.2}%)", symbol, reason, exit_px, pnl, pct),
                    "INFO",
                ).await;

                let (mode, last_tops) = {
                    (
                        self.config.read().await.coin_source_mode.clone(),
                        self.last_top_symbols.read().await.clone(),
                    )
                };
                if mode != "Manual" && !last_tops.contains(symbol) {
                    self.engines.write().await.remove(symbol);
                    self.restart_websocket().await;
                    self.log(&format!("[{}] Posisi ditutup & di luar screener top. Melepas engine (Graceful Demotion selesai).", symbol), "INFO").await;
                }
            }
            Err(e) => self.log(&format!("Gagal Tutup Posisi {}: {}", symbol, e), "ERROR").await,
        }
    }

    pub async fn evaluate_exits_live(&self, symbol: &str, price: f64) {
        let mut pos = match self.active_positions.write().await.get_mut(symbol).cloned() {
            Some(p) => p,
            None => return,
        };

        pos.highest_px = pos.highest_px.max(price);
        pos.lowest_px = pos.lowest_px.min(price);

        let diff_pct = if pos.entry > 0.0 {
            if pos.side == "BUY" { (price - pos.entry) / pos.entry * 100.0 } else { (pos.entry - price) / pos.entry * 100.0 }
        } else {
            0.0
        };
        pos.unrealized_pnl = if pos.side == "BUY" { (price - pos.entry) * pos.qty } else { (pos.entry - price) * pos.qty };

        let strat = self.get_effective_strategy(symbol).await;

        if strat.tp_type == "Fixed %" && diff_pct >= strat.tp_val {
            self.close_active_position(symbol, &format!("TP Fixed ({:+.2}%)", diff_pct)).await;
            return;
        }

        if strat.sl_type == "Fixed %" && diff_pct <= -strat.sl_val {
            self.close_active_position(symbol, &format!("SL Fixed ({:+.2}%)", diff_pct)).await;
            return;
        }

        if strat.tp_type == "Median %" || strat.sl_type == "Median %" {
            let (fav_m, adv_m) = {
                let engines = self.engines.read().await;
                if let Some(eng) = engines.get(symbol) {
                    let (mh, ml) = eng.get_median_excursion();
                    if pos.side == "BUY" { (mh, ml) } else { (ml, mh) }
                } else {
                    (None, None)
                }
            };

            if strat.tp_type == "Median %" {
                if let Some(fav) = fav_m {
                    let tp_target = fav * strat.tp_val;
                    if diff_pct >= tp_target {
                        self.close_active_position(symbol, &format!("TP Median ({:+.2}%)", diff_pct)).await;
                        return;
                    }
                }
            }

            if strat.sl_type == "Median %" {
                if let Some(adv) = adv_m {
                    let sl_target = adv * strat.sl_val;
                    if diff_pct <= -sl_target {
                        self.close_active_position(symbol, &format!("SL Median ({:+.2}%)", diff_pct)).await;
                        return;
                    }
                }
            }
        }

        if strat.use_trailing {
            if strat.trailing_mode == "Binance Endpoint" && pos.algo_trailing_id.is_some() {
                // Biarkan native trailing stop dieksekusi Binance
                if let Some(curr) = self.active_positions.write().await.get_mut(symbol) {
                    curr.highest_px = pos.highest_px;
                    curr.lowest_px = pos.lowest_px;
                    curr.unrealized_pnl = pos.unrealized_pnl;
                }
                return;
            }

            let (act_pct, dist_pct) = if strat.trail_type == "Median %" {
                let engines = self.engines.read().await;
                let (mh, ml) = engines.get(symbol).map(|e| e.get_median_excursion()).unwrap_or((Some(1.0), Some(1.0)));
                let fav = if pos.side == "BUY" { mh.unwrap_or(1.0) } else { ml.unwrap_or(1.0) };
                (fav * strat.trail_act, fav * strat.trail_dist)
            } else {
                (strat.trail_act, strat.trail_dist)
            };

            if diff_pct >= act_pct && !pos.trail_active {
                pos.trail_active = true;
                pos.trail_stop = Some(if pos.side == "BUY" {
                    price * (1.0 - dist_pct / 100.0)
                } else {
                    price * (1.0 + dist_pct / 100.0)
                });
                self.log(&format!("[{}] Trailing Stop Aktif @ {:.2}!", symbol, pos.trail_stop.unwrap()), "INFO").await;
            }

            if pos.trail_active {
                if pos.side == "BUY" {
                    let new_stop = price * (1.0 - dist_pct / 100.0);
                    pos.trail_stop = Some(pos.trail_stop.map(|s| s.max(new_stop)).unwrap_or(new_stop));
                    if let Some(stop) = pos.trail_stop {
                        if price <= stop {
                            self.close_active_position(symbol, &format!("TRAIL STOP ({:.2})", stop)).await;
                            return;
                        }
                    }
                } else {
                    let new_stop = price * (1.0 + dist_pct / 100.0);
                    pos.trail_stop = Some(pos.trail_stop.map(|s| s.min(new_stop)).unwrap_or(new_stop));
                    if let Some(stop) = pos.trail_stop {
                        if price >= stop {
                            self.close_active_position(symbol, &format!("TRAIL STOP ({:.2})", stop)).await;
                            return;
                        }
                    }
                }
            }
        }

        if let Some(curr) = self.active_positions.write().await.get_mut(symbol) {
            curr.highest_px = pos.highest_px;
            curr.lowest_px = pos.lowest_px;
            curr.unrealized_pnl = pos.unrealized_pnl;
            curr.trail_active = pos.trail_active;
            curr.trail_stop = pos.trail_stop;
        }
    }

    pub async fn evaluate_exits_on_candle_close(&self, symbol: &str, is_new_sr: bool, open_p: f64, close_p: f64) {
        let mut pos = match self.active_positions.write().await.get_mut(symbol).cloned() {
            Some(p) => p,
            None => return,
        };

        pos.bars_held += 1;
        let is_dir = if pos.side == "BUY" { close_p > open_p } else { close_p < open_p };
        let is_adv = if pos.side == "BUY" { close_p < open_p } else { close_p > open_p };

        if is_dir { pos.dir_cnt += 1; }
        if is_adv { pos.adv_cnt += 1; }

        let strat = self.get_effective_strategy(symbol).await;

        if strat.sl_type == "NextSR Only" && is_new_sr {
            self.close_active_position(symbol, "NextSR").await;
            return;
        }

        let net_profit = pos.unrealized_pnl > 0.0;
        let net_loss = pos.unrealized_pnl < 0.0;

        let b_tp_n = strat.tp_val as usize;
        let b_sl_n = strat.sl_val as usize;

        if strat.tp_type == "BarFix" && pos.bars_held >= b_tp_n {
            self.close_active_position(symbol, &format!("BarFix ({}b)", pos.bars_held)).await;
            return;
        }
        if strat.tp_type == "BarProf" && pos.bars_held >= b_tp_n && net_profit {
            self.close_active_position(symbol, &format!("BarProf ({}b)", pos.bars_held)).await;
            return;
        }
        if strat.tp_type == "BarDir" && pos.dir_cnt >= b_tp_n && net_profit {
            self.close_active_position(symbol, &format!("BarDir ({}c)", pos.dir_cnt)).await;
            return;
        }

        if strat.sl_type == "BarLoss" && pos.bars_held >= b_sl_n && net_loss {
            self.close_active_position(symbol, &format!("BarLoss ({}b)", pos.bars_held)).await;
            return;
        }
        if strat.sl_type == "MaxBar" && pos.bars_held >= b_sl_n {
            self.close_active_position(symbol, &format!("MaxBar Timeout ({}b)", pos.bars_held)).await;
            return;
        }
        if strat.sl_type == "BarAdv" && pos.adv_cnt >= b_sl_n && net_loss {
            self.close_active_position(symbol, &format!("BarAdv ({}c)", pos.adv_cnt)).await;
            return;
        }

        if let Some(curr) = self.active_positions.write().await.get_mut(symbol) {
            curr.bars_held = pos.bars_held;
            curr.dir_cnt = pos.dir_cnt;
            curr.adv_cnt = pos.adv_cnt;
        }
    }

    pub async fn on_new_confirmed_sr(&self, symbol: &str, candle_open_time: i64, close_price: f64) {
        let dir_opt = self.engines.read().await.get(symbol).and_then(|e| e.cur_dir.clone());
        if let Some(dir) = dir_opt {
            self.log(&format!("🎯 [{}] Signal SR Terkonfirmasi: {} @ {:.2}", symbol, dir, close_price), "INFO").await;
            self.execute_entry(symbol, &dir, close_price, candle_open_time).await;
        }
    }

    // -----------------------------------------------------------------
    // WORKER WEBSOCKET REAL-TIME (AGGTRADE + KLINE)
    // -----------------------------------------------------------------
    pub async fn websocket_worker(self: Arc<Self>) {
        let mut endpoint_idx = 0;

        while self.is_running.load(Ordering::SeqCst) {
            let (symbols, interval, interval_ms) = {
                let engines = self.engines.read().await;
                let cfg = self.config.read().await;
                let s_list: Vec<String> = engines.keys().map(|s| s.to_lowercase()).collect();
                (s_list, cfg.timeframe.clone(), get_interval_ms(&cfg.timeframe))
            };

            if symbols.is_empty() {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                continue;
            }

            let mut streams = Vec::new();
            for s in &symbols {
                streams.push(format!("{}@aggTrade", s));
                streams.push(format!("{}@kline_{}", s, interval));
            }

            let ws_endpoint = WSS_ENDPOINTS[endpoint_idx % WSS_ENDPOINTS.len()];
            endpoint_idx += 1;

            match connect_async(ws_endpoint).await {
                Ok((mut ws_stream, _)) => {
                    self.log(&format!("WSS Real-Time Live terhubung ({} koin, {})", symbols.len(), ws_endpoint), "INFO").await;

                    let sub_payload = serde_json::json!({
                        "method": "SUBSCRIBE",
                        "params": streams,
                        "id": 1
                    });

                    if let Err(e) = ws_stream.send(tokio_tungstenite::tungstenite::Message::Text(sub_payload.to_string().into())).await {
                        self.log(&format!("WSS Subscribe error: {}", e), "WARN").await;
                        continue;
                    }

                    loop {
                        tokio::select! {
                            _ = self.ws_restart_notify.notified() => {
                                self.log("WSS restart diminta (koin diperbarui). Menutup koneksi lama...", "INFO").await;
                                let _ = ws_stream.close(None).await;
                                break;
                            }
                            msg_opt = ws_stream.next() => {
                                match msg_opt {
                                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(txt))) => {
                                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                                            if v.get("result").is_some() && v.get("id").and_then(|x| x.as_i64()) == Some(1) {
                                                continue;
                                            }

                                            let data = v.get("data").unwrap_or(&v);
                                            let event = data.get("e").and_then(|x| x.as_str()).unwrap_or("");
                                            let sym = data.get("s").and_then(|x| x.as_str()).unwrap_or("").to_uppercase();

                                            if sym.is_empty() { continue; }

                                            if event == "aggTrade" {
                                                let p = data.get("p").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                                                let raw_p = data.get("p").and_then(|x| x.as_str());
                                                let t_ts = data.get("T").and_then(|x| x.as_i64()).unwrap_or(0);

                                                let (is_sniping_candidate, candidate_dir) = {
                                                    let mut engines = self.engines.write().await;
                                                    if let Some(eng) = engines.get_mut(&sym) {
                                                        eng.update_live_tick(p, None, None, None, None, None, raw_p, Some(t_ts));
                                                        (eng.next_sr_active, eng.next_sr_dir.clone())
                                                    } else {
                                                        (false, None)
                                                    }
                                                };

                                                let now_ms = self.client.get_server_time_ms().await;
                                                let expected_open = (now_ms / interval_ms) * interval_ms;
                                                let expected_close = expected_open + interval_ms;
                                                let rem_ms = (expected_close - now_ms).max(0);

                                                let entry_mode = self.config.read().await.entry_mode.clone();

                                                if entry_mode.contains("Sniping") && rem_ms > 0 && rem_ms <= 2500 && is_sniping_candidate {
                                                    if let Some(dir) = candidate_dir {
                                                        let already_entered = self.last_entered_candle.read().await.get(&sym).copied() == Some(expected_open);
                                                        if !already_entered {
                                                            self.log(&format!("🎯 [{}] SNIPING TRIGGERED (:59s) pada calon {} (sisa {}ms)!", sym, dir, rem_ms), "INFO").await;
                                                            let this = self.clone();
                                                            let sym_c = sym.clone();
                                                            tokio::spawn(async move {
                                                                this.execute_entry(&sym_c, &dir, p, expected_open).await;
                                                            });
                                                        }
                                                    }
                                                }

                                                self.evaluate_exits_live(&sym, p).await;

                                            } else if event == "kline" {
                                                if let Some(k) = data.get("k") {
                                                    let o = k.get("o").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                                                    let h = k.get("h").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                                                    let l = k.get("l").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                                                    let c = k.get("c").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                                                    let raw_c = k.get("c").and_then(|x| x.as_str());
                                                    let is_closed = k.get("x").and_then(|x| x.as_bool()).unwrap_or(false);
                                                    let open_t = k.get("t").and_then(|x| x.as_i64()).unwrap_or(0);
                                                    let close_t = k.get("T").and_then(|x| x.as_i64()).unwrap_or(0);

                                                    if is_closed {
                                                        let is_new_sr = {
                                                            let mut engines = self.engines.write().await;
                                                            if let Some(eng) = engines.get_mut(&sym) {
                                                                eng.process_closed_candle(o, h, l, c, raw_c, Some(open_t), Some(close_t), Some(interval_ms))
                                                            } else {
                                                                false
                                                            }
                                                        };

                                                        if is_new_sr {
                                                            self.on_new_confirmed_sr(&sym, open_t, c).await;
                                                        }

                                                        self.evaluate_exits_on_candle_close(&sym, is_new_sr, o, c).await;
                                                    } else {
                                                        let mut engines = self.engines.write().await;
                                                        if let Some(eng) = engines.get_mut(&sym) {
                                                            if open_t == eng.candle_open_time || eng.candle_open_time <= eng.last_closed_open_time {
                                                                eng.update_live_tick(c, Some(h), Some(l), Some(o), Some(open_t), Some(close_t), raw_c, None);
                                                            }
                                                        }
                                                        self.evaluate_exits_live(&sym, c).await;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                                        break;
                                    }
                                    Some(Err(e)) => {
                                        self.log(&format!("WSS read error: {}", e), "WARN").await;
                                        break;
                                    }
                                    None => break,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    self.log(&format!("WSS Connect error: {}", e), "WARN").await;
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
        }
    }

    // -----------------------------------------------------------------
    // WORKER FAPI RECONCILIATION FALLBACK
    // -----------------------------------------------------------------
    pub async fn fapi_live_fallback_worker(self: Arc<Self>) {
        while self.is_running.load(Ordering::SeqCst) {
            let (interval, interval_ms, num_coins) = {
                let cfg = self.config.read().await;
                (cfg.timeframe.clone(), get_interval_ms(&cfg.timeframe), self.engines.read().await.len())
            };
            let poll_ms = (350.max((num_coins as u64) * 250)).min(3000);
            tokio::time::sleep(tokio::time::Duration::from_millis(poll_ms)).await;

            let syms: Vec<String> = self.engines.read().await.keys().cloned().collect();
            let now_ms = self.client.get_server_time_ms().await;
            let expected_current_open = (now_ms / interval_ms) * interval_ms;
            let expected_prev_open = expected_current_open - interval_ms;

            for sym in syms {
                let missed_close = {
                    let engines = self.engines.read().await;
                    engines.get(&sym).map(|e| e.last_closed_open_time < expected_prev_open && (now_ms - expected_current_open) >= 1200).unwrap_or(false)
                };

                if !missed_close { continue; }

                if let Ok(klines) = self.client.fetch_klines(&sym, &interval, 3).await {
                    let n = klines.len();
                    if n > 1 {
                        for k in &klines[..n - 1] {
                            let k_open = k.get(0).and_then(|x| x.as_i64()).unwrap_or(0);
                            let k_close = k.get(6).and_then(|x| x.as_i64()).unwrap_or(0);
                            let c_val = k.get(4).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                            let o_val = k.get(1).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                            let h_val = k.get(2).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                            let l_val = k.get(3).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                            let raw_c = k.get(4).and_then(|x| x.as_str());

                            let is_new_sr = {
                                let mut engines = self.engines.write().await;
                                if let Some(eng) = engines.get_mut(&sym) {
                                    if k_open > eng.last_closed_open_time && k_close <= now_ms {
                                        eng.process_closed_candle(o_val, h_val, l_val, c_val, raw_c, Some(k_open), Some(k_close), Some(interval_ms))
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            };

                            if is_new_sr {
                                self.log(&format!("⚡ [{}] Step SR (via Rekonsiliasi)", sym), "INFO").await;
                                self.on_new_confirmed_sr(&sym, k_open, c_val).await;
                                self.evaluate_exits_on_candle_close(&sym, is_new_sr, o_val, c_val).await;
                            }
                        }
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // WORKER TIMEOUT ORDER LIMIT
    // -----------------------------------------------------------------
    pub async fn order_timeout_cleaner(self: Arc<Self>) {
        while self.is_running.load(Ordering::SeqCst) {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let timeout_sec = self.config.read().await.limit_timeout_sec;
            let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;

            let mut to_del = Vec::new();
            {
                let orders = self.pending_sniping_orders.read().await;
                for (sym, o) in orders.iter() {
                    if now - o.time >= timeout_sec {
                        let _ = self.client.cancel_order(sym, o.order_id).await;
                        self.log(&format!("[{}] Limit order ID {} dibatalkan (Timeout {}s).", sym, o.order_id, timeout_sec), "INFO").await;
                        to_del.push(sym.clone());
                    }
                }
            }

            let mut orders_w = self.pending_sniping_orders.write().await;
            for sym in to_del {
                orders_w.remove(&sym);
            }
        }
    }

    // -----------------------------------------------------------------
    // WORKER SINKRONISASI BURSA (EXCHANGE SYNC)
    // -----------------------------------------------------------------
    pub async fn exchange_sync_worker(self: Arc<Self>) {
        while self.is_running.load(Ordering::SeqCst) {
            tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;
            self.sync_active_positions_from_exchange().await;
            self.sync_trades_from_exchange().await;
        }
    }

    // -----------------------------------------------------------------
    // WORKER DYNAMIC SCREENER (DENGAN GRACEFUL DEMOTION)
    // -----------------------------------------------------------------
    pub async fn dynamic_screener_worker(self: Arc<Self>) {
        while self.is_running.load(Ordering::SeqCst) {
            let refresh_min = self.config.read().await.screener_refresh_min;
            tokio::time::sleep(tokio::time::Duration::from_secs(10.max(refresh_min * 60))).await;

            let mode = self.config.read().await.coin_source_mode.clone();
            if mode == "Manual" {
                continue;
            }

            let new_top_symbols: HashSet<String> = self.resolve_active_symbols().await.into_iter().collect();
            *self.last_top_symbols.write().await = new_top_symbols.clone();

            let current_symbols: HashSet<String> = self.engines.read().await.keys().cloned().collect();
            let active_pos_symbols: HashSet<String> = self.active_positions.read().await.keys().cloned().collect();

            // Graceful Demotion: Koin yang dipertahankan = Top N baru + Posisi aktif saat ini
            let needed_symbols: HashSet<String> = new_top_symbols.union(&active_pos_symbols).cloned().collect();

            let to_remove: Vec<String> = current_symbols.difference(&needed_symbols).cloned().collect();
            let to_add: Vec<String> = new_top_symbols.difference(&current_symbols).cloned().collect();

            let mut changed = false;

            for sym in to_remove {
                self.log(&format!("[{}] Demoted dari screener (tidak ada posisi aktif). Melepas engine.", sym), "INFO").await;
                self.pending_sniping_orders.write().await.remove(&sym);
                self.engines.write().await.remove(&sym);
                changed = true;
            }

            if !to_add.is_empty() {
                let (timeframe, limit) = {
                    let cfg = self.config.read().await;
                    (cfg.timeframe.clone(), cfg.warmup_limit)
                };

                for sym in to_add {
                    self.log(&format!("[{}] Masuk ranking screener ({}). Menginisialisasi engine & warmup...", sym, mode), "INFO").await;
                    let cfg = self.config.read().await.clone();
                    let eng = VolStepEngine::new(
                        sym.clone(),
                        cfg.line_mode,
                        cfg.vol_type,
                        cfg.lookback,
                        cfg.vol_mult,
                        cfg.stdev_bessel,
                    );
                    self.engines.write().await.insert(sym.clone(), eng);
                    self.warmup_single_symbol(&sym, limit, &timeframe).await;
                    changed = true;
                }
            }

            if changed {
                self.log(
                    &format!("🔄 Screener diperbarui ({}): {} koin aktif. Menyelaraskan WebSocket stream...", mode, self.engines.read().await.len()),
                    "INFO",
                ).await;
                self.restart_websocket().await;
            }
        }
    }

    pub async fn run(self: Arc<Self>) {
        self.is_running.store(true, Ordering::SeqCst);
        self.init_engines().await;
        self.warmup_all().await;

        let b1 = self.clone();
        let b2 = self.clone();
        let b3 = self.clone();
        let b4 = self.clone();
        let b5 = self.clone();

        tokio::select! {
            _ = b1.websocket_worker() => {},
            _ = b2.fapi_live_fallback_worker() => {},
            _ = b3.exchange_sync_worker() => {},
            _ = b4.order_timeout_cleaner() => {},
            _ = b5.dynamic_screener_worker() => {},
        }

        self.is_running.store(false, Ordering::SeqCst);
        *self.status.write().await = "STOPPED".to_string();
        self.log("Bot dihentikan.", "INFO").await;
    }

    pub async fn start(self: Arc<Self>) {
        if self.is_running.load(Ordering::SeqCst) {
            return;
        }
        self.is_running.store(true, Ordering::SeqCst);
        *self.status.write().await = "STARTING".to_string();
        self.log("Memulai bot di background tokio runtime...", "INFO").await;

        let this = self.clone();
        tokio::spawn(async move {
            this.run().await;
        });
    }

    pub async fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        *self.status.write().await = "STOPPED".to_string();
        self.log("Permintaan stop bot diterima.", "INFO").await;
        self.ws_restart_notify.notify_waiters();
    }
}
