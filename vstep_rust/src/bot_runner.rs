use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tungstenite::connect;

use crate::binance_api::{
    fetch_specific_symbols_tickers, fetch_top_screened_coins, get_live_positions, get_max_leverage, market_close_position,
    market_open_position, panic_close_all_positions, set_symbol_leverage, test_testnet_account,
    AccountBalanceInfo, LivePositionInfo,
};
use crate::credentials::load_credentials;
use crate::engine::{calculate_step_sr, calculate_volatility, create_sim_suite, median, run_backtest};
use crate::fetcher::fetch_binance_fapi_klines_by_count;

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

/// Replikasi presisi VolStepEngine dari vstep.py (100% identik dengan TradingView / Pine Script)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VolStepEngine {
    pub symbol: String,
    pub line_mode: String,       // "Anchor", "Without Anchor"
    pub vol_type: String,        // "Stdev", "Atr"
    pub length: usize,           // default 200
    pub vol_mult: f64,           // default 4.0

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

    pub live_open: Option<f64>,
    pub live_high: Option<f64>,
    pub live_low: Option<f64>,
    pub live_close: Option<f64>,
    pub tick_dir: i32,           // 1 = naik, -1 = turun, 0 = tetap
    pub candle_open_time: i64,
    pub candle_close_time: i64,

    pub pending_tick: Option<(f64, String, i64)>,

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
    ) -> Self {
        let len = length.max(2);
        Self {
            symbol: symbol.to_uppercase(),
            line_mode,
            vol_type,
            length: len,
            vol_mult,
            closed_closes: VecDeque::with_capacity(len + 1),
            tr_warmup: Vec::with_capacity(len + 1),
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
        if raw_str.is_empty() || !raw_str.contains('.') {
            return;
        }
        let trimmed = raw_str.trim_end_matches('0');
        if let Some(pos) = trimmed.find('.') {
            let dec = trimmed[pos + 1..].len();
            if dec > self.price_decimals {
                self.price_decimals = dec.min(8);
            }
        }
    }

    pub fn fmt(&self, val: Option<f64>) -> String {
        match val {
            None => "-".to_string(),
            Some(v) => format!("{:.prec$}", v, prec = self.price_decimals),
        }
    }

    fn calc_stdev_manual(values: &[f64]) -> Option<f64> {
        let n = values.len();
        if n == 0 {
            return None;
        }
        let mean = values.iter().sum::<f64>() / (n as f64);
        let variance = values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (n as f64);
        Some(variance.sqrt())
    }

    fn calc_true_range(high: f64, low: f64, prev_close: Option<f64>) -> f64 {
        match prev_close {
            None => high - low,
            Some(pc) => (high - low).max((high - pc).abs()).max((low - pc).abs()),
        }
    }

    pub fn compute_volatility_closed(&mut self, high: f64, low: f64, close: f64) -> Option<f64> {
        let tr = Self::calc_true_range(high, low, self.prev_closed_close);
        self.prev_closed_close = Some(close);
        if self.closed_closes.len() == self.length {
            self.closed_closes.pop_front();
        }
        self.closed_closes.push_back(close);
        self.closed_bars_count += 1;

        let len_f = self.length as f64;
        if self.closed_bars_count < self.length {
            self.tr_warmup.push(tr);
            self.confirmed_rma = None;
        } else if self.closed_bars_count == self.length {
            self.tr_warmup.push(tr);
            self.confirmed_rma = Some(self.tr_warmup.iter().sum::<f64>() / len_f);
        } else if let Some(rma) = self.confirmed_rma {
            self.confirmed_rma = Some((tr + (len_f - 1.0) * rma) / len_f);
        }

        if self.closed_bars_count < self.length {
            return None;
        }

        let raw_vol = if self.vol_type == "Stdev" {
            let slice: Vec<f64> = self.closed_closes.iter().copied().collect();
            Self::calc_stdev_manual(&slice)
        } else {
            self.confirmed_rma
        };

        raw_vol.map(|v| v * self.vol_mult)
    }

    pub fn compute_volatility_live(&self, high: f64, low: f64, close: f64) -> Option<f64> {
        if self.closed_bars_count < self.length.saturating_sub(1) {
            return None;
        }

        let len_f = self.length as f64;
        let raw_vol = if self.vol_type == "Stdev" {
            let needed = if self.length > 1 { self.length - 1 } else { 0 };
            let count = self.closed_closes.len();
            let start = if count >= needed { count - needed } else { 0 };
            let mut window: Vec<f64> = self.closed_closes.iter().skip(start).copied().collect();
            window.push(close);
            if window.len() < self.length {
                return None;
            }
            Self::calc_stdev_manual(&window)
        } else {
            let live_tr = Self::calc_true_range(high, low, self.prev_closed_close);
            if self.closed_bars_count == self.length - 1 {
                Some((self.tr_warmup.iter().sum::<f64>() + live_tr) / len_f)
            } else if let Some(rma) = self.confirmed_rma {
                Some((live_tr + (len_f - 1.0) * rma) / len_f)
            } else {
                None
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
    ) {
        if let Some(ot) = open_time {
            if ot <= self.last_closed_open_time {
                return;
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

        let cur_sr = self.confirmed_sr.unwrap_or(close_p);
        let new_level = if let Some(vv) = vol_value {
            let step_level = if self.line_mode == "Anchor" {
                close_p
            } else {
                let direction = if close_p > cur_sr { 1.0 } else { -1.0 };
                cur_sr + direction * vv
            };

            if (close_p - cur_sr).abs() > vv {
                step_level
            } else {
                cur_sr
            }
        } else {
            cur_sr
        };

        // barstate.isconfirmed: kunci newLevel ke confirmedSr (SR Saat Ini)
        self.confirmed_sr = Some(new_level);
        let active_sr = new_level;

        let is_price_step = match self.cur_level {
            None => true,
            Some(curr) => active_sr != curr,
        };

        if is_price_step {
            if let Some(curr) = self.cur_level {
                self.prev_level = Some(curr);
            }
            self.cur_level = Some(active_sr);

            if let Some(prev) = self.prev_level {
                if prev != 0.0 {
                    let pct = ((active_sr - prev) / prev) * 100.0;
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
        }

        // Reset SR Next karena bar sudah resmi close dan pindah ke SR Saat Ini
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

            if let Some((p_price, p_raw, p_ts)) = self.pending_tick.take() {
                if p_ts >= next_open {
                    self.update_live_tick(
                        p_price,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(&p_raw),
                        Some(p_ts),
                    );
                }
            }
        }
    }

    pub fn evaluate_live_sr(&mut self) {
        let (h, l, c) = match (self.live_high, self.live_low, self.live_close) {
            (Some(h), Some(l), Some(c)) => (h, l, c),
            _ => return,
        };

        let vol_value = self.compute_volatility_live(h, l, c);
        self.live_vol_value = vol_value;

        let (vv, conf_sr) = match (vol_value, self.confirmed_sr) {
            (Some(v), Some(sr)) => (v, sr),
            _ => {
                self.next_sr_active = false;
                return;
            }
        };

        let diff = c - conf_sr;
        if diff.abs() > vv {
            self.next_sr_active = true;
            self.next_sr_dir = Some(if diff > 0.0 { "BULL".to_string() } else { "BEAR".to_string() });
            let next_price = if self.line_mode == "Anchor" {
                c
            } else {
                let direction = if diff > 0.0 { 1.0 } else { -1.0 };
                conf_sr + direction * vv
            };
            self.next_sr_price = Some(next_price);
            self.next_sr_pct = if conf_sr != 0.0 {
                Some(((next_price - conf_sr) / conf_sr) * 100.0)
            } else {
                Some(0.0)
            };
        } else {
            self.next_sr_active = false;
            self.next_sr_dir = None;
            self.next_sr_price = None;
            self.next_sr_pct = None;
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

        // GUARD: Jika ini tick aggTrade yang timestamp-nya sudah masuk ke bar baru
        // sedangkan paket kline x=True belum tiba, tahan di pending_tick
        if let Some(tt) = trade_time {
            if self.candle_close_time > 0 && tt > self.candle_close_time {
                self.pending_tick = Some((price, raw_price_str.unwrap_or("").to_string(), tt));
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
                    self.live_high = Some(self.live_high.map(|v| v.max(hp)).unwrap_or(hp));
                }
                if let Some(lp) = low_p {
                    self.live_low = Some(self.live_low.map(|v| v.min(lp)).unwrap_or(lp));
                }
            }
        } else {
            if self.live_open.is_none() {
                self.live_open = Some(price);
            }
            self.live_high = Some(self.live_high.map(|v| v.max(price)).unwrap_or(price));
            self.live_low = Some(self.live_low.map(|v| v.min(price)).unwrap_or(price));
        }

        if let Some(cur_c) = self.live_close {
            if price != cur_c {
                self.tick_dir = if price > cur_c { 1 } else { -1 };
            }
        }
        self.live_close = Some(price);

        self.evaluate_live_sr();
    }

    pub fn cur_level_str(&self) -> String {
        self.fmt(self.cur_level)
    }

    pub fn next_sr_price_str(&self) -> String {
        self.fmt(self.next_sr_price)
    }
}

impl Default for VolStepEngine {
    fn default() -> Self {
        Self::new(
            "BTCUSDT".to_string(),
            "Anchor".to_string(),
            "Atr".to_string(),
            200,
            4.0,
        )
    }
}

pub fn format_price_clean(val: f64) -> String {
    if val >= 1000.0 {
        format!("{:.2}", val)
    } else if val >= 1.0 {
        format!("{:.4}", val)
    } else if val > 0.0 {
        format!("{:.6}", val)
    } else {
        "0.00".to_string()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScannedCoinState {
    pub symbol: String,
    pub price: f64,
    pub price_str: String,
    pub change_24h: f64,
    pub volume_24h: f64,
    pub is_active: bool,
    pub current_sr: f64,
    pub current_sr_str: String,
    pub breakout_buy: f64,
    pub breakout_buy_str: String,
    pub breakout_sell: f64,
    pub breakout_sell_str: String,
    pub dist_pct: f64,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub time: String,
    pub level: String, // "info", "success", "warning", "error", "trade"
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveTradeState {
    pub symbol: String,
    pub side: String, // "BUY" or "SELL"
    pub entry_price: f64,
    pub quantity: f64,
    pub entry_time_ms: i64,

    pub tp_price: Option<f64>,
    pub sl_price: Option<f64>,
    pub tp_desc: String,
    pub sl_desc: String,

    pub use_trailing: bool,
    pub tr_act_pct: f64,
    pub tr_dist_pct: f64,
    pub tr_extreme: f64,
    pub tr_stop_price: Option<f64>,

    pub tp_bar_type: i32,   // 0 none, 3 BarFix, 4 BarProf, 5 BarDir
    pub tp_bar_target: i32,
    pub sl_bar_type: i32,   // 0 none, 3 BarLoss, 4 MaxBar, 5 BarAdv
    pub sl_bar_target: i32,
    pub bars_held: i32,
    pub dir_candle_count: i32,
    pub adv_candle_count: i32,

    pub sl_next_sr: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveSimConfig {
    pub name: String,
    pub key: String,
    pub mode: i32, // 0 Hedge, 1 Trend, 2 Counter
    pub tp_t: i32, // 0 none, 1 med, 2 fix, 3 BarFix, 4 BarProf, 5 BarDir
    pub tp_v: f64,
    pub sl_t: i32, // 0 NextSR, 1 med, 2 fix, 3 BarLoss, 4 MaxBar, 5 BarAdv
    pub sl_v: f64,
    pub use_tr: bool,
    pub tr_u: i32, // 1 med, 2 fix
    pub tr_a: f64,
    pub tr_d: f64,
}

impl ActiveSimConfig {
    pub fn default_trend() -> Self {
        Self {
            name: "TREND | TP BarProf B1 | SL BarLoss S1".to_string(),
            key: "TREND | TP BarProf B1 | SL BarLoss S1".to_string(),
            mode: 1,
            tp_t: 4,
            tp_v: 3.0,
            sl_t: 3,
            sl_v: 4.0,
            use_tr: false,
            tr_u: 1,
            tr_a: 0.0,
            tr_d: 0.0,
        }
    }
}

pub fn resolve_sim_config(chosen_strat: &str, cfg: &BotConfig) -> ActiveSimConfig {
    let suite = create_sim_suite(
        true, true, true, true, true,
        cfg.fixed1, cfg.fixed2, cfg.fixed3,
        cfg.bar_tp1, cfg.bar_tp2, cfg.bar_tp_dir,
        cfg.sl_bar1, cfg.sl_bar2, cfg.sl_bar_adv,
        1000.0,
        chosen_strat,
    );

    if let Some(s) = suite.iter().find(|s| s.key == chosen_strat || s.name == chosen_strat || chosen_strat.contains(&s.key) || s.key.contains(chosen_strat)) {
        return ActiveSimConfig {
            name: s.name.clone(),
            key: s.key.clone(),
            mode: s.mode,
            tp_t: s.tp_t,
            tp_v: s.tp_v,
            sl_t: s.sl_t,
            sl_v: s.sl_v,
            use_tr: s.use_tr,
            tr_u: s.tr_u,
            tr_a: s.tr_a,
            tr_d: s.tr_d,
        };
    }

    // Fallback parser if not found in suite
    let mode = if chosen_strat.starts_with("COUNTER") { 2 } else if chosen_strat.starts_with("HEDGE") { 0 } else { 1 };

    let (tp_t, tp_v) = if chosen_strat.contains("TP 0.5M") {
        (1, 0.5)
    } else if chosen_strat.contains("TP 1M") {
        (1, 1.0)
    } else if chosen_strat.contains("TP F1") {
        (2, cfg.fixed1)
    } else if chosen_strat.contains("TP F2") {
        (2, cfg.fixed2)
    } else if chosen_strat.contains("TP F3") {
        (2, cfg.fixed3)
    } else if chosen_strat.contains("BarFix B1") {
        (3, cfg.bar_tp1 as f64)
    } else if chosen_strat.contains("BarFix B2") {
        (3, cfg.bar_tp2 as f64)
    } else if chosen_strat.contains("BarProf B1") {
        (4, cfg.bar_tp1 as f64)
    } else if chosen_strat.contains("BarProf B2") {
        (4, cfg.bar_tp2 as f64)
    } else if chosen_strat.contains("BarDir") {
        (5, cfg.bar_tp_dir as f64)
    } else {
        (0, 0.0)
    };

    let (sl_t, sl_v) = if chosen_strat.contains("SL 0.5M") {
        (1, 0.5)
    } else if chosen_strat.contains("SL 1M") {
        (1, 1.0)
    } else if chosen_strat.contains("SL F1") {
        (2, cfg.fixed1)
    } else if chosen_strat.contains("SL F2") {
        (2, cfg.fixed2)
    } else if chosen_strat.contains("BarLoss S1") {
        (3, cfg.sl_bar1 as f64)
    } else if chosen_strat.contains("BarLoss S2") {
        (3, cfg.sl_bar2 as f64)
    } else if chosen_strat.contains("MaxBar S1") {
        (4, cfg.sl_bar1 as f64)
    } else if chosen_strat.contains("MaxBar S2") {
        (4, cfg.sl_bar2 as f64)
    } else if chosen_strat.contains("BarAdv SA") {
        (5, cfg.sl_bar_adv as f64)
    } else {
        (0, 0.0) // NextSR
    };

    let (use_tr, tr_u, tr_a, tr_d) = if chosen_strat.contains("Trail 0.5M/0.5M") {
        (true, 1, 0.5, 0.5)
    } else if chosen_strat.contains("Trail 1M/0.5M") {
        (true, 1, 1.0, 0.5)
    } else if chosen_strat.contains("Trail F2/F1") {
        (true, 2, cfg.fixed2, cfg.fixed1)
    } else {
        (false, 1, 0.0, 0.0)
    };

    ActiveSimConfig {
        name: chosen_strat.to_string(),
        key: chosen_strat.to_string(),
        mode,
        tp_t,
        tp_v,
        sl_t,
        sl_v,
        use_tr,
        tr_u,
        tr_a,
        tr_d,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BotConfig {
    pub is_running: bool,

    // 1. Line & Volatility Settings (Pine Script Engine)
    pub line_mode: String,          // "Anchor", "Without Anchor"
    pub volatility_type: String,    // "Stdev", "Atr"
    pub volatility_length: usize,   // 200
    pub vol_mult: f64,              // 4.0
    pub execution_mode: String,     // "realtime_no_lag", "pre_close", "on_close"
    pub pre_close_buffer_ms: u64,   // 1000 ms

    // 2. Mode Koin & Screener
    pub coin_mode: String,          // "top_gainers", "top_volume", "manual"
    pub manual_symbol: String,      // "BTCUSDT"
    pub manual_symbols: Vec<String>, // ["BTCUSDT", "ETHUSDT"]
    pub interval: String,           // "15m", "1h", etc.
    pub max_active_slots: usize,    // 5 slot
    pub warmup_klines: usize,       // 10000 kline default

    // 3. Strategy Finder Settings (Persis seperti Backtest)
    pub strategy_mode: String,      // "best_bar2", "best_overall", "best_btp", "best_bsl", "best_fix", "best_med", "manual"
    pub manual_strategy_key: String,
    pub inc_trend: bool,
    pub inc_counter: bool,
    pub inc_hedge: bool,
    pub inc_bar_tp: bool,
    pub inc_bar_sl: bool,
    pub fixed1: f64,
    pub fixed2: f64,
    pub fixed3: f64,
    pub bar_tp1: i32,
    pub bar_tp2: i32,
    pub bar_tp_dir: i32,
    pub sl_bar1: i32,
    pub sl_bar2: i32,
    pub sl_bar_adv: i32,
    pub min_levels: usize,
    pub rank_by: String,
    pub one_way_rule: String,

    // 4. Leverage & Alokasi Margin
    pub leverage_mode: String,      // "auto_max", "manual"
    pub manual_leverage: i32,
    pub margin_pct_per_slot: f64,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            is_running: false,
            line_mode: "Anchor".to_string(),
            volatility_type: "Atr".to_string(),
            volatility_length: 200,
            vol_mult: 4.0,
            execution_mode: "realtime_no_lag".to_string(),
            pre_close_buffer_ms: 1000,

            coin_mode: "top_gainers".to_string(),
            manual_symbol: "BTCUSDT".to_string(),
            manual_symbols: vec!["BTCUSDT".to_string()],
            interval: "1m".to_string(),
            max_active_slots: 5,
            warmup_klines: 10000,

            strategy_mode: "best_bar2".to_string(),
            manual_strategy_key: "TREND | TP BarProf B1 | SL BarLoss S1".to_string(),
            inc_trend: true,
            inc_counter: true,
            inc_hedge: false,
            inc_bar_tp: true,
            inc_bar_sl: true,
            fixed1: 0.3,
            fixed2: 0.6,
            fixed3: 1.0,
            bar_tp1: 3,
            bar_tp2: 6,
            bar_tp_dir: 3,
            sl_bar1: 4,
            sl_bar2: 8,
            sl_bar_adv: 3,
            min_levels: 10,
            rank_by: "Net / Max DD".to_string(),
            one_way_rule: "Tutup Posisi Lama & Buka Baru di SR Baru".to_string(),

            leverage_mode: "auto_max".to_string(),
            manual_leverage: 10,
            margin_pct_per_slot: 4.0,
        }
    }
}

pub struct BotState {
    pub config: BotConfig,
    pub logs: Vec<LogEntry>,
    pub cached_balance: Option<AccountBalanceInfo>,
    pub cached_positions: Vec<LivePositionInfo>,
    pub last_warmup_symbol: String,
    pub last_selected_strategy: String,
    pub active_sim: Option<ActiveSimConfig>,
    pub current_price: f64,
    pub current_price_str: String,
    pub current_sr: f64,
    pub current_sr_str: String,
    pub current_vol: f64,
    pub current_vol_str: String,
    pub med_h: f64,
    pub med_l: f64,
    pub active_leverage: i32,
    pub last_evaluated_bar: i64,
    pub last_intrabar_signal_bar: i64,
    pub last_intrabar_signal_dir: Option<String>,
    pub position_bars_held: i32,
    pub active_trade: Option<ActiveTradeState>,
    pub scanned_coins: Vec<ScannedCoinState>,
    pub engine: VolStepEngine,
}

impl BotState {
    pub fn new() -> Self {
        let mut initial_logs = Vec::with_capacity(100);
        initial_logs.push(LogEntry {
            time: Utc::now().format("%H:%M:%S").to_string(),
            level: "info".to_string(),
            message: "VolStep Demo Bot System diinisialisasi. Siap dengan WebSocket streaming & Pre-Close Sniping.".to_string(),
        });

        Self {
            config: BotConfig::default(),
            logs: initial_logs,
            cached_balance: None,
            cached_positions: Vec::new(),
            last_warmup_symbol: String::new(),
            last_selected_strategy: String::new(),
            active_sim: None,
            current_price: 0.0,
            current_price_str: "-".to_string(),
            current_sr: 0.0,
            current_sr_str: "-".to_string(),
            current_vol: 0.0,
            current_vol_str: "-".to_string(),
            med_h: 1.0,
            med_l: 1.0,
            active_leverage: 10,
            last_evaluated_bar: 0,
            last_intrabar_signal_bar: 0,
            last_intrabar_signal_dir: None,
            position_bars_held: 0,
            active_trade: None,
            scanned_coins: Vec::new(),
            engine: VolStepEngine::default(),
        }
    }

    pub fn add_log(&mut self, level: &str, msg: &str) {
        let entry = LogEntry {
            time: Utc::now().format("%H:%M:%S").to_string(),
            level: level.to_string(),
            message: msg.to_string(),
        };
        self.logs.push(entry);
        if self.logs.len() > 150 {
            self.logs.remove(0);
        }
    }
}

#[derive(Clone)]
pub struct SharedBotState(pub Arc<Mutex<BotState>>);

impl SharedBotState {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(BotState::new()));
        let shared = SharedBotState(state);
        shared.spawn_background_worker();
        shared.spawn_websocket_worker();
        shared
    }

    pub fn start(&self, mut new_cfg: BotConfig) -> Result<(), String> {
        if new_cfg.manual_symbol.trim().is_empty() && !new_cfg.manual_symbols.is_empty() {
            new_cfg.manual_symbol = new_cfg.manual_symbols[0].clone();
        }
        let creds = load_credentials();
        if creds.demo_api_key.trim().is_empty() || creds.demo_secret_key.trim().is_empty() {
            let mut lock = self.0.lock().unwrap();
            lock.add_log(
                "error",
                "❌ Gagal menyalakan bot: Kredensial API Key / Secret Key Demo belum diisi di menu Setting! Buka menu Setting (ikon gear) untuk memasukkan kunci Binance Testnet.",
            );
            return Err("Kredensial API Key / Secret Key Demo belum diisi di menu Setting!".to_string());
        }

        // Tes koneksi dulu sebelum bot aktif
        match test_testnet_account(&creds.demo_api_key, &creds.demo_secret_key) {
            Ok(bal) => {
                let mut lock = self.0.lock().unwrap();
                lock.config = new_cfg;
                lock.config.is_running = true;
                lock.cached_balance = Some(bal.clone());
                // Reset warm-up agar langsung melakukan warm-up baru
                lock.last_warmup_symbol.clear();
                lock.last_evaluated_bar = 0;
                lock.last_intrabar_signal_bar = 0;
                lock.last_intrabar_signal_dir = None;
                lock.position_bars_held = 0;
                lock.active_trade = None;
                lock.engine = VolStepEngine::new(
                    lock.config.manual_symbol.clone(),
                    lock.config.line_mode.clone(),
                    lock.config.volatility_type.clone(),
                    lock.config.volatility_length,
                    lock.config.vol_mult,
                );

                let mode_str = match lock.config.coin_mode.as_str() {
                    "top_gainers" => format!("Top {} Gainers 24h", lock.config.max_active_slots),
                    "top_volume" => format!("Top {} Volume 24h", lock.config.max_active_slots),
                    _ => format!("Manual Koin: {}", lock.config.manual_symbol),
                };

                let strat_str = match lock.config.strategy_mode.as_str() {
                    "best_bar2" => "Juara #1 Base Bar Murni (TP+SL Bar)".to_string(),
                    "best_overall" => "Juara #1 Keseluruhan (ALL)".to_string(),
                    "best_btp" => "Juara #1 Base Bar TP".to_string(),
                    "best_bsl" => "Juara #1 Base Bar SL".to_string(),
                    "best_fix" => "Juara #1 Base Persen Fixed".to_string(),
                    "best_med" => "Juara #1 Base Persen Median".to_string(),
                    _ => format!("Manual: {}", lock.config.manual_strategy_key),
                };

                let exec_str = match lock.config.execution_mode.as_str() {
                    "pre_close" => format!("Pre-Close Sniping ({}ms sebelum close)", lock.config.pre_close_buffer_ms),
                    "realtime_no_lag" => "Realtime No Lag (Intrabar Langsung)".to_string(),
                    _ => "Tunggu Candle Closed".to_string(),
                };

                let lev_str = if lock.config.leverage_mode == "auto_max" {
                    "Auto Max Leverage".to_string()
                } else {
                    format!("Manual {}x", lock.config.manual_leverage)
                };

                let margin_slot = lock.config.margin_pct_per_slot;
                lock.add_log(
                    "success",
                    &format!(
                        "⚡ BOT DEMO BINANCE DIAKTIFKAN! Saldo: {:.2} USDT | Koin: {} | Strategi: {} | Eksekusi: {} | Leverage: {} | Margin: {}%/slot",
                        bal.wallet_balance, mode_str, strat_str, exec_str, lev_str, margin_slot
                    ),
                );
                Ok(())
            }
            Err(e) => {
                let mut lock = self.0.lock().unwrap();
                lock.add_log(
                    "error",
                    &format!("❌ Gagal menghubungkan ke Binance Testnet: {}", e),
                );
                Err(format!("Gagal menghubungkan ke Binance Testnet: {}", e))
            }
        }
    }

    pub fn stop(&self) {
        let mut lock = self.0.lock().unwrap();
        lock.config.is_running = false;
        lock.add_log("warning", "🛑 BOT DEMO BINANCE DIHENTIKAN OLEH PENGGUNA. WebSocket diputus & tidak ada entry baru yang dibuka.");
    }

    pub fn panic_close_all(&self) -> Result<usize, String> {
        let creds = load_credentials();
        let mut lock = self.0.lock().unwrap();
        lock.active_trade = None;
        lock.add_log("error", "⚠️ PANIC CLOSE ALL TRIGGERED! Mengirim market close untuk semua posisi terbuka...");

        match panic_close_all_positions(&creds.demo_api_key, &creds.demo_secret_key) {
            Ok(count) => {
                lock.add_log("success", &format!("✓ Berhasil menutup paksa {} posisi di Binance Testnet.", count));
                Ok(count)
            }
            Err(e) => {
                lock.add_log("error", &format!("Gagal eksekusi panic close: {}", e));
                Err(e)
            }
        }
    }

    /// Background Worker 1: Supervisor Polling (Update Saldo, Posisi, Screener, dan Warm-up)
    fn spawn_background_worker(&self) {
        let state_arc = self.0.clone();

        thread::spawn(move || {
            let mut tick_counter: u64 = 0;

            loop {
                thread::sleep(Duration::from_secs(3));
                tick_counter += 1;

                let (is_running, cfg) = {
                    let lock = state_arc.lock().unwrap();
                    (lock.config.is_running, lock.config.clone())
                };

                let creds = load_credentials();
                if creds.demo_api_key.trim().is_empty() || creds.demo_secret_key.trim().is_empty() {
                    continue;
                }

                // Update saldo & posisi dari Binance Testnet setiap 6 detik
                if tick_counter % 2 == 0 {
                    if let Ok(bal) = test_testnet_account(&creds.demo_api_key, &creds.demo_secret_key) {
                        if let Ok(positions) = get_live_positions(&creds.demo_api_key, &creds.demo_secret_key) {
                            let mut lock = state_arc.lock().unwrap();
                            lock.cached_balance = Some(bal);
                            lock.cached_positions = positions;
                        }
                    }
                }

                // 1. Tentukan target symbol & koin yang di-scan dari Screener atau Manual
                let (raw_tickers, target_symbol) = match cfg.coin_mode.as_str() {
                    "top_gainers" | "top_losers" | "top_volume" => {
                        match fetch_top_screened_coins(cfg.max_active_slots, &cfg.coin_mode) {
                            Ok(coins) => {
                                if is_running && tick_counter % 10 == 0 {
                                    let names: Vec<String> = coins.iter().map(|c| format!("{}({:+0.1}%)", c.symbol, c.price_change_percent)).collect();
                                    let mut lock = state_arc.lock().unwrap();
                                    lock.add_log("info", &format!("🔍 [SCREENER {} 24H]: {}", cfg.coin_mode.to_uppercase(), names.join(", ")));
                                }
                                let first_sym = coins.first().map(|c| c.symbol.clone()).unwrap_or_else(|| cfg.manual_symbol.clone());
                                (coins, first_sym)
                            }
                            Err(_) => (Vec::new(), cfg.manual_symbol.clone()),
                        }
                    }
                    _ => {
                        let syms = if !cfg.manual_symbols.is_empty() {
                            cfg.manual_symbols.clone()
                        } else if !cfg.manual_symbol.is_empty() {
                            vec![cfg.manual_symbol.clone()]
                        } else {
                            vec!["BTCUSDT".to_string()]
                        };
                        let tickers = fetch_specific_symbols_tickers(&syms).unwrap_or_default();
                        let first_sym = syms.first().cloned().unwrap_or_else(|| "BTCUSDT".to_string());
                        (tickers, first_sym)
                    }
                };

                // Perbarui daftar scanned_coins di state untuk UI Live Scanner
                {
                    let mut lock = state_arc.lock().unwrap();
                    let cur_sr = lock.current_sr;
                    let cur_vol = lock.current_vol;
                    let active_px = lock.current_price;
                    let active_px_str = lock.current_price_str.clone();
                    let has_active_trade = lock.active_trade.as_ref().map(|t| (t.symbol.clone(), t.side.clone()));

                    let mut scanned: Vec<ScannedCoinState> = Vec::new();
                    for t in &raw_tickers {
                        let is_active = t.symbol == target_symbol;
                        let px = if is_active && active_px > 0.0 { active_px } else { t.last_price };
                        let px_str = if is_active && !active_px_str.is_empty() && active_px_str != "-" {
                            active_px_str.clone()
                        } else {
                            format_price_clean(px)
                        };

                        let (sr_val, sr_str, buy_val, buy_str, sell_val, sell_str, dist_pct, status) = if is_active && cur_sr > 0.0 {
                            let upper = cur_sr + cur_vol;
                            let lower = (cur_sr - cur_vol).max(0.0);
                            let dist = ((px - cur_sr) / cur_sr) * 100.0;
                            let stat = if let Some((ref tr_sym, ref tr_side)) = has_active_trade {
                                if tr_sym == &t.symbol {
                                    format!("DALAM POSISI ({})", tr_side)
                                } else {
                                    "DALAM RADAR".to_string()
                                }
                            } else if cur_vol > 0.0 && px > upper {
                                "BREAKOUT UP (POTENSI BUY)".to_string()
                            } else if cur_vol > 0.0 && px < lower {
                                "BREAKOUT DOWN (POTENSI SELL)".to_string()
                            } else {
                                "MENUNGGU BREAKOUT S/R".to_string()
                            };

                            (
                                cur_sr,
                                format_price_clean(cur_sr),
                                upper,
                                format_price_clean(upper),
                                lower,
                                format_price_clean(lower),
                                dist,
                                stat
                            )
                        } else if is_active {
                            (0.0, "-".into(), 0.0, "-".into(), 0.0, "-".into(), 0.0, "MEMPERSIAPKAN WARM-UP".into())
                        } else {
                            (0.0, "-".into(), 0.0, "-".into(), 0.0, "-".into(), 0.0, "ANTRIAN RADAR".into())
                        };

                        scanned.push(ScannedCoinState {
                            symbol: t.symbol.clone(),
                            price: px,
                            price_str: px_str,
                            change_24h: t.price_change_percent,
                            volume_24h: t.quote_volume,
                            is_active,
                            current_sr: sr_val,
                            current_sr_str: sr_str,
                            breakout_buy: buy_val,
                            breakout_buy_str: buy_str,
                            breakout_sell: sell_val,
                            breakout_sell_str: sell_str,
                            dist_pct,
                            status,
                        });
                    }
                    lock.scanned_coins = scanned;
                }

                if !is_running {
                    continue;
                }

                // 2. Warm-up check untuk target_symbol
                let needs_warmup = {
                    let lock = state_arc.lock().unwrap();
                    lock.last_warmup_symbol != target_symbol
                };

                if needs_warmup {
                    let mut lock = state_arc.lock().unwrap();
                    let warmup_target = cfg.warmup_klines.clamp(500, 3000);
                    lock.add_log("info", &format!("⏳ Memulai Warm-up Backtest untuk {} (TF: {}, target: {} candle)...", target_symbol, cfg.interval, warmup_target));
                    drop(lock);

                    // Fetch klines sesuai target candle
                    if let Ok(data) = fetch_binance_fapi_klines_by_count(&target_symbol, &cfg.interval, warmup_target, true, |_| {}) {
                        let bt_res = run_backtest(
                            &data.times,
                            &data.open,
                            &data.high,
                            &data.low,
                            &data.close,
                            &cfg.line_mode,
                            &cfg.volatility_type,
                            cfg.volatility_length,
                            cfg.vol_mult,
                            true,
                            1000.0,
                            cfg.margin_pct_per_slot,
                            "Current Balance",
                            cfg.manual_leverage,
                            0.05,
                            0.05,
                            0.4,
                            &cfg.one_way_rule,
                            cfg.max_active_slots,
                            0,
                            cfg.inc_hedge,
                            cfg.inc_trend,
                            cfg.inc_counter,
                            cfg.inc_bar_tp,
                            cfg.inc_bar_sl,
                            cfg.fixed1,
                            cfg.fixed2,
                            cfg.fixed3,
                            cfg.bar_tp1,
                            cfg.bar_tp2,
                            cfg.bar_tp_dir,
                            cfg.sl_bar1,
                            cfg.sl_bar2,
                            cfg.sl_bar_adv,
                            cfg.min_levels,
                            &cfg.rank_by,
                            "Semua Strategi",
                            15,
                            "Top #1 Sesuai Filter Leaderboard (Otomatis)",
                        );

                        // Ambil strategi juara sesuai strategy_mode
                        let chosen_strat = match cfg.strategy_mode.as_str() {
                            "best_bar2" => bt_res.leaderboard.category_winners.iter().find(|w| w.color_code == "bar2").map(|w| w.name.clone()).unwrap_or_else(|| bt_res.leaderboard.rows.first().map(|r| r.raw_name.clone()).unwrap_or_else(|| "TREND | TP BarProf B1 | SL BarLoss S1".into())),
                            "best_btp" => bt_res.leaderboard.category_winners.iter().find(|w| w.color_code == "btp").map(|w| w.name.clone()).unwrap_or_else(|| bt_res.leaderboard.rows.first().map(|r| r.raw_name.clone()).unwrap_or_else(|| "TREND | TP BarProf B1 | SL NextSR".into())),
                            "best_bsl" => bt_res.leaderboard.category_winners.iter().find(|w| w.color_code == "bsl").map(|w| w.name.clone()).unwrap_or_else(|| bt_res.leaderboard.rows.first().map(|r| r.raw_name.clone()).unwrap_or_else(|| "TREND | TP 1M | SL BarLoss S1".into())),
                            "best_fix" => bt_res.leaderboard.category_winners.iter().find(|w| w.color_code == "fix").map(|w| w.name.clone()).unwrap_or_else(|| bt_res.leaderboard.rows.first().map(|r| r.raw_name.clone()).unwrap_or_else(|| "TREND | TP Fix1 | SL Fix1".into())),
                            "best_med" => bt_res.leaderboard.category_winners.iter().find(|w| w.color_code == "med").map(|w| w.name.clone()).unwrap_or_else(|| bt_res.leaderboard.rows.first().map(|r| r.raw_name.clone()).unwrap_or_else(|| "TREND | TP 1M | SL 1M".into())),
                            "manual" => cfg.manual_strategy_key.clone(),
                            _ => bt_res.leaderboard.rows.first().map(|r| r.raw_name.clone()).unwrap_or_else(|| "TREND | TP 1M | SL NextSR".into()),
                        };

                        let interval_ms: i64 = match cfg.interval.as_str() {
                            "1m" => 60_000,
                            "3m" => 180_000,
                            "5m" => 300_000,
                            "15m" => 900_000,
                            "30m" => 1_800_000,
                            "1h" => 3_600_000,
                            "2h" => 7_200_000,
                            "4h" => 14_400_000,
                            "1d" => 86_400_000,
                            _ => 60_000,
                        };

                        // Inisialisasi VolStepEngine sesuai script python vstep.py
                        let mut eng = VolStepEngine::new(
                            target_symbol.clone(),
                            cfg.line_mode.clone(),
                            cfg.volatility_type.clone(),
                            cfg.volatility_length,
                            cfg.vol_mult,
                        );

                        let n_bars = data.times.len();
                        if n_bars > 0 {
                            for i in 0..(n_bars.saturating_sub(1)) {
                                let t_open = data.times[i];
                                let t_close = t_open + interval_ms - 1;
                                eng.process_closed_candle(
                                    data.open[i],
                                    data.high[i],
                                    data.low[i],
                                    data.close[i],
                                    None,
                                    Some(t_open),
                                    Some(t_close),
                                    Some(interval_ms),
                                );
                            }

                            let last_i = n_bars - 1;
                            let last_open = data.times[last_i];
                            let last_close = last_open + interval_ms - 1;
                            eng.update_live_tick(
                                data.close[last_i],
                                Some(data.high[last_i]),
                                Some(data.low[last_i]),
                                Some(data.open[last_i]),
                                Some(last_open),
                                Some(last_close),
                                None,
                                Some(Utc::now().timestamp_millis()),
                            );
                        }

                        let cur_sr = eng.confirmed_sr.unwrap_or_else(|| *data.close.last().unwrap_or(&0.0));
                        let cur_vol = eng.confirmed_vol_value.or(eng.live_vol_value).unwrap_or(0.0);
                        let cur_px = eng.live_close.unwrap_or_else(|| *data.close.last().unwrap_or(&0.0));

                        // Hitung med_h dan med_l dari riwayat S/R step backtest
                        let vol_series = calculate_volatility(
                            &data.high,
                            &data.low,
                            &data.close,
                            &cfg.volatility_type,
                            cfg.volatility_length,
                            cfg.vol_mult,
                        );
                        let sr_series = calculate_step_sr(&data.close, &vol_series, &cfg.line_mode);

                        let mut hi_arr = Vec::new();
                        let mut lo_arr = Vec::new();
                        let mut sim_level: Option<f64> = None;
                        let mut sim_hi: Option<f64> = None;
                        let mut sim_lo: Option<f64> = None;
                        for idx in 0..data.close.len() {
                            let c_sr = sr_series[idx];
                            let h = data.high[idx];
                            let l = data.low[idx];
                            match sim_level {
                                None => sim_level = Some(c_sr),
                                Some(curr) if c_sr != curr => {
                                    if let (Some(sh), Some(sl)) = (sim_hi, sim_lo) {
                                        if curr != 0.0 {
                                            hi_arr.push(((sh - curr) / curr * 100.0).max(0.0));
                                            lo_arr.push(((curr - sl) / curr * 100.0).max(0.0));
                                        }
                                    }
                                    sim_level = Some(c_sr);
                                    sim_hi = None;
                                    sim_lo = None;
                                }
                                _ => {
                                    sim_hi = Some(sim_hi.map(|v| v.max(h)).unwrap_or(h));
                                    sim_lo = Some(sim_lo.map(|v| v.min(l)).unwrap_or(l));
                                }
                            }
                        }
                        let med_h_raw = median(&hi_arr);
                        let med_l_raw = median(&lo_arr);
                        let med_h = if med_h_raw.is_nan() || med_h_raw <= 0.0 { 1.0 } else { med_h_raw };
                        let med_l = if med_l_raw.is_nan() || med_l_raw <= 0.0 { 1.0 } else { med_l_raw };

                        let sim_cfg = resolve_sim_config(&chosen_strat, &cfg);

                        // Atur leverage di Binance Testnet
                        let applied_lev = if cfg.leverage_mode == "auto_max" {
                            get_max_leverage(&target_symbol, &creds.demo_api_key, &creds.demo_secret_key).unwrap_or(cfg.manual_leverage)
                        } else {
                            cfg.manual_leverage
                        };
                        let _ = set_symbol_leverage(&target_symbol, applied_lev, &creds.demo_api_key, &creds.demo_secret_key);

                        let mut lock = state_arc.lock().unwrap();
                        lock.last_warmup_symbol = target_symbol.clone();
                        lock.last_selected_strategy = chosen_strat.clone();
                        lock.active_sim = Some(sim_cfg);
                        lock.current_sr = cur_sr;
                        lock.current_sr_str = eng.cur_level_str();
                        lock.current_vol = cur_vol;
                        lock.current_vol_str = format_price_clean(cur_vol);
                        lock.current_price = cur_px;
                        lock.current_price_str = format_price_clean(cur_px);
                        lock.med_h = med_h;
                        lock.med_l = med_l;
                        lock.active_leverage = applied_lev;
                        lock.engine = eng;

                        if let Some(target) = lock.scanned_coins.iter_mut().find(|c| c.symbol == target_symbol) {
                            target.current_sr = cur_sr;
                            target.current_sr_str = format_price_clean(cur_sr);
                            target.breakout_buy = cur_sr + cur_vol;
                            target.breakout_buy_str = format_price_clean(target.breakout_buy);
                            target.breakout_sell = (cur_sr - cur_vol).max(0.0);
                            target.breakout_sell_str = format_price_clean(target.breakout_sell);
                            if cur_sr > 0.0 && target.price > 0.0 {
                                target.dist_pct = ((target.price - cur_sr) / cur_sr) * 100.0;
                            }
                        }
                        lock.add_log(
                            "success",
                            &format!(
                                "🔥 [WARM-UP SELESAI] {} ({} candle). Juara: {} | Confirmed S/R: {:.4} (Band: {:.4}) | MedH: {:.2}% / MedL: {:.2}% | Leverage: {}x",
                                target_symbol,
                                data.times.len(),
                                chosen_strat,
                                cur_sr,
                                cur_vol,
                                med_h,
                                med_l,
                                applied_lev
                            ),
                        );
                    }
                }

                // 3. ENGINE 2 RECONCILIATION: Pastikan tidak ada candle close yang tertinggal jika WS sempat lag
                if is_running {
                    let (last_closed_open, interval, sym) = {
                        let lock = state_arc.lock().unwrap();
                        (lock.engine.last_closed_open_time, lock.config.interval.clone(), lock.last_warmup_symbol.clone())
                    };
                    if !sym.is_empty() && last_closed_open > 0 {
                        let now_ms = Utc::now().timestamp_millis();
                        let interval_ms: i64 = match interval.as_str() {
                            "1m" => 60_000,
                            "3m" => 180_000,
                            "5m" => 300_000,
                            "15m" => 900_000,
                            "30m" => 1_800_000,
                            "1h" => 3_600_000,
                            "2h" => 7_200_000,
                            "4h" => 14_400_000,
                            "1d" => 86_400_000,
                            _ => 60_000,
                        };
                        let expected_cur_open = (now_ms / interval_ms) * interval_ms;
                        let expected_prev_open = expected_cur_open - interval_ms;
                        if last_closed_open < expected_prev_open && (now_ms - expected_cur_open) >= 1200 {
                            if let Ok(kl) = fetch_binance_fapi_klines_by_count(&sym, &interval, 3, false, |_| {}) {
                                let mut lock = state_arc.lock().unwrap();
                                for i in 0..(kl.times.len().saturating_sub(1)) {
                                    let t = kl.times[i];
                                    if t > lock.engine.last_closed_open_time {
                                        let t_end = t + interval_ms - 1;
                                        lock.engine.process_closed_candle(
                                            kl.open[i],
                                            kl.high[i],
                                            kl.low[i],
                                            kl.close[i],
                                            None,
                                            Some(t),
                                            Some(t_end),
                                            Some(interval_ms),
                                        );
                                        if let Some(sr) = lock.engine.confirmed_sr {
                                            lock.current_sr = sr;
                                            lock.current_sr_str = lock.engine.cur_level_str();
                                        }
                                        if let Some(vol) = lock.engine.confirmed_vol_value {
                                            lock.current_vol = vol;
                                            lock.current_vol_str = format_price_clean(vol);
                                        }
                                        let log_sr = lock.current_sr;
                                        let log_vol = lock.current_vol;
                                        lock.add_log("info", &format!("🔄 [RECONCILE] S/R bar terlewat disinkronkan: {:.4} (Band: {:.4})", log_sr, log_vol));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /// Background Worker 2: WebSocket Kline & AggTrade Streaming (Dual-Stream persis vstep.py)
    fn spawn_websocket_worker(&self) {
        let state_arc = self.0.clone();

        thread::spawn(move || {
            loop {
                // Tunggu sampai bot running dan target koin sudah warm-up
                let (is_running, symbol, interval) = {
                    let lock = state_arc.lock().unwrap();
                    (
                        lock.config.is_running,
                        lock.last_warmup_symbol.clone(),
                        lock.config.interval.clone(),
                    )
                };

                if !is_running || symbol.is_empty() {
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }

                let stream_sym = symbol.to_lowercase();
                // Combined stream: aggTrade + kline_{interval}
                let ws_url = format!("wss://fstream.binance.com/stream?streams={}@aggTrade/{}@kline_{}", stream_sym, stream_sym, interval);

                {
                    let lock = state_arc.lock().unwrap();
                    let exec_str = match lock.config.execution_mode.as_str() {
                        "pre_close" => format!("Pre-Close Sniping ({}ms sebelum bar close)", lock.config.pre_close_buffer_ms),
                        "realtime_no_lag" => "Realtime No Lag (Intrabar Per-Tick)".to_string(),
                        _ => "On Candle Close".to_string(),
                    };
                    drop(lock);
                    let mut lock_write = state_arc.lock().unwrap();
                    lock_write.add_log("info", &format!("📡 [WS DUAL-STREAM CONNECTED] {} | Mode: {}", ws_url, exec_str));
                }

                match connect(&ws_url) {
                    Ok((mut socket, _)) => {
                        loop {
                            let (still_running, current_target) = {
                                let lock = state_arc.lock().unwrap();
                                (lock.config.is_running, lock.last_warmup_symbol.clone())
                            };

                            if !still_running || current_target != symbol {
                                let _ = socket.close(None);
                                break;
                            }

                            let msg = match socket.read() {
                                Ok(m) => m,
                                Err(e) => {
                                    let mut lock = state_arc.lock().unwrap();
                                    lock.add_log("warning", &format!("WS Reconnect: {}", e));
                                    break;
                                }
                            };

                            if !msg.is_text() {
                                continue;
                            }

                            let text = match msg.to_text() {
                                Ok(t) => t,
                                Err(_) => continue,
                            };

                            let val: serde_json::Value = match serde_json::from_str(text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };

                            let stream_name = val.get("stream").and_then(|s| s.as_str()).unwrap_or("");
                            let data_obj = val.get("data").unwrap_or(&val);
                            let event_type = data_obj.get("e").and_then(|e| e.as_str()).unwrap_or("");

                            let now_ms = Utc::now().timestamp_millis();

                            let mut live_tick_price: Option<f64> = None;
                            let mut live_tick_str: Option<String> = None;
                            let mut candle_closed_flag = false;
                            let mut bar_start_time = 0i64;
                            let mut bar_close_time = 0i64;
                            let mut bar_open_px = 0.0f64;
                            let mut bar_high_px = 0.0f64;
                            let mut bar_low_px = 0.0f64;
                            let mut bar_close_px = 0.0f64;
                            let _ = (bar_high_px, bar_low_px);

                            let interval_ms: i64 = match interval.as_str() {
                                "1m" => 60_000,
                                "3m" => 180_000,
                                "5m" => 300_000,
                                "15m" => 900_000,
                                "30m" => 1_800_000,
                                "1h" => 3_600_000,
                                "2h" => 7_200_000,
                                "4h" => 14_400_000,
                                "1d" => 86_400_000,
                                _ => 60_000,
                            };

                            if event_type == "aggTrade" || stream_name.ends_with("@aggTrade") {
                                let p_val = data_obj.get("p").and_then(|p| p.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                let p_str = data_obj.get("p").and_then(|p| p.as_str()).unwrap_or("").to_string();
                                let t_trade = data_obj.get("T").and_then(|t| t.as_i64()).unwrap_or(now_ms);

                                if p_val <= 0.0 {
                                    continue;
                                }

                                live_tick_price = Some(p_val);
                                live_tick_str = Some(p_str.clone());

                                let mut lock = state_arc.lock().unwrap();
                                lock.engine.update_live_tick(
                                    p_val,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    Some(&p_str),
                                    Some(t_trade),
                                );
                                lock.current_price = p_val;
                                if !p_str.is_empty() {
                                    lock.current_price_str = p_str.clone();
                                }
                                if let Some(sr) = lock.engine.confirmed_sr {
                                    lock.current_sr = sr;
                                    lock.current_sr_str = lock.engine.cur_level_str();
                                }
                                if let Some(vol) = lock.engine.confirmed_vol_value.or(lock.engine.live_vol_value) {
                                    lock.current_vol = vol;
                                    lock.current_vol_str = format_price_clean(vol);
                                }
                            } else if event_type == "kline" || stream_name.contains("@kline") {
                                if let Some(k) = data_obj.get("k") {
                                    bar_start_time = k["t"].as_i64().unwrap_or(0);
                                    bar_close_time = k["T"].as_i64().unwrap_or(0);
                                    candle_closed_flag = k["x"].as_bool().unwrap_or(false);
                                    bar_open_px = k["o"].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                    bar_high_px = k["h"].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                    bar_low_px = k["l"].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                    bar_close_px = k["c"].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                    let c_str = k["c"].as_str().unwrap_or("").to_string();

                                    if bar_close_px <= 0.0 {
                                        continue;
                                    }

                                    live_tick_price = Some(bar_close_px);
                                    live_tick_str = Some(c_str.clone());

                                    let mut lock = state_arc.lock().unwrap();
                                    lock.current_price = bar_close_px;
                                    if !c_str.is_empty() {
                                        lock.current_price_str = c_str.clone();
                                    }

                                    if candle_closed_flag {
                                        lock.engine.process_closed_candle(
                                            bar_open_px,
                                            bar_high_px,
                                            bar_low_px,
                                            bar_close_px,
                                            Some(&c_str),
                                            Some(bar_start_time),
                                            Some(bar_close_time),
                                            Some(interval_ms),
                                        );
                                        if let Some(sr) = lock.engine.confirmed_sr {
                                            lock.current_sr = sr;
                                            lock.current_sr_str = lock.engine.cur_level_str();
                                        }
                                        if let Some(vol) = lock.engine.confirmed_vol_value {
                                            lock.current_vol = vol;
                                            lock.current_vol_str = format_price_clean(vol);
                                        }
                                    } else {
                                        lock.engine.update_live_tick(
                                            bar_close_px,
                                            Some(bar_high_px),
                                            Some(bar_low_px),
                                            Some(bar_open_px),
                                            Some(bar_start_time),
                                            Some(bar_close_time),
                                            Some(&c_str),
                                            Some(now_ms),
                                        );
                                        if let Some(vol) = lock.engine.live_vol_value {
                                            lock.current_vol = vol;
                                            lock.current_vol_str = format_price_clean(vol);
                                        }
                                    }
                                }
                            }

                            // Update scanned coins radar state
                            if let Some(c_px) = live_tick_price {
                                let mut lock = state_arc.lock().unwrap();
                                let is_next_active = lock.engine.next_sr_active;
                                let next_dir = lock.engine.next_sr_dir.clone().unwrap_or_default();
                                let next_px_str = lock.engine.next_sr_price_str();
                                let cur_sr = lock.current_sr;
                                let cur_vol = lock.current_vol;
                                let active_side = lock.active_trade.as_ref().map(|t| t.side.clone());

                                if let Some(target) = lock.scanned_coins.iter_mut().find(|c| c.symbol == symbol) {
                                    target.price = c_px;
                                    if let Some(ref p_str) = live_tick_str {
                                        if !p_str.is_empty() {
                                            target.price_str = p_str.clone();
                                        }
                                    }
                                    if cur_sr > 0.0 {
                                        target.dist_pct = ((c_px - cur_sr) / cur_sr) * 100.0;
                                        target.current_sr = cur_sr;
                                        target.current_sr_str = format_price_clean(cur_sr);
                                        target.breakout_buy = cur_sr + cur_vol;
                                        target.breakout_buy_str = format_price_clean(target.breakout_buy);
                                        target.breakout_sell = (cur_sr - cur_vol).max(0.0);
                                        target.breakout_sell_str = format_price_clean(target.breakout_sell);

                                        if is_next_active {
                                            target.status = format!("NEXT SR CALON {} @ {}", next_dir, next_px_str);
                                        } else if let Some(ref side) = active_side {
                                            target.status = format!("POSISI TERBUKA ({})", side);
                                        } else {
                                            target.status = "DALAM RENTANG S/R".to_string();
                                        }
                                    }
                                }
                            }

                            let creds = load_credentials();

                            // 1. INTRABAR POSITION MANAGEMENT (TP, SL, TRAILING STOP ON EVERY TICK)
                            if let Some(c_px) = live_tick_price {
                                let mut trade_to_close: Option<(String, String, f64, String, f64)> = None;
                                {
                                    let mut lock = state_arc.lock().unwrap();
                                    if let Some(ref mut trade) = lock.active_trade {
                                        if trade.symbol == symbol {
                                            // Update extreme price
                                            if trade.side == "BUY" {
                                                trade.tr_extreme = trade.tr_extreme.max(c_px);
                                            } else {
                                                trade.tr_extreme = trade.tr_extreme.min(c_px);
                                            }

                                            // Trailing Stop Ratchet
                                            if trade.use_trailing {
                                                let fav_pct = if trade.side == "BUY" {
                                                    (trade.tr_extreme - trade.entry_price) / trade.entry_price * 100.0
                                                } else {
                                                    (trade.entry_price - trade.tr_extreme) / trade.entry_price * 100.0
                                                };
                                                if fav_pct >= trade.tr_act_pct {
                                                    let new_stop = if trade.side == "BUY" {
                                                        trade.tr_extreme * (1.0 - trade.tr_dist_pct / 100.0)
                                                    } else {
                                                        trade.tr_extreme * (1.0 + trade.tr_dist_pct / 100.0)
                                                    };
                                                    trade.tr_stop_price = Some(match trade.tr_stop_price {
                                                        Some(cur) => if trade.side == "BUY" { cur.max(new_stop) } else { cur.min(new_stop) },
                                                        None => new_stop,
                                                    });
                                                }
                                            }

                                            // Effective Stop
                                            let eff_stop = if trade.side == "BUY" {
                                                match (trade.sl_price, trade.tr_stop_price) {
                                                    (None, t) => t,
                                                    (Some(s), None) => Some(s),
                                                    (Some(s), Some(t)) => Some(s.max(t)),
                                                }
                                            } else {
                                                match (trade.sl_price, trade.tr_stop_price) {
                                                    (None, t) => t,
                                                    (Some(s), None) => Some(s),
                                                    (Some(s), Some(t)) => Some(s.min(t)),
                                                }
                                            };

                                            let stop_hit = eff_stop.map(|s| {
                                                if trade.side == "BUY" { c_px <= s } else { c_px >= s }
                                            }).unwrap_or(false);

                                            let tp_hit = trade.tp_price.map(|tp| {
                                                if trade.side == "BUY" { c_px >= tp } else { c_px <= tp }
                                            }).unwrap_or(false);

                                            if stop_hit {
                                                let is_tr = trade.tr_stop_price.is_some() && (trade.sl_price.is_none() || (
                                                    if trade.side == "BUY" { trade.tr_stop_price.unwrap() >= trade.sl_price.unwrap() }
                                                    else { trade.tr_stop_price.unwrap() <= trade.sl_price.unwrap() }
                                                ));
                                                let stop_target = eff_stop.unwrap_or(c_px);
                                                let why = if is_tr {
                                                    format!("TRAIL STOP ({:.4})", stop_target)
                                                } else {
                                                    format!("STOP LOSS ({:.4})", stop_target)
                                                };
                                                trade_to_close = Some((trade.symbol.clone(), trade.side.clone(), trade.quantity, why, c_px));
                                            } else if tp_hit {
                                                let why = format!("TAKE PROFIT ({:.4})", trade.tp_price.unwrap_or(c_px));
                                                trade_to_close = Some((trade.symbol.clone(), trade.side.clone(), trade.quantity, why, c_px));
                                            }
                                        }
                                    }
                                }

                                if let Some((sym, side, qty, why, exit_px)) = trade_to_close {
                                    let _ = market_close_position(&sym, &side, qty, &creds.demo_api_key, &creds.demo_secret_key);
                                    let mut lock = state_arc.lock().unwrap();
                                    let pnl_str = if let Some(ref t) = lock.active_trade {
                                        let pnl = (if t.side == "BUY" { 1.0 } else { -1.0 }) * t.quantity * (exit_px - t.entry_price);
                                        format!("(Est. PnL: {:+.2} USDT)", pnl)
                                    } else {
                                        String::new()
                                    };
                                    lock.active_trade = None;
                                    lock.add_log("trade", &format!("🏁 [EXIT {}] Menutup posisi {} {} @ {:.4} {}", why, side, sym, exit_px, pnl_str));
                                }
                            }

                            // 2. CANDLE CLOSE PROCESSING (BAR-BASED TP & SL)
                            if candle_closed_flag {
                                let mut bar_exit: Option<(String, String, f64, String, f64)> = None;
                                {
                                    let mut lock = state_arc.lock().unwrap();
                                    if let Some(ref mut trade) = lock.active_trade {
                                        if trade.symbol == symbol {
                                            trade.bars_held += 1;
                                            let is_dir = if trade.side == "BUY" { bar_close_px > bar_open_px } else { bar_close_px < bar_open_px };
                                            let is_adv = if trade.side == "BUY" { bar_close_px < bar_open_px } else { bar_close_px > bar_open_px };
                                            trade.dir_candle_count = if is_dir { trade.dir_candle_count + 1 } else { 0 };
                                            trade.adv_candle_count = if is_adv { trade.adv_candle_count + 1 } else { 0 };

                                            let gross_pnl = (if trade.side == "BUY" { 1.0 } else { -1.0 }) * trade.quantity * (bar_close_px - trade.entry_price);
                                            let is_net_prof = gross_pnl > 0.0;
                                            let is_in_loss = gross_pnl <= 0.0;

                                            let mut bar_tp_hit = false;
                                            let mut bar_tp_why = String::new();
                                            let mut bar_sl_hit = false;
                                            let mut bar_sl_why = String::new();

                                            if trade.tp_bar_type == 3 && trade.bars_held >= trade.tp_bar_target {
                                                bar_tp_hit = true;
                                                bar_tp_why = format!("TP BAR ({}b)", trade.bars_held);
                                            } else if trade.tp_bar_type == 4 && trade.bars_held >= trade.tp_bar_target && is_net_prof {
                                                bar_tp_hit = true;
                                                bar_tp_why = format!("TP B-PROF ({}b)", trade.bars_held);
                                            } else if trade.tp_bar_type == 5 && trade.dir_candle_count >= trade.tp_bar_target && is_net_prof {
                                                bar_tp_hit = true;
                                                bar_tp_why = format!("TP B-DIR ({}c)", trade.dir_candle_count);
                                            }

                                            if trade.sl_bar_type == 3 && trade.bars_held >= trade.sl_bar_target && is_in_loss {
                                                bar_sl_hit = true;
                                                bar_sl_why = format!("SL B-LOSS ({}b)", trade.bars_held);
                                            } else if trade.sl_bar_type == 4 && trade.bars_held >= trade.sl_bar_target {
                                                bar_sl_hit = true;
                                                bar_sl_why = format!("SL MAX-BAR ({}b)", trade.bars_held);
                                            } else if trade.sl_bar_type == 5 && trade.adv_candle_count >= trade.sl_bar_target && is_in_loss {
                                                bar_sl_hit = true;
                                                bar_sl_why = format!("SL B-ADV ({}c)", trade.adv_candle_count);
                                            }

                                            if bar_sl_hit && trade.sl_bar_type != 4 {
                                                bar_exit = Some((trade.symbol.clone(), trade.side.clone(), trade.quantity, bar_sl_why, bar_close_px));
                                            } else if bar_tp_hit {
                                                bar_exit = Some((trade.symbol.clone(), trade.side.clone(), trade.quantity, bar_tp_why, bar_close_px));
                                            } else if bar_sl_hit {
                                                bar_exit = Some((trade.symbol.clone(), trade.side.clone(), trade.quantity, bar_sl_why, bar_close_px));
                                            }
                                        }
                                    }
                                }

                                if let Some((sym, side, qty, why, exit_px)) = bar_exit {
                                    let _ = market_close_position(&sym, &side, qty, &creds.demo_api_key, &creds.demo_secret_key);
                                    let mut lock = state_arc.lock().unwrap();
                                    let pnl_str = if let Some(ref t) = lock.active_trade {
                                        let pnl = (if t.side == "BUY" { 1.0 } else { -1.0 }) * t.quantity * (exit_px - t.entry_price);
                                        format!("(Est. PnL: {:+.2} USDT)", pnl)
                                    } else {
                                        String::new()
                                    };
                                    lock.active_trade = None;
                                    lock.add_log("trade", &format!("🏁 [EXIT {}] Menutup posisi {} {} @ {:.4} {}", why, side, sym, exit_px, pnl_str));
                                }
                            }

                            // 3. ENTRY EVALUATION (REALTIME_NO_LAG, PRE_CLOSE, ON_CLOSE)
                            let (exec_mode, pre_close_buf_ms, active_sim, med_h, med_l, lev, margin_pct, one_way_rule) = {
                                let lock = state_arc.lock().unwrap();
                                (
                                    lock.config.execution_mode.clone(),
                                    lock.config.pre_close_buffer_ms,
                                    lock.active_sim.clone(),
                                    lock.med_h,
                                    lock.med_l,
                                    lock.active_leverage,
                                    lock.config.margin_pct_per_slot,
                                    lock.config.one_way_rule.clone(),
                                )
                            };

                            let mut should_execute = false;
                            let mut sig_step_up = false;
                            let mut is_preclose_trigger = false;
                            let mut snipe_time_left_ms: i64 = 0;

                            match exec_mode.as_str() {
                                "realtime_no_lag" => {
                                    let mut lock = state_arc.lock().unwrap();
                                    if lock.engine.next_sr_active {
                                        let bar_time = lock.engine.candle_open_time;
                                        let dir = lock.engine.next_sr_dir.clone().unwrap_or_default();
                                        if lock.last_intrabar_signal_bar != bar_time || lock.last_intrabar_signal_dir.as_deref() != Some(&dir) {
                                            lock.last_intrabar_signal_bar = bar_time;
                                            lock.last_intrabar_signal_dir = Some(dir.clone());
                                            should_execute = true;
                                            sig_step_up = dir == "BULL";
                                            let sr_str = lock.engine.next_sr_price_str();
                                            let px_str = lock.current_price_str.clone();
                                            lock.add_log("info", &format!("⚡ [INTRABAR LIVE SIGNAL] Next SR {} aktif @ {} (Harga: {})!", dir, sr_str, px_str));
                                        }
                                    }
                                }
                                "pre_close" => {
                                    let mut lock = state_arc.lock().unwrap();
                                    let t_close = if bar_close_time > 0 { bar_close_time } else { lock.engine.candle_close_time };
                                    let t_open = if bar_start_time > 0 { bar_start_time } else { lock.engine.candle_open_time };
                                    let time_left_ms = t_close - now_ms;
                                    snipe_time_left_ms = time_left_ms;
                                    let in_snipe_window = time_left_ms <= pre_close_buf_ms as i64 && time_left_ms > -3000;
                                    if in_snipe_window && lock.engine.next_sr_active && lock.last_evaluated_bar != t_open {
                                        lock.last_evaluated_bar = t_open;
                                        let dir = lock.engine.next_sr_dir.clone().unwrap_or_default();
                                        should_execute = true;
                                        sig_step_up = dir == "BULL";
                                        is_preclose_trigger = true;
                                        let sr_str = lock.engine.next_sr_price_str();
                                        lock.add_log("info", &format!("🎯 [PRE-CLOSE SNIPER] Sisa {}ms, Next SR {} aktif @ {}!", time_left_ms, dir, sr_str));
                                    }
                                }
                                _ => {
                                    // on_close
                                    if candle_closed_flag {
                                        let mut lock = state_arc.lock().unwrap();
                                        if lock.last_evaluated_bar != bar_start_time {
                                            lock.last_evaluated_bar = bar_start_time;
                                            if let (Some(cur), Some(prev)) = (lock.engine.cur_level, lock.engine.prev_level) {
                                                if cur != prev {
                                                    should_execute = true;
                                                    sig_step_up = cur > prev;
                                                    lock.add_log("info", &format!("📊 [ON CLOSE SIGNAL] Bar closed dengan level SR baru: {:.4} (Prev: {:.4})", cur, prev));
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if should_execute {
                                let c_px = live_tick_price.unwrap_or(0.0);
                                if c_px > 0.0 {
                                    let sim = active_sim.unwrap_or_else(|| ActiveSimConfig::default_trend());
                                    let sig_side = match sim.mode {
                                        1 => if sig_step_up { "BUY" } else { "SELL" },      // TREND
                                        2 => if sig_step_up { "SELL" } else { "BUY" },      // COUNTER
                                        _ => if sig_step_up { "BUY" } else { "SELL" },      // HEDGE default
                                    };

                                    let mut can_open_new = true;
                                    let existing_pos = {
                                        let lock = state_arc.lock().unwrap();
                                        lock.active_trade.clone()
                                    };

                                    if let Some(ref trade) = existing_pos {
                                        if trade.sl_next_sr {
                                            let _ = market_close_position(&trade.symbol, &trade.side, trade.quantity, &creds.demo_api_key, &creds.demo_secret_key);
                                            let mut lock = state_arc.lock().unwrap();
                                            lock.active_trade = None;
                                            lock.add_log("trade", &format!("🏁 [EXIT NEXT SR] Menutup posisi {} {} karena level SR baru terbentuk.", trade.side, trade.symbol));
                                        } else if one_way_rule == "Tutup Posisi Lama & Buka Baru di SR Baru" {
                                            let _ = market_close_position(&trade.symbol, &trade.side, trade.quantity, &creds.demo_api_key, &creds.demo_secret_key);
                                            let mut lock = state_arc.lock().unwrap();
                                            lock.active_trade = None;
                                            lock.add_log("info", &format!("🔄 [ONE-WAY] Menutup posisi lama {} {} sebelum membuka posisi baru di SR baru.", trade.side, trade.symbol));
                                        } else if one_way_rule == "Reverse Jika Lawan Arah, Tahan Jika Searah" {
                                            if trade.side == sig_side {
                                                let mut lock = state_arc.lock().unwrap();
                                                lock.add_log("info", &format!("🛡️ [ONE-WAY HOLD] Sinyal {} searah dengan posisi aktif. Skip buka baru.", sig_side));
                                                can_open_new = false;
                                            } else {
                                                let _ = market_close_position(&trade.symbol, &trade.side, trade.quantity, &creds.demo_api_key, &creds.demo_secret_key);
                                                let mut lock = state_arc.lock().unwrap();
                                                lock.active_trade = None;
                                                lock.add_log("info", &format!("🔄 [ONE-WAY REVERSE] Membalik arah posisi dari {} ke {}.", trade.side, sig_side));
                                            }
                                        } else {
                                            // Tahan Posisi Lama (Skip SR Baru)
                                            let mut lock = state_arc.lock().unwrap();
                                            lock.add_log("info", &format!("🛡️ [ONE-WAY SKIP] Posisi {} {} masih terbuka. Melewatkan sinyal baru.", trade.symbol, trade.side));
                                            can_open_new = false;
                                        }
                                    }

                                    if can_open_new {
                                        let wallet_bal = {
                                            let lock = state_arc.lock().unwrap();
                                            lock.cached_balance.as_ref().map(|b| b.wallet_balance).unwrap_or(1000.0)
                                        };
                                        let slot_margin = wallet_bal * (margin_pct / 100.0);
                                        let notional = slot_margin * (lev as f64);
                                        let raw_qty = notional / c_px;

                                        match market_open_position(&symbol, sig_side, raw_qty, &creds.demo_api_key, &creds.demo_secret_key) {
                                            Ok((order_id, actual_qty, qty_str)) => {
                                                let dir_f = if sig_side == "BUY" { 1.0 } else { -1.0 };
                                                let fav_m = if sig_side == "BUY" { med_h } else { med_l };
                                                let adv_m = if sig_side == "BUY" { med_l } else { med_h };

                                                let tp_p = match sim.tp_t {
                                                    1 => Some(fav_m * sim.tp_v),
                                                    2 => Some(sim.tp_v),
                                                    _ => None,
                                                };
                                                let tp_px = tp_p.map(|p| c_px * (1.0 + dir_f * p / 100.0));

                                                let sl_p = match sim.sl_t {
                                                    1 => Some(adv_m * sim.sl_v),
                                                    2 => Some(sim.sl_v),
                                                    _ => None,
                                                };
                                                let sl_px = sl_p.map(|p| c_px * (1.0 - dir_f * p / 100.0));

                                                let (use_tr, tr_act_pct, tr_dist_pct) = if sim.use_tr {
                                                    let a = if sim.tr_u == 1 { fav_m * sim.tr_a } else { sim.tr_a };
                                                    let d = if sim.tr_u == 1 { fav_m * sim.tr_d } else { sim.tr_d };
                                                    (true, a, d)
                                                } else {
                                                    (false, 0.0, 0.0)
                                                };

                                                let is_bar_tp = sim.tp_t >= 3;
                                                let b_tp_type = if is_bar_tp { sim.tp_t } else { 0 };
                                                let b_tp_tgt = if is_bar_tp { sim.tp_v as i32 } else { 0 };

                                                let is_bar_sl = sim.sl_t >= 3;
                                                let b_sl_type = if is_bar_sl { sim.sl_t } else { 0 };
                                                let b_sl_tgt = if is_bar_sl { sim.sl_v as i32 } else { 0 };

                                                let sl_next_sr = sim.sl_t == 0;

                                                let tp_desc = if let Some(tp) = tp_px {
                                                    format!("{:.4}", tp)
                                                } else if use_tr {
                                                    format!("Trail({:.2}%/{:.2}%)", tr_act_pct, tr_dist_pct)
                                                } else if b_tp_type == 3 {
                                                    format!("BarFix({}b)", b_tp_tgt)
                                                } else if b_tp_type == 4 {
                                                    format!("BarProf(>={}b)", b_tp_tgt)
                                                } else if b_tp_type == 5 {
                                                    format!("BarDir({}c)", b_tp_tgt)
                                                } else {
                                                    "-".to_string()
                                                };

                                                let sl_desc = if let Some(sl) = sl_px {
                                                    format!("{:.4}", sl)
                                                } else if b_sl_type == 3 {
                                                    format!("BarLoss(>={}b)", b_sl_tgt)
                                                } else if b_sl_type == 4 {
                                                    format!("MaxBar({}b)", b_sl_tgt)
                                                } else if b_sl_type == 5 {
                                                    format!("BarAdv({}c)", b_sl_tgt)
                                                } else {
                                                    "NextSR".to_string()
                                                };

                                                let new_trade = ActiveTradeState {
                                                    symbol: symbol.clone(),
                                                    side: sig_side.to_string(),
                                                    entry_price: c_px,
                                                    quantity: actual_qty,
                                                    entry_time_ms: now_ms,
                                                    tp_price: tp_px,
                                                    sl_price: sl_px,
                                                    tp_desc: tp_desc.clone(),
                                                    sl_desc: sl_desc.clone(),
                                                    use_trailing: use_tr,
                                                    tr_act_pct,
                                                    tr_dist_pct,
                                                    tr_extreme: c_px,
                                                    tr_stop_price: None,
                                                    tp_bar_type: b_tp_type,
                                                    tp_bar_target: b_tp_tgt,
                                                    sl_bar_type: b_sl_type,
                                                    sl_bar_target: b_sl_tgt,
                                                    bars_held: 0,
                                                    dir_candle_count: 0,
                                                    adv_candle_count: 0,
                                                    sl_next_sr,
                                                };

                                                let tag = if is_preclose_trigger {
                                                    format!("🎯 [PRE-CLOSE SNIPER (Sisa: {}ms)]", snipe_time_left_ms)
                                                } else {
                                                    "⚡ [SR BREAKOUT EXECUTED LIVE]".to_string()
                                                };

                                                let mut lock = state_arc.lock().unwrap();
                                                lock.active_trade = Some(new_trade);
                                                lock.position_bars_held = 0;
                                                lock.add_log(
                                                    "trade",
                                                    &format!(
                                                        "{} Order #{} {} {} {} @ {:.4} BERHASIL! | Lev: {}x | TP: {} | SL: {} | Strategi: {}",
                                                        tag, order_id, sig_side, qty_str, symbol, c_px, lev, tp_desc, sl_desc, sim.key
                                                    ),
                                                );
                                            }
                                            Err(e) => {
                                                let mut lock = state_arc.lock().unwrap();
                                                lock.add_log("error", &format!("❌ Gagal eksekusi order {} {}: {}", sig_side, symbol, e));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let mut lock = state_arc.lock().unwrap();
                        lock.add_log("warning", &format!("Gagal koneksi WebSocket ke Binance: {}. Reconnecting dalam 3 detik...", e));
                        thread::sleep(Duration::from_secs(3));
                    }
                }
            }
        });
    }
}
