use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tungstenite::connect;

use crate::binance_api::{
    fetch_top_screened_coins, get_live_positions, get_max_leverage, market_close_position,
    market_open_position, panic_close_all_positions, set_symbol_leverage, test_testnet_account,
    AccountBalanceInfo, LivePositionInfo,
};
use crate::credentials::load_credentials;
use crate::engine::{calculate_step_sr, calculate_volatility, run_backtest};
use crate::fetcher::fetch_binance_fapi_klines_by_count;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub time: String,
    pub level: String, // "info", "success", "warning", "error", "trade"
    pub message: String,
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
    pub execution_mode: String,     // "pre_close", "realtime_no_lag", "on_close"
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
            volatility_type: "Stdev".to_string(),
            volatility_length: 200,
            vol_mult: 4.0,
            execution_mode: "pre_close".to_string(),
            pre_close_buffer_ms: 1000,

            coin_mode: "top_gainers".to_string(),
            manual_symbol: "BTCUSDT".to_string(),
            manual_symbols: vec!["BTCUSDT".to_string()],
            interval: "15m".to_string(),
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
    pub current_sr: f64,
    pub current_vol: f64,
    pub active_leverage: i32,
    pub last_evaluated_bar: i64,
    pub position_bars_held: i32,
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
            current_sr: 0.0,
            current_vol: 0.0,
            active_leverage: 10,
            last_evaluated_bar: 0,
            position_bars_held: 0,
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
                lock.position_bars_held = 0;

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

                if !is_running {
                    continue;
                }

                // 1. Tentukan target symbol dari Screener atau Manual
                let target_symbol = match cfg.coin_mode.as_str() {
                    "top_gainers" => {
                        match fetch_top_screened_coins(cfg.max_active_slots, "top_gainers") {
                            Ok(coins) => {
                                if tick_counter % 10 == 0 {
                                    let names: Vec<String> = coins.iter().map(|c| format!("{}({:+0.1}%)", c.symbol, c.price_change_percent)).collect();
                                    let mut lock = state_arc.lock().unwrap();
                                    lock.add_log("info", &format!("🔍 [SCREENER TOP GAINERS 24H]: {}", names.join(", ")));
                                }
                                coins.first().map(|c| c.symbol.clone()).unwrap_or(cfg.manual_symbol.clone())
                            }
                            Err(_) => cfg.manual_symbol.clone(),
                        }
                    }
                    "top_volume" => {
                        match fetch_top_screened_coins(cfg.max_active_slots, "top_volume") {
                            Ok(coins) => {
                                if tick_counter % 10 == 0 {
                                    let names: Vec<String> = coins.iter().map(|c| format!("{}({}M)", c.symbol, c.quote_volume)).collect();
                                    let mut lock = state_arc.lock().unwrap();
                                    lock.add_log("info", &format!("📊 [SCREENER TOP VOLUME 24H]: {}", names.join(", ")));
                                }
                                coins.first().map(|c| c.symbol.clone()).unwrap_or(cfg.manual_symbol.clone())
                            }
                            Err(_) => cfg.manual_symbol.clone(),
                        }
                    }
                    _ => cfg.manual_symbol.clone(),
                };

                // 2. Warm-up check untuk target_symbol
                let needs_warmup = {
                    let lock = state_arc.lock().unwrap();
                    lock.last_warmup_symbol != target_symbol
                };

                if needs_warmup {
                    let mut lock = state_arc.lock().unwrap();
                    let warmup_target = cfg.warmup_klines;
                    lock.add_log("info", &format!("⏳ Memulai Warm-up Backtest untuk {} (TF: {}, target: {} candle)...", target_symbol, cfg.interval, warmup_target));
                    drop(lock);

                    // Fetch klines sesuai target candle (default 10.000 kline)
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

                        // Hitung level S/R dan volatilitas terkini
                        let vol_series = calculate_volatility(
                            &data.high,
                            &data.low,
                            &data.close,
                            &cfg.volatility_type,
                            cfg.volatility_length,
                            cfg.vol_mult,
                        );
                        let sr_series = calculate_step_sr(&data.close, &vol_series, &cfg.line_mode);
                        let cur_sr = *sr_series.last().unwrap_or(&data.close[data.close.len() - 1]);
                        let cur_vol = *vol_series.last().unwrap_or(&0.0);

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
                        lock.current_sr = cur_sr;
                        lock.current_vol = cur_vol;
                        lock.active_leverage = applied_lev;
                        lock.add_log(
                            "success",
                            &format!(
                                "🔥 [WARM-UP SELESAI] {} ({} candle). Juara: {} | Current S/R: {:.2} (Band: {:.2}) | Leverage: {}x",
                                target_symbol,
                                data.times.len(),
                                chosen_strat,
                                cur_sr,
                                cur_vol,
                                applied_lev
                            ),
                        );
                    }
                }
            }
        });
    }

    /// Background Worker 2: WebSocket Kline Streaming & Pre-Close Sniping
    fn spawn_websocket_worker(&self) {
        let state_arc = self.0.clone();

        thread::spawn(move || {
            loop {
                // Tunggu sampai bot running dan target koin sudah warm-up
                let (is_running, symbol, interval, cfg, cur_sr, _cur_vol, strat, lev) = {
                    let lock = state_arc.lock().unwrap();
                    (
                        lock.config.is_running,
                        lock.last_warmup_symbol.clone(),
                        lock.config.interval.clone(),
                        lock.config.clone(),
                        lock.current_sr,
                        lock.current_vol,
                        lock.last_selected_strategy.clone(),
                        lock.active_leverage,
                    )
                };

                if !is_running || symbol.is_empty() {
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }

                let stream_sym = symbol.to_lowercase();
                // Hubungkan ke Binance Futures WebSocket live kline feed
                let ws_url = format!("wss://fstream.binance.com/ws/{}@kline_{}", stream_sym, interval);

                {
                    let mut lock = state_arc.lock().unwrap();
                    let exec_str = match cfg.execution_mode.as_str() {
                        "pre_close" => format!("Pre-Close Sniping ({}ms sebelum bar close)", cfg.pre_close_buffer_ms),
                        "realtime_no_lag" => "Realtime No Lag".to_string(),
                        _ => "On Candle Close".to_string(),
                    };
                    lock.add_log("info", &format!("📡 [WS STREAM CONNECTED] wss://fstream.binance.com/ws/{}@kline_{} | Mode: {}", stream_sym, interval, exec_str));
                }

                match connect(&ws_url) {
                    Ok((mut socket, _)) => {
                        loop {
                            // Cek apakah bot masih jalan dan target symbol belum berganti
                            let (still_running, current_target) = {
                                let lock = state_arc.lock().unwrap();
                                (lock.config.is_running, lock.last_warmup_symbol.clone())
                            };

                            if !still_running || current_target != symbol {
                                let _ = socket.close(None);
                                break;
                            }

                            // Baca frame WebSocket dari Binance
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

                            if let Some(k) = val.get("k") {
                                let t_start = k["t"].as_i64().unwrap_or(0);
                                let t_close = k["T"].as_i64().unwrap_or(0);
                                let is_closed = k["x"].as_bool().unwrap_or(false);
                                let c_px = k["c"].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                let now_ms = Utc::now().timestamp_millis();
                                let time_left_ms = t_close - now_ms;

                                let creds = load_credentials();

                                // Evaluasi trigger sniping sesuai execution_mode
                                let (should_evaluate, is_sniper_trigger) = match cfg.execution_mode.as_str() {
                                    "pre_close" => {
                                        let in_snipe_window = time_left_ms <= cfg.pre_close_buffer_ms as i64 && time_left_ms > -3000;
                                        let lock = state_arc.lock().unwrap();
                                        let not_evaluated = lock.last_evaluated_bar != t_start;
                                        (in_snipe_window && not_evaluated, true)
                                    }
                                    "realtime_no_lag" => (true, false),
                                    _ => {
                                        // on_close
                                        let lock = state_arc.lock().unwrap();
                                        (is_closed && lock.last_evaluated_bar != t_start, false)
                                    }
                                };

                                if should_evaluate && cur_sr > 0.0 && c_px > 0.0 {
                                    // Tandai candle ini sudah dievaluasi untuk menghindari order ganda
                                    {
                                        let mut lock = state_arc.lock().unwrap();
                                        lock.last_evaluated_bar = t_start;
                                    }

                                    // Breakout check
                                    let is_breakout_up = c_px > cur_sr;
                                    let is_breakout_down = c_px < cur_sr;

                                    let sig_side = if strat.contains("COUNTER") {
                                        if is_breakout_up { Some("SELL") } else if is_breakout_down { Some("BUY") } else { None }
                                    } else {
                                        // TREND
                                        if is_breakout_up { Some("BUY") } else if is_breakout_down { Some("SELL") } else { None }
                                    };

                                    if let Some(side) = sig_side {
                                        let (wallet_bal, existing_pos) = {
                                            let lock = state_arc.lock().unwrap();
                                            let bal = lock.cached_balance.as_ref().map(|b| b.wallet_balance).unwrap_or(1000.0);
                                            let pos = lock.cached_positions.iter().find(|p| p.symbol == symbol).cloned();
                                            (bal, pos)
                                        };

                                        // Terapkan One-Way Rule
                                        let mut allow_entry = true;
                                        if let Some(ref pos) = existing_pos {
                                            if cfg.one_way_rule == "Tutup Posisi Lama & Buka Baru di SR Baru" {
                                                let mut lock = state_arc.lock().unwrap();
                                                lock.add_log("info", &format!("🔄 [ONE-WAY] Menutup posisi lama {} {} sebelum entry baru di SR baru.", pos.side, pos.symbol));
                                                let _ = market_close_position(&pos.symbol, &pos.side, pos.position_amt, &creds.demo_api_key, &creds.demo_secret_key);
                                            } else if cfg.one_way_rule == "Reverse Jika Lawan Arah, Tahan Jika Searah" {
                                                if pos.side == side {
                                                    let mut lock = state_arc.lock().unwrap();
                                                    lock.add_log("info", &format!("🛡️ [ONE-WAY] Sinyal {} searah dengan posisi aktif {}. Posisi dipertahankan (Hold).", side, pos.symbol));
                                                    allow_entry = false;
                                                } else {
                                                    let mut lock = state_arc.lock().unwrap();
                                                    lock.add_log("info", &format!("🔄 [ONE-WAY REVERSE] Membalik arah posisi dari {} ke {}.", pos.side, side));
                                                    let _ = market_close_position(&pos.symbol, &pos.side, pos.position_amt, &creds.demo_api_key, &creds.demo_secret_key);
                                                }
                                            } else {
                                                // Tahan Posisi Lama
                                                let mut lock = state_arc.lock().unwrap();
                                                lock.add_log("info", &format!("🛡️ [ONE-WAY SKIP] Posisi {} {} masih terbuka. Melewatkan sinyal baru.", pos.symbol, pos.side));
                                                allow_entry = false;
                                            }
                                        }

                                        if allow_entry {
                                            // Hitung ukuran margin dan jumlah koin
                                            let slot_margin = wallet_bal * (cfg.margin_pct_per_slot / 100.0);
                                            let notional = slot_margin * (lev as f64);
                                            let raw_qty = notional / c_px;

                                            match market_open_position(&symbol, side, raw_qty, &creds.demo_api_key, &creds.demo_secret_key) {
                                                Ok((order_id, _actual_qty, qty_str)) => {
                                                    let mut lock = state_arc.lock().unwrap();
                                                    lock.position_bars_held = 0;
                                                    let tag = if is_sniper_trigger {
                                                        format!("🎯 [PRE-CLOSE SNIPER (Sisa: {}ms)]", time_left_ms)
                                                    } else {
                                                        "⚡ [SIGNAL EXECUTED]".to_string()
                                                    };
                                                    lock.add_log(
                                                        "trade",
                                                        &format!(
                                                            "{} Order #{} {} {} {} @ {:.2} BERHASIL! | Lev: {}x | Margin: {:.2} USDT | Strategi: {}",
                                                            tag, order_id, side, qty_str, symbol, c_px, lev, slot_margin, strat
                                                        ),
                                                    );
                                                }
                                                Err(e) => {
                                                    let mut lock = state_arc.lock().unwrap();
                                                    lock.add_log("error", &format!("❌ Gagal eksekusi order {} {}: {}", side, symbol, e));
                                                }
                                            }
                                        }
                                    }
                                }

                                // 3. Pada candle penutupan (is_closed == true)
                                if is_closed {
                                    let mut lock = state_arc.lock().unwrap();
                                    // Update S/R level untuk candle berikutnya
                                    if (c_px - lock.current_sr).abs() > lock.current_vol && lock.current_vol > 0.0 {
                                        let old_sr = lock.current_sr;
                                        let new_sr = if lock.config.line_mode == "Anchor" {
                                            c_px
                                        } else {
                                            lock.current_sr + (if c_px > lock.current_sr { 1.0 } else { -1.0 }) * lock.current_vol
                                        };
                                        lock.current_sr = new_sr;
                                        lock.add_log("info", &format!("📈 [S/R UPDATED] {} Garis S/R bergeser: {:.2} ➔ {:.2}", symbol, old_sr, new_sr));
                                    }

                                    // Cek Bar-Based TP dan SL untuk posisi terbuka
                                    lock.position_bars_held += 1;
                                    let bars_held = lock.position_bars_held;

                                    if let Some(pos) = lock.cached_positions.iter().find(|p| p.symbol == symbol).cloned() {
                                        let mut trigger_close = false;
                                        let mut reason = String::new();

                                        let target_tp_bar = if strat.contains("B2") { lock.config.bar_tp2 } else if strat.contains("B1") { lock.config.bar_tp1 } else if strat.contains("BD") { lock.config.bar_tp_dir } else { 0 };
                                        let target_sl_bar = if strat.contains("S2") { lock.config.sl_bar2 } else if strat.contains("S1") { lock.config.sl_bar1 } else if strat.contains("SA") { lock.config.sl_bar_adv } else { 0 };

                                        if target_tp_bar > 0 && bars_held >= target_tp_bar && pos.unrealized_profit > 0.0 {
                                            trigger_close = true;
                                            reason = format!("TP BAR ({}b)", bars_held);
                                        } else if target_sl_bar > 0 && bars_held >= target_sl_bar {
                                            trigger_close = true;
                                            reason = format!("SL BAR ({}b)", bars_held);
                                        }

                                        if trigger_close {
                                            lock.add_log("trade", &format!("🏁 [EXIT {}] Menutup posisi {} {} (PnL: {:.2} USDT)...", reason, pos.side, pos.symbol, pos.unrealized_profit));
                                            let _ = market_close_position(&pos.symbol, &pos.side, pos.position_amt, &creds.demo_api_key, &creds.demo_secret_key);
                                            lock.position_bars_held = 0;
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
