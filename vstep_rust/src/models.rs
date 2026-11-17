use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub secret_key: String,
    #[serde(default = "default_true")]
    pub use_testnet: bool,
    #[serde(default = "default_symbols")]
    pub symbols: Vec<String>,
    #[serde(default = "default_coin_source_mode")]
    pub coin_source_mode: String,
    #[serde(default = "default_max_screener_coins")]
    pub max_screener_coins: usize,
    #[serde(default = "default_min_24h_volume")]
    pub min_24h_volume_usdt: f64,
    #[serde(default = "default_screener_refresh_min")]
    pub screener_refresh_min: u64,
    #[serde(default = "default_max_open_positions")]
    pub max_open_positions: usize,
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
    #[serde(default = "default_warmup_limit")]
    pub warmup_limit: usize,
    #[serde(default = "default_warmup_delay")]
    pub warmup_delay: f64,
    #[serde(default = "default_line_mode")]
    pub line_mode: String,
    #[serde(default = "default_vol_type")]
    pub vol_type: String,
    #[serde(default = "default_lookback")]
    pub lookback: usize,
    #[serde(default = "default_vol_mult")]
    pub vol_mult: f64,
    #[serde(default = "default_true")]
    pub stdev_bessel: bool,
    #[serde(default = "default_strategy_mode")]
    pub strategy_mode: String,
    #[serde(default = "default_one_way_rule")]
    pub one_way_rule: String,
    #[serde(default = "default_entry_mode")]
    pub entry_mode: String,
    #[serde(default = "default_limit_timeout_sec")]
    pub limit_timeout_sec: f64,
    #[serde(default = "default_margin_pct")]
    pub margin_pct: f64,
    #[serde(default = "default_leverage_mode")]
    pub leverage_mode: String,
    #[serde(default = "default_leverage")]
    pub leverage: u32,
    #[serde(default = "default_trailing_mode")]
    pub trailing_mode: String,
    #[serde(default = "default_selected_mode")]
    pub selected_mode: String,
    #[serde(default = "default_selected_cat")]
    pub selected_cat: String,
    #[serde(default = "default_selected_preset")]
    pub selected_preset: String,
    #[serde(default = "default_fixed1")]
    pub fixed1: f64,
    #[serde(default = "default_fixed2")]
    pub fixed2: f64,
    #[serde(default = "default_fixed3")]
    pub fixed3: f64,
    #[serde(default = "default_bar_tp1")]
    pub bar_tp1: usize,
    #[serde(default = "default_bar_tp2")]
    pub bar_tp2: usize,
    #[serde(default = "default_bar_tp_dir")]
    pub bar_tp_dir: usize,
    #[serde(default = "default_sl_bar1")]
    pub sl_bar1: usize,
    #[serde(default = "default_sl_bar2")]
    pub sl_bar2: usize,
    #[serde(default = "default_sl_bar_adv")]
    pub sl_bar_adv: usize,
    #[serde(default = "default_min_levels_median")]
    pub min_levels_median: usize,
    #[serde(default = "default_tp_type")]
    pub tp_type: String,
    #[serde(default = "default_tp_val")]
    pub tp_val: f64,
    #[serde(default = "default_sl_type")]
    pub sl_type: String,
    #[serde(default = "default_sl_val")]
    pub sl_val: f64,
    #[serde(default)]
    pub use_trailing: bool,
    #[serde(default = "default_trail_act")]
    pub trail_act: f64,
    #[serde(default = "default_trail_dist")]
    pub trail_dist: f64,
}

fn default_true() -> bool { true }
fn default_symbols() -> Vec<String> { vec!["BTCUSDT".to_string()] }
fn default_coin_source_mode() -> String { "Manual".to_string() }
fn default_max_screener_coins() -> usize { 10 }
fn default_min_24h_volume() -> f64 { 10000000.0 }
fn default_screener_refresh_min() -> u64 { 15 }
fn default_max_open_positions() -> usize { 3 }
fn default_timeframe() -> String { "1m".to_string() }
fn default_warmup_limit() -> usize { 1500 }
fn default_warmup_delay() -> f64 { 0.2 }
fn default_line_mode() -> String { "Anchor".to_string() }
fn default_vol_type() -> String { "Atr".to_string() }
fn default_lookback() -> usize { 200 }
fn default_vol_mult() -> f64 { 1.0 }
fn default_strategy_mode() -> String { "One-Way Trend".to_string() }
fn default_one_way_rule() -> String { "Reverse Jika Lawan Arah, Tahan Jika Searah".to_string() }
fn default_entry_mode() -> String { "Market Sniping (:59s)".to_string() }
fn default_limit_timeout_sec() -> f64 { 60.0 }
fn default_margin_pct() -> f64 { 5.0 }
fn default_leverage_mode() -> String { "Manual".to_string() }
fn default_leverage() -> u32 { 10 }
fn default_trailing_mode() -> String { "Local".to_string() }
fn default_selected_mode() -> String { "TREND".to_string() }
fn default_selected_cat() -> String { "cat_a".to_string() }
fn default_selected_preset() -> String { "TREND | TP 1M | SL NextSR".to_string() }
fn default_fixed1() -> f64 { 0.3 }
fn default_fixed2() -> f64 { 0.6 }
fn default_fixed3() -> f64 { 1.0 }
fn default_bar_tp1() -> usize { 3 }
fn default_bar_tp2() -> usize { 6 }
fn default_bar_tp_dir() -> usize { 3 }
fn default_sl_bar1() -> usize { 4 }
fn default_sl_bar2() -> usize { 8 }
fn default_sl_bar_adv() -> usize { 3 }
fn default_min_levels_median() -> usize { 10 }
fn default_tp_type() -> String { "Median %".to_string() }
fn default_tp_val() -> f64 { 1.0 }
fn default_sl_type() -> String { "NextSR Only".to_string() }
fn default_sl_val() -> f64 { 1.0 }
fn default_trail_act() -> f64 { 0.6 }
fn default_trail_dist() -> f64 { 0.3 }

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            secret_key: String::new(),
            use_testnet: true,
            symbols: default_symbols(),
            coin_source_mode: default_coin_source_mode(),
            max_screener_coins: default_max_screener_coins(),
            min_24h_volume_usdt: default_min_24h_volume(),
            screener_refresh_min: default_screener_refresh_min(),
            max_open_positions: default_max_open_positions(),
            timeframe: default_timeframe(),
            warmup_limit: default_warmup_limit(),
            warmup_delay: default_warmup_delay(),
            line_mode: default_line_mode(),
            vol_type: default_vol_type(),
            lookback: default_lookback(),
            vol_mult: default_vol_mult(),
            stdev_bessel: default_true(),
            strategy_mode: default_strategy_mode(),
            one_way_rule: default_one_way_rule(),
            entry_mode: default_entry_mode(),
            limit_timeout_sec: default_limit_timeout_sec(),
            margin_pct: default_margin_pct(),
            leverage_mode: default_leverage_mode(),
            leverage: default_leverage(),
            trailing_mode: default_trailing_mode(),
            selected_mode: default_selected_mode(),
            selected_cat: default_selected_cat(),
            selected_preset: default_selected_preset(),
            fixed1: default_fixed1(),
            fixed2: default_fixed2(),
            fixed3: default_fixed3(),
            bar_tp1: default_bar_tp1(),
            bar_tp2: default_bar_tp2(),
            bar_tp_dir: default_bar_tp_dir(),
            sl_bar1: default_sl_bar1(),
            sl_bar2: default_sl_bar2(),
            sl_bar_adv: default_sl_bar_adv(),
            min_levels_median: default_min_levels_median(),
            tp_type: default_tp_type(),
            tp_val: default_tp_val(),
            sl_type: default_sl_type(),
            sl_val: default_sl_val(),
            use_trailing: false,
            trail_act: default_trail_act(),
            trail_dist: default_trail_dist(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    pub side: String,
    pub entry: f64,
    pub qty: f64,
    pub bars_held: usize,
    pub dir_cnt: usize,
    pub adv_cnt: usize,
    pub unrealized_pnl: f64,
    pub highest_px: f64,
    pub lowest_px: f64,
    pub trail_stop: Option<f64>,
    pub trail_active: bool,
    pub candle_time: i64,
    pub algo_trailing_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub id: Option<serde_json::Value>,
    pub symbol: String,
    pub side: String,
    pub entry: f64,
    pub exit: f64,
    pub price: f64,
    pub qty: f64,
    pub pnl: f64,
    pub pct: f64,
    pub reason: String,
    pub time: String,
    pub raw_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub time: String,
    pub level: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceSummary {
    pub wallet_balance: f64,
    pub available_balance: f64,
    pub unrealized_pnl: f64,
    pub margin_used: f64,
    pub equity: f64,
}

impl Default for BalanceSummary {
    fn default() -> Self {
        Self {
            wallet_balance: 0.0,
            available_balance: 0.0,
            unrealized_pnl: 0.0,
            margin_used: 0.0,
            equity: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenerItem {
    pub symbol: String,
    pub live_price: Option<f64>,
    pub live_price_str: String,
    pub tick_dir: i32,
    pub countdown: String,
    pub rem_ms: i64,
    pub confirmed_sr: Option<f64>,
    pub confirmed_sr_str: String,
    pub confirmed_pct: Option<f64>,
    pub confirmed_dir: Option<String>,
    pub next_sr_active: bool,
    pub next_sr_dir: Option<String>,
    pub next_sr_price: Option<f64>,
    pub next_sr_price_str: String,
    pub next_sr_pct: Option<f64>,
    pub effective_leverage: u32,
    pub position: Option<PositionInfo>,
}
