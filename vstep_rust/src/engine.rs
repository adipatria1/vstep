use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Pos {
    pub side: i32, // 1 = Long, -1 = Short
    pub entry: f64,
    pub qty: f64,
    pub margin: f64,
    pub fee_in: f64, // Fee entry (Taker/Market) yang dibayar saat buka posisi
    pub tp: Option<f64>,
    pub sl: Option<f64>,
    pub tp_bar_t: i32, // 0 = bukan Bar TP | 3 = BarFix | 4 = BarProf | 5 = BarDir
    pub tp_bar_n: i32, // target N bar / candle searah
    pub dir_cnt: i32,  // hitungan candle searah berturut-turut
    pub tp_desc: String,
    pub sl_bar_t: i32, // 0 = bukan Bar SL | 3 = BarLoss | 4 = MaxBar | 5 = BarAdv
    pub sl_bar_n: i32, // target N bar / candle lawan untuk SL
    pub adv_cnt: i32,  // hitungan candle lawan arah berturut-turut
    pub sl_desc: String,
    pub act_pct: Option<f64>,
    pub dist_pct: Option<f64>,
    pub ext: f64,
    pub tstop: Option<f64>,
    pub bar: usize,
    pub entry_time: i64,
    pub lvl_start_time: Option<i64>,
    pub tr_trig_px: Option<f64>,
    pub tr_act_time: Option<i64>,
    pub tr_act_stop: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradeRec {
    pub side: String,
    #[serde(rename = "entryTime")]
    pub entry_time: String,
    #[serde(rename = "entryPx")]
    pub entry_px: f64,
    #[serde(rename = "tpPx")]
    pub tp_px: String,
    #[serde(rename = "slPx")]
    pub sl_px: String,
    #[serde(rename = "tpDesc")]
    pub tp_desc: String,
    #[serde(rename = "slDesc")]
    pub sl_desc: String,
    #[serde(rename = "exitTime")]
    pub exit_time: String,
    #[serde(rename = "exitPx")]
    pub exit_px: f64,
    pub why: String,
    #[serde(rename = "movePct")]
    pub move_pct: String,
    #[serde(rename = "netPnl")]
    pub net_pnl: String,
    pub pnl_color: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenPositionView {
    pub side: String,
    #[serde(rename = "entryTime")]
    pub entry_time: String,
    #[serde(rename = "entryPx")]
    pub entry_px: f64,
    pub qty: f64,
    pub margin: f64,
    #[serde(rename = "feeIn")]
    pub fee_in: f64,
    #[serde(rename = "tpPx")]
    pub tp_px: String,
    #[serde(rename = "slPx")]
    pub sl_px: String,
    #[serde(rename = "tpDesc")]
    pub tp_desc: String,
    #[serde(rename = "slDesc")]
    pub sl_desc: String,
    #[serde(rename = "floatPct")]
    pub float_pct: String,
    #[serde(rename = "floatUsdt")]
    pub float_usdt: String,
    pub pnl_color: String,
}

#[derive(Clone, Debug)]
pub struct Sim {
    pub name: String,
    pub key: String,
    pub mode: i32, // 0 Hedge | 1 One-Way Trend | 2 One-Way Counter
    pub tp_t: i32, // 0 tanpa TP | 1 median % | 2 fixed % | 3 BarFix | 4 BarProf | 5 BarDir
    pub tp_v: f64,
    pub sl_t: i32, // 0 Next SR | 1 median % | 2 fixed % | 3 BarLoss | 4 MaxBar | 5 BarAdv
    pub sl_v: f64,
    pub use_tr: bool,
    pub tr_u: i32, // 1 median % | 2 fixed %
    pub tr_a: f64, // aktivasi trailing
    pub tr_d: f64, // jarak trailing

    pub book: Vec<Pos>,
    pub trades: Vec<TradeRec>,
    pub bal: f64,
    pub peak_eq: f64,
    pub peak_bal: f64,
    pub dd_eq: f64,
    pub dd_bal: f64,
    pub liquid: bool,
    pub liq_time: Option<i64>,
    pub liq_bar: Option<usize>,
    pub liq_px: Option<f64>,

    pub n_long: i32,
    pub n_short: i32,
    pub n_win: i32,
    pub n_loss: i32,
    pub n_tp: i32,
    pub n_bar_tp: i32,
    pub n_stop: i32,
    pub n_bar_sl: i32,
    pub n_trail: i32,
    pub n_next: i32,
    pub n_skip: i32,

    pub fees: f64,
    pub gp: f64,
    pub gl: f64,
    pub max_vis_trades: usize,
}

impl Sim {
    pub fn new(
        name: String,
        key: String,
        mode: i32,
        tp_t: i32,
        tp_v: f64,
        sl_t: i32,
        sl_v: f64,
        use_tr: bool,
        tr_u: i32,
        tr_a: f64,
        tr_d: f64,
        init_cap: f64,
        max_vis_trades: usize,
    ) -> Self {
        Self {
            name,
            key,
            mode,
            tp_t,
            tp_v,
            sl_t,
            sl_v,
            use_tr,
            tr_u,
            tr_a,
            tr_d,
            book: Vec::with_capacity(20),
            trades: Vec::with_capacity(max_vis_trades),
            bal: init_cap,
            peak_eq: init_cap,
            peak_bal: init_cap,
            dd_eq: 0.0,
            dd_bal: 0.0,
            liquid: false,
            liq_time: None,
            liq_bar: None,
            liq_px: None,
            n_long: 0,
            n_short: 0,
            n_win: 0,
            n_loss: 0,
            n_tp: 0,
            n_bar_tp: 0,
            n_stop: 0,
            n_bar_sl: 0,
            n_trail: 0,
            n_next: 0,
            n_skip: 0,
            fees: 0.0,
            gp: 0.0,
            gl: 0.0,
            max_vis_trades,
        }
    }

    #[inline(always)]
    pub fn is_bar_tp(&self) -> bool {
        self.tp_t >= 3
    }

    #[inline(always)]
    pub fn is_bar_sl(&self) -> bool {
        self.sl_t >= 3
    }

    #[inline(always)]
    pub fn is_bar_any(&self) -> bool {
        self.tp_t >= 3 || self.sl_t >= 3
    }

    #[inline(always)]
    pub fn is_bar_both(&self) -> bool {
        self.tp_t >= 3 && self.sl_t >= 3
    }

    #[inline(always)]
    pub fn is_fixed_pct(&self) -> bool {
        self.tp_t < 3 && self.sl_t < 3 && (self.tp_t == 2 || self.sl_t == 2 || (self.use_tr && self.tr_u == 2))
    }

    #[inline(always)]
    pub fn is_median_pct(&self) -> bool {
        self.tp_t < 3 && self.sl_t < 3 && (self.tp_t == 1 || self.sl_t == 1 || (self.use_tr && self.tr_u == 1))
    }

    #[inline(always)]
    pub fn is_pct_any(&self) -> bool {
        self.tp_t < 3 && self.sl_t < 3 && (self.tp_t > 0 || self.sl_t > 0 || self.use_tr)
    }

    #[inline(always)]
    pub fn category_badge(&self) -> &'static str {
        if self.is_bar_both() {
            "bar2"
        } else if self.is_bar_tp() {
            "btp"
        } else if self.is_bar_sl() {
            "bsl"
        } else if self.is_fixed_pct() {
            "fix"
        } else if self.is_median_pct() {
            "med"
        } else {
            "neutral"
        }
    }

    #[inline(always)]
    pub fn scan_at(&self, px: f64) -> (f64, f64) {
        let mut f = 0.0;
        let mut n = 0.0;
        for p in &self.book {
            f += (p.side as f64) * p.qty * (px - p.entry);
            n += p.qty * px;
        }
        (f, n)
    }

    #[inline(always)]
    pub fn used_m(&self) -> f64 {
        self.book.iter().map(|p| p.margin).sum()
    }

    pub fn close_at(&mut self, i: usize, px: f64, fee_pct: f64, reason: i32, why: &str, _bar_idx: usize, t_ms: i64) {
        let p = self.book.remove(i);
        let gross_pnl = (p.side as f64) * p.qty * (px - p.entry);
        let fee_out = p.qty * px * fee_pct / 100.0;
        
        // Saldo bertambah grossPnl dikurangi feeOut (karena feeIn sudah dipotong dari saldo saat entry)
        self.bal += gross_pnl - fee_out;
        self.fees += fee_out;

        // Net PnL bulat setelah fee 2 arah (Round-trip)
        let net = gross_pnl - p.fee_in - fee_out;

        if net > 0.0 {
            self.n_win += 1;
            self.gp += net;
        } else {
            self.n_loss += 1;
            self.gl += -net;
        }

        if reason == 1 {
            self.n_tp += 1;
        } else if reason == 5 {
            self.n_tp += 1;
            self.n_bar_tp += 1;
        } else if reason == 2 {
            self.n_stop += 1;
        } else if reason == 6 {
            self.n_stop += 1;
            self.n_bar_sl += 1;
        } else if reason == 3 {
            self.n_stop += 1;
            self.n_trail += 1;
        } else {
            self.n_next += 1;
        }

        let move_pct = (p.side as f64) * (px - p.entry) / p.entry * 100.0;

        let dt_entry = DateTime::from_timestamp_millis(p.entry_time).unwrap_or_else(|| Utc::now());
        let dt_exit = DateTime::from_timestamp_millis(t_ms).unwrap_or_else(|| Utc::now());

        let rec = TradeRec {
            side: if p.side == 1 { "BUY".to_string() } else { "SELL".to_string() },
            entry_time: dt_entry.format("%Y-%m-%d %H:%M").to_string(),
            entry_px: (p.entry * 10000.0).round() / 10000.0,
            tp_px: p.tp.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "-".to_string()),
            sl_px: p.sl.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "NextSR".to_string()),
            tp_desc: p.tp_desc,
            sl_desc: p.sl_desc,
            exit_time: dt_exit.format("%Y-%m-%d %H:%M").to_string(),
            exit_px: (px * 10000.0).round() / 10000.0,
            why: why.to_string(),
            move_pct: format!("{:+0.2}%", move_pct),
            net_pnl: format!("{:+0.2} USDT", net),
            pnl_color: if net >= 0.0 { "good".to_string() } else { "bad".to_string() },
        };

        self.trades.push(rec);
        if self.trades.len() > self.max_vis_trades {
            self.trades.remove(0);
        }
    }

    pub fn process_bar(
        &mut self,
        new_sr: bool,
        sr_px: f64,
        prev_sr: Option<f64>,
        med_h: f64,
        med_l: f64,
        n_samp: usize,
        in_win: bool,
        bar_idx: usize,
        t_ms: i64,
        o_px: f64,
        h_px: f64,
        l_px: f64,
        c_px: f64,
        init_cap: f64,
        margin_pct: f64,
        margin_basis: &str,
        leverage: i32,
        fee_entry: f64,
        fee_exit: f64,
        mmr_pct: f64,
        max_open: usize,
        min_levels: usize,
        one_way_rule: &str,
    ) {
        if self.liquid {
            return;
        }

        let mut min_eq_pre = self.bal;

        // 1. Evaluasi posisi yang sudah terbuka dari bar sebelumnya
        if !self.book.is_empty() {
            let (f_lo, n_lo) = self.scan_at(l_px);
            let (f_hi, n_hi) = self.scan_at(h_px);
            let eq_lo = self.bal + f_lo;
            let eq_hi = self.bal + f_hi;
            min_eq_pre = eq_lo.min(eq_hi);

            if eq_lo <= n_lo * mmr_pct / 100.0 || eq_hi <= n_hi * mmr_pct / 100.0 {
                self.liquid = true;
                self.liq_time = Some(t_ms);
                self.liq_bar = Some(bar_idx);
                self.liq_px = Some(c_px);
                self.bal = 0.0;
                self.dd_eq = 100.0;
                self.dd_bal = 100.0;
                self.book.clear();
            } else {
                let mut i = self.book.len() as isize - 1;
                while i >= 0 {
                    let idx = i as usize;
                    if self.book[idx].lvl_start_time.is_none() {
                        self.book[idx].lvl_start_time = Some(t_ms);
                    }

                    let p = &mut self.book[idx];
                    let bars_held = bar_idx.saturating_sub(p.bar);

                    // Update hitungan candle searah & lawan arah pada penutupan candle
                    let is_dir_candle = if p.side == 1 { c_px > o_px } else { c_px < o_px };
                    let is_adv_candle = if p.side == 1 { c_px < o_px } else { c_px > o_px };
                    p.dir_cnt = if is_dir_candle { p.dir_cnt + 1 } else { 0 };
                    p.adv_cnt = if is_adv_candle { p.adv_cnt + 1 } else { 0 };

                    let eff_stop = if p.side == 1 {
                        match (p.sl, p.tstop) {
                            (None, t) => t,
                            (Some(s), None) => Some(s),
                            (Some(s), Some(t)) => Some(s.max(t)),
                        }
                    } else {
                        match (p.sl, p.tstop) {
                            (None, t) => t,
                            (Some(s), None) => Some(s),
                            (Some(s), Some(t)) => Some(s.min(t)),
                        }
                    };

                    let stop_hit = eff_stop.map(|s| if p.side == 1 { l_px <= s } else { h_px >= s }).unwrap_or(false);
                    let tp_hit = p.tp.map(|tp| if p.side == 1 { h_px >= tp } else { l_px <= tp }).unwrap_or(false);

                    // Evaluasi Bar-Based TP & Bar-Based SL
                    let close_gross = (p.side as f64) * p.qty * (c_px - p.entry);
                    let close_fee_out = p.qty * c_px * fee_entry / 100.0;
                    let close_net_pnl = close_gross - p.fee_in - close_fee_out;
                    let is_net_prof = close_net_pnl > 0.0;
                    let is_in_loss = close_net_pnl <= 0.0;

                    let mut bar_tp_hit = false;
                    let mut bar_tp_why = String::new();
                    let mut bar_sl_hit = false;
                    let mut bar_sl_why = String::new();

                    if p.tp_bar_t == 3 && bars_held >= p.tp_bar_n as usize {
                        bar_tp_hit = true;
                        bar_tp_why = format!("TP BAR ({}b)", bars_held);
                    } else if p.tp_bar_t == 4 && bars_held >= p.tp_bar_n as usize && is_net_prof {
                        bar_tp_hit = true;
                        bar_tp_why = format!("TP B-PROF ({}b)", bars_held);
                    } else if p.tp_bar_t == 5 && p.dir_cnt >= p.tp_bar_n && is_net_prof {
                        bar_tp_hit = true;
                        bar_tp_why = format!("TP B-DIR ({}c)", p.dir_cnt);
                    }

                    if p.sl_bar_t == 3 && bars_held >= p.sl_bar_n as usize && is_in_loss {
                        bar_sl_hit = true;
                        bar_sl_why = format!("SL B-LOSS ({}b)", bars_held);
                    } else if p.sl_bar_t == 4 && bars_held >= p.sl_bar_n as usize {
                        bar_sl_hit = true;
                        bar_sl_why = format!("SL MAX-BAR ({}b)", bars_held);
                    } else if p.sl_bar_t == 5 && p.adv_cnt >= p.sl_bar_n && is_in_loss {
                        bar_sl_hit = true;
                        bar_sl_why = format!("SL B-ADV ({}c)", p.adv_cnt);
                    }

                    // Urutan konservatif eksekusi intrabar:
                    // StopHit -> BarSl (Loss/Adv) -> TpHit -> BarTp -> MaxBar Timeout
                    if stop_hit {
                        let is_tr = p.tstop.is_some() && (p.sl.is_none() || (if p.side == 1 { p.tstop.unwrap() >= p.sl.unwrap() } else { p.tstop.unwrap() <= p.sl.unwrap() }));
                        let stop_px = eff_stop.unwrap();
                        self.close_at(idx, stop_px, fee_entry, if is_tr { 3 } else { 2 }, if is_tr { "TRAIL" } else { "SL" }, bar_idx, t_ms);
                    } else if bar_sl_hit && p.sl_bar_t != 4 {
                        self.close_at(idx, c_px, fee_entry, 6, &bar_sl_why, bar_idx, t_ms);
                    } else if tp_hit {
                        let target_tp = p.tp.unwrap();
                        self.close_at(idx, target_tp, fee_exit, 1, "TP", bar_idx, t_ms);
                    } else if bar_tp_hit {
                        self.close_at(idx, c_px, fee_entry, 5, &bar_tp_why, bar_idx, t_ms);
                    } else if bar_sl_hit {
                        self.close_at(idx, c_px, fee_entry, 6, &bar_sl_why, bar_idx, t_ms);
                    } else if let (Some(act_pct), Some(dist_pct)) = (p.act_pct, p.dist_pct) {
                        p.ext = if p.side == 1 { p.ext.max(h_px) } else { p.ext.min(l_px) };
                        let fav_pct = if p.side == 1 {
                            (p.ext - p.entry) / p.entry * 100.0
                        } else {
                            (p.entry - p.ext) / p.entry * 100.0
                        };
                        if fav_pct >= act_pct {
                            let ns = if p.side == 1 {
                                p.ext * (1.0 - dist_pct / 100.0)
                            } else {
                                p.ext * (1.0 + dist_pct / 100.0)
                            };
                            if p.tstop.is_none() {
                                p.tstop = Some(ns);
                                p.tr_act_time = Some(t_ms);
                                p.tr_act_stop = Some(ns);
                            } else {
                                let cur = p.tstop.unwrap();
                                p.tstop = Some(if p.side == 1 { cur.max(ns) } else { cur.min(ns) });
                            }
                        }
                    }
                    i -= 1;
                }
            }
        }

        if !self.liquid {
            // 2. Penanganan Posisi Lama Saat Terbentuk SR Baru
            if new_sr && !self.book.is_empty() {
                if self.sl_t == 0 {
                    // Strategi SL NextSR: wajib tutup semua posisi lama di harga SR baru
                    let mut j = self.book.len() as isize - 1;
                    while j >= 0 {
                        self.close_at(j as usize, sr_px, fee_entry, 4, "NEXT SR", bar_idx, t_ms);
                        j -= 1;
                    }
                } else if self.mode != 0 && prev_sr.is_some() {
                    // One-Way Mode (Trend/Counter) dengan SL Tetap / Bar-SL: terapkan aturan oneWayRule
                    let p_sr = prev_sr.unwrap();
                    let up_step_now = sr_px > p_sr;
                    let new_sig_dir = if self.mode == 1 {
                        if up_step_now { 1 } else { -1 }
                    } else {
                        if up_step_now { -1 } else { 1 }
                    };

                    if one_way_rule == "Tutup Posisi Lama & Buka Baru di SR Baru" {
                        let mut j = self.book.len() as isize - 1;
                        while j >= 0 {
                            self.close_at(j as usize, sr_px, fee_entry, 4, "NEXT SR", bar_idx, t_ms);
                            j -= 1;
                        }
                    } else if one_way_rule == "Reverse Jika Lawan Arah, Tahan Jika Searah" {
                        let mut j = self.book.len() as isize - 1;
                        while j >= 0 {
                            if self.book[j as usize].side != new_sig_dir {
                                self.close_at(j as usize, sr_px, fee_entry, 4, "REV SR", bar_idx, t_ms);
                            }
                            j -= 1;
                        }
                    }
                    // Opsi 'Tahan Posisi Lama (Skip SR Baru Jika Masih Open)': posisi lama tetap dibiarkan!
                }
            }

            // 3. Entry Baru Tepat di Bar & Harga SR Saat Terbentuk
            if new_sr && prev_sr.is_some() && in_win {
                let p_sr = prev_sr.unwrap();
                let need_med = self.tp_t == 1 || self.sl_t == 1 || (self.use_tr && self.tr_u == 1);
                let max_pos_allowed = if self.mode == 0 { max_open } else { 1 };

                if !need_med || n_samp >= min_levels {
                    for k in 0..2 {
                        let dir = if k == 0 { 1 } else { -1 };
                        let up_step = sr_px > p_sr;
                        let allowed = match self.mode {
                            0 => true,
                            1 => if up_step { dir == 1 } else { dir == -1 },
                            _ => if up_step { dir == -1 } else { dir == 1 },
                        };

                        if allowed {
                            let fav_m = if dir == 1 { med_h } else { med_l };
                            let adv_m = if dir == 1 { med_l } else { med_h };
                            let is_bar_tp = self.tp_t >= 3;
                            let is_bar_sl = self.sl_t >= 3;

                            let tp_p = match self.tp_t {
                                1 => Some(fav_m * self.tp_v),
                                2 => Some(self.tp_v),
                                _ => None,
                            };
                            let sl_p = match self.sl_t {
                                1 => Some(adv_m * self.sl_v),
                                2 => Some(self.sl_v),
                                _ => None,
                            };
                            let ac_p = if self.use_tr {
                                Some(if self.tr_u == 1 { fav_m * self.tr_a } else { self.tr_a })
                            } else {
                                None
                            };
                            let ds_p = if self.use_tr {
                                Some(if self.tr_u == 1 { fav_m * self.tr_d } else { self.tr_d })
                            } else {
                                None
                            };

                            let ok = (self.tp_t == 0 || is_bar_tp || tp_p.map(|v| v > 0.0).unwrap_or(false))
                                && (self.sl_t == 0 || is_bar_sl || sl_p.map(|v| v > 0.0).unwrap_or(false))
                                && (!self.use_tr || (ac_p.map(|v| v > 0.0).unwrap_or(false) && ds_p.map(|v| v > 0.0).unwrap_or(false)));

                            let base = if margin_basis == "Initial Balance" { init_cap } else { self.bal };
                            let mrg = base * margin_pct / 100.0;
                            let notional = mrg * (leverage as f64);
                            let fee_in = notional * fee_entry / 100.0;
                            let (f_c, _) = self.scan_at(c_px);
                            let avail_eq = self.bal + f_c - self.used_m();

                            if ok {
                                if mrg > 0.0 && mrg + fee_in <= avail_eq && self.book.len() < max_pos_allowed {
                                    let qty = notional / sr_px;
                                    let tp_px = tp_p.map(|p| sr_px * (1.0 + (dir as f64) * p / 100.0));
                                    let sl_px = sl_p.map(|p| sr_px * (1.0 - (dir as f64) * p / 100.0));
                                    let tr_trig_px = ac_p.map(|p| sr_px * (1.0 + (dir as f64) * p / 100.0));

                                    let b_tp_type = if is_bar_tp { self.tp_t } else { 0 };
                                    let b_tp_tgt = if is_bar_tp { self.tp_v as i32 } else { 0 };
                                    let b_sl_type = if is_bar_sl { self.sl_t } else { 0 };
                                    let b_sl_tgt = if is_bar_sl { self.sl_v as i32 } else { 0 };

                                    let tp_label = if let Some(tp) = tp_px {
                                        format!("{:.4}", tp)
                                    } else if self.use_tr {
                                        "TRAIL".to_string()
                                    } else if b_tp_type == 3 {
                                        format!("BarFix({}b)", b_tp_tgt)
                                    } else if b_tp_type == 4 {
                                        format!("BarProf(>={}b)", b_tp_tgt)
                                    } else if b_tp_type == 5 {
                                        format!("BarDir({}c)", b_tp_tgt)
                                    } else {
                                        "-".to_string()
                                    };

                                    let sl_label = if let Some(sl) = sl_px {
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

                                    self.book.push(Pos {
                                        side: dir,
                                        entry: sr_px,
                                        qty,
                                        margin: mrg,
                                        fee_in,
                                        tp: tp_px,
                                        sl: sl_px,
                                        tp_bar_t: b_tp_type,
                                        tp_bar_n: b_tp_tgt,
                                        dir_cnt: 0,
                                        tp_desc: tp_label,
                                        sl_bar_t: b_sl_type,
                                        sl_bar_n: b_sl_tgt,
                                        adv_cnt: 0,
                                        sl_desc: sl_label,
                                        act_pct: ac_p,
                                        dist_pct: ds_p,
                                        ext: sr_px,
                                        tstop: None,
                                        bar: bar_idx,
                                        entry_time: t_ms,
                                        lvl_start_time: None,
                                        tr_trig_px,
                                        tr_act_time: None,
                                        tr_act_stop: None,
                                    });

                                    // Fee entry dipotong langsung dari saldo
                                    self.bal -= fee_in;
                                    self.fees += fee_in;

                                    if dir == 1 {
                                        self.n_long += 1;
                                    } else {
                                        self.n_short += 1;
                                    }
                                } else {
                                    self.n_skip += 1;
                                }
                            }
                        }
                    }
                }
            }

            // 4. Max Drawdown (equity worst-case & balance)
            let (f_cl, _) = self.scan_at(c_px);
            let eq_close = self.bal + f_cl;
            let worst = min_eq_pre.min(eq_close);

            if self.peak_eq > 0.0 {
                self.dd_eq = self.dd_eq.max((self.peak_eq - worst) / self.peak_eq * 100.0);
            }
            self.peak_eq = self.peak_eq.max(eq_close);

            if self.peak_bal > 0.0 {
                self.dd_bal = self.dd_bal.max((self.peak_bal - self.bal) / self.peak_bal * 100.0);
            }
            self.peak_bal = self.peak_bal.max(self.bal);
        }
    }

    #[inline(always)]
    pub fn net_pct(&self, c_px: f64, init_cap: f64) -> f64 {
        let (f, _) = self.scan_at(c_px);
        let eq = if self.liquid { 0.0 } else { self.bal + f };
        ((eq - init_cap) / init_cap) * 100.0
    }

    #[inline(always)]
    pub fn pf(&self) -> f64 {
        if self.gl > 0.0 {
            self.gp / self.gl
        } else if self.gp > 0.0 {
            99.0
        } else {
            0.0
        }
    }

    pub fn score(&self, c_px: f64, init_cap: f64, rank_by: &str) -> f64 {
        let n = self.n_win + self.n_loss;
        if n == 0 {
            return -1_000_000_000.0;
        }
        let nt = self.net_pct(c_px, init_cap);
        match rank_by {
            "Net Profit %" => nt,
            "Profit Factor" => self.pf(),
            _ => nt / self.dd_eq.max(1.0),
        }
    }
}

pub fn calculate_volatility(
    high: &[f64],
    low: &[f64],
    close: &[f64],
    vol_type: &str,
    vol_length: usize,
    vol_mult: f64,
) -> Vec<f64> {
    let n = close.len();
    let mut vol_value = vec![f64::NAN; n];

    if vol_type == "Stdev" {
        if n >= vol_length {
            for i in (vol_length - 1)..n {
                let window = &close[(i + 1 - vol_length)..=i];
                let mean = window.iter().sum::<f64>() / (vol_length as f64);
                let mut sum_sq_diff = 0.0;
                for &v in window {
                    let d = v - mean;
                    sum_sq_diff += d * d;
                }
                let std = (sum_sq_diff / ((vol_length - 1) as f64)).sqrt();
                vol_value[i] = std * vol_mult;
            }
        }
    } else {
        // ATR with Wilder's RMA
        let mut tr = vec![0.0; n];
        if n > 0 {
            tr[0] = high[0] - low[0];
            for i in 1..n {
                let tr1 = high[i] - low[i];
                let tr2 = (high[i] - close[i - 1]).abs();
                let tr3 = (low[i] - close[i - 1]).abs();
                tr[i] = tr1.max(tr2).max(tr3);
            }
        }
        if n >= vol_length {
            let initial_sma: f64 = tr[..vol_length].iter().sum::<f64>() / (vol_length as f64);
            vol_value[vol_length - 1] = initial_sma * vol_mult;
            let inv_len = 1.0 / (vol_length as f64);
            let one_minus_inv = 1.0 - inv_len;
            let mut curr = initial_sma;
            for i in vol_length..n {
                curr = curr * one_minus_inv + tr[i] * inv_len;
                vol_value[i] = curr * vol_mult;
            }
        }
    }
    vol_value
}

pub fn calculate_step_sr(close: &[f64], vol_value: &[f64], line_mode: &str) -> Vec<f64> {
    let n = close.len();
    let mut confirmed_sr = vec![0.0; n];
    if n == 0 {
        return confirmed_sr;
    }
    let mut cur_sr = close[0];
    confirmed_sr[0] = cur_sr;

    for i in 1..n {
        let vv = vol_value[i];
        let c = close[i];
        if !vv.is_nan() && (c - cur_sr).abs() > vv {
            if line_mode == "Anchor" {
                cur_sr = c;
            } else {
                cur_sr = cur_sr + (if c > cur_sr { 1.0 } else { -1.0 }) * vv;
            }
        }
        confirmed_sr[i] = cur_sr;
    }
    confirmed_sr
}

pub fn median(arr: &[f64]) -> f64 {
    if arr.is_empty() {
        return f64::NAN;
    }
    let mut sorted = arr.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let len = sorted.len();
    if len % 2 == 0 {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
    } else {
        sorted[len / 2]
    }
}

pub fn create_sim_suite(
    inc_hedge: bool,
    inc_trend: bool,
    inc_counter: bool,
    inc_bar_tp: bool,
    inc_bar_sl: bool,
    fixed1: f64,
    fixed2: f64,
    fixed3: f64,
    bar_tp1: i32,
    bar_tp2: i32,
    bar_tp_dir: i32,
    sl_bar1: i32,
    sl_bar2: i32,
    sl_bar_adv: i32,
    init_cap: f64,
    sel_strategy: &str,
) -> Vec<Sim> {
    let mut sims = Vec::with_capacity(105);
    let f1 = format!("{:.2}", fixed1).trim_end_matches('0').trim_end_matches('.').to_string();
    let f2 = format!("{:.2}", fixed2).trim_end_matches('0').trim_end_matches('.').to_string();
    let f3 = format!("{:.2}", fixed3).trim_end_matches('0').trim_end_matches('.').to_string();
    let b1 = bar_tp1.to_string();
    let b2 = bar_tp2.to_string();
    let bd = bar_tp_dir.to_string();
    let s1 = sl_bar1.to_string();
    let s2 = sl_bar2.to_string();
    let sa = sl_bar_adv.to_string();

    let add_set = |sims: &mut Vec<Sim>, pfx: &str, mode: i32| {
        let p = format!("{} | ", pfx);

        // 1. Strategi Base Persen (13 Strategi)
        sims.push(Sim::new(format!("{}TP 0.5M | SL NextSR", p), format!("{}TP 0.5M | SL NextSR", p), mode, 1, 0.5, 0, 0.0, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}TP 1M | SL NextSR", p), format!("{}TP 1M | SL NextSR", p), mode, 1, 1.0, 0, 0.0, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}NextSR only", p), format!("{}NextSR only", p), mode, 0, 0.0, 0, 0.0, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}TP 0.5M | SL 1M", p), format!("{}TP 0.5M | SL 1M", p), mode, 1, 0.5, 1, 1.0, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}TP 1M | SL 1M", p), format!("{}TP 1M | SL 1M", p), mode, 1, 1.0, 1, 1.0, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}TP 1M | SL 0.5M", p), format!("{}TP 1M | SL 0.5M", p), mode, 1, 1.0, 1, 0.5, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}TP F1 ({}%) | SL F1 ({}%)", p, f1, f1), format!("{}TP F1 | SL F1", p), mode, 2, fixed1, 2, fixed1, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}TP F2 ({}%) | SL F1 ({}%)", p, f2, f1), format!("{}TP F2 | SL F1", p), mode, 2, fixed2, 2, fixed1, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}TP F3 ({}%) | SL F2 ({}%)", p, f3, f2), format!("{}TP F3 | SL F2", p), mode, 2, fixed3, 2, fixed2, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}TP F2 ({}%) | SL NextSR", p, f2), format!("{}TP F2 | SL NextSR", p), mode, 2, fixed2, 0, 0.0, false, 1, 0.0, 0.0, init_cap, 30));
        sims.push(Sim::new(format!("{}Trail 0.5M/0.5M | SL NextSR", p), format!("{}Trail 0.5M/0.5M | SL NextSR", p), mode, 0, 0.0, 0, 0.0, true, 1, 0.5, 0.5, init_cap, 30));
        sims.push(Sim::new(format!("{}Trail 1M/0.5M | SL NextSR", p), format!("{}Trail 1M/0.5M | SL NextSR", p), mode, 0, 0.0, 0, 0.0, true, 1, 1.0, 0.5, init_cap, 30));
        sims.push(Sim::new(format!("{}Trail F2/F1 ({}%/{}) | SL F1 ({}%)", p, f2, f1, f1), format!("{}Trail F2/F1 | SL F1", p), mode, 0, 0.0, 2, fixed1, true, 2, fixed2, fixed1, init_cap, 30));

        // 2. Strategi Base Bar - TP (8 Strategi)
        let need_bar_tp = inc_bar_tp || sel_strategy.contains("BarFix") || sel_strategy.contains("BarProf") || sel_strategy.contains("BarDir");
        if need_bar_tp {
            sims.push(Sim::new(format!("{}TP BarFix B1 ({}b) | SL NextSR", p, b1), format!("{}TP BarFix B1 | SL NextSR", p), mode, 3, bar_tp1 as f64, 0, 0.0, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarFix B2 ({}b) | SL NextSR", p, b2), format!("{}TP BarFix B2 | SL NextSR", p), mode, 3, bar_tp2 as f64, 0, 0.0, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarFix B1 ({}b) | SL F1 ({}%)", p, b1, f1), format!("{}TP BarFix B1 | SL F1", p), mode, 3, bar_tp1 as f64, 2, fixed1, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarProf B1 (>={}b) | SL NextSR", p, b1), format!("{}TP BarProf B1 | SL NextSR", p), mode, 4, bar_tp1 as f64, 0, 0.0, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarProf B2 (>={}b) | SL NextSR", p, b2), format!("{}TP BarProf B2 | SL NextSR", p), mode, 4, bar_tp2 as f64, 0, 0.0, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarProf B1 (>={}b) | SL F1 ({}%)", p, b1, f1), format!("{}TP BarProf B1 | SL F1", p), mode, 4, bar_tp1 as f64, 2, fixed1, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarDir BD ({}c) | SL NextSR", p, bd), format!("{}TP BarDir BD | SL NextSR", p), mode, 5, bar_tp_dir as f64, 0, 0.0, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarDir BD ({}c) | SL F1 ({}%)", p, bd, f1), format!("{}TP BarDir BD | SL F1", p), mode, 5, bar_tp_dir as f64, 2, fixed1, false, 1, 0.0, 0.0, init_cap, 30));
        }

        // 3. Strategi Base Bar - SL (8 Strategi)
        let need_bar_sl = inc_bar_sl || sel_strategy.contains("BarLoss") || sel_strategy.contains("MaxBar") || sel_strategy.contains("BarAdv");
        if need_bar_sl {
            sims.push(Sim::new(format!("{}TP 1M | SL BarLoss S1 (>={}b)", p, s1), format!("{}TP 1M | SL BarLoss S1", p), mode, 1, 1.0, 3, sl_bar1 as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP F2 ({}%) | SL BarLoss S1 (>={}b)", p, f2, s1), format!("{}TP F2 | SL BarLoss S1", p), mode, 2, fixed2, 3, sl_bar1 as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}Trail 1M/0.5M | SL BarLoss S1 (>={}b)", p, s1), format!("{}Trail 1M/0.5M | SL BarLoss S1", p), mode, 0, 0.0, 3, sl_bar1 as f64, true, 1, 1.0, 0.5, init_cap, 30));
            sims.push(Sim::new(format!("{}TP 1M | SL MaxBar S1 ({}b)", p, s1), format!("{}TP 1M | SL MaxBar S1", p), mode, 1, 1.0, 4, sl_bar1 as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP F2 ({}%) | SL MaxBar S2 ({}b)", p, f2, s2), format!("{}TP F2 | SL MaxBar S2", p), mode, 2, fixed2, 4, sl_bar2 as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP 1M | SL BarAdv SA ({}c)", p, sa), format!("{}TP 1M | SL BarAdv SA", p), mode, 1, 1.0, 5, sl_bar_adv as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP F2 ({}%) | SL BarAdv SA ({}c)", p, f2, sa), format!("{}TP F2 | SL BarAdv SA", p), mode, 2, fixed2, 5, sl_bar_adv as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}Trail F2/F1 ({}%/{}) | SL BarAdv SA ({}c)", p, f2, f1, sa), format!("{}Trail F2/F1 | SL BarAdv SA", p), mode, 0, 0.0, 5, sl_bar_adv as f64, true, 2, fixed2, fixed1, init_cap, 30));
        }

        // 4. Strategi Base Bar Murni (6 Strategi)
        let need_bar_both = (inc_bar_tp && inc_bar_sl) || sel_strategy.contains("Bar");
        if need_bar_both {
            sims.push(Sim::new(format!("{}TP BarFix B2 ({}b) | SL BarLoss S1 (>={}b)", p, b2, s1), format!("{}TP BarFix B2 | SL BarLoss S1", p), mode, 3, bar_tp2 as f64, 3, sl_bar1 as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarProf B1 (>={}b) | SL BarLoss S1 (>={}b)", p, b1, s1), format!("{}TP BarProf B1 | SL BarLoss S1", p), mode, 4, bar_tp1 as f64, 3, sl_bar1 as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarProf B1 (>={}b) | SL MaxBar S2 ({}b)", p, b1, s2), format!("{}TP BarProf B1 | SL MaxBar S2", p), mode, 4, bar_tp1 as f64, 4, sl_bar2 as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarProf B1 (>={}b) | SL BarAdv SA ({}c)", p, b1, sa), format!("{}TP BarProf B1 | SL BarAdv SA", p), mode, 4, bar_tp1 as f64, 5, sl_bar_adv as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarDir BD ({}c) | SL BarLoss S1 (>={}b)", p, bd, s1), format!("{}TP BarDir BD | SL BarLoss S1", p), mode, 5, bar_tp_dir as f64, 3, sl_bar1 as f64, false, 1, 0.0, 0.0, init_cap, 30));
            sims.push(Sim::new(format!("{}TP BarDir BD ({}c) | SL BarAdv SA ({}c)", p, bd, sa), format!("{}TP BarDir BD | SL BarAdv SA", p), mode, 5, bar_tp_dir as f64, 5, sl_bar_adv as f64, false, 1, 0.0, 0.0, init_cap, 30));
        }
    };

    if inc_trend || sel_strategy.starts_with("TREND") {
        add_set(&mut sims, "TREND", 1);
    }
    if inc_counter || sel_strategy.starts_with("COUNTER") {
        add_set(&mut sims, "COUNTER", 2);
    }
    if inc_hedge || sel_strategy.starts_with("HEDGE") {
        add_set(&mut sims, "HEDGE", 0);
    }

    if sims.is_empty() {
        add_set(&mut sims, "TREND", 1);
    }

    sims
}

pub fn get_all_strategy_keys() -> Vec<String> {
    let mut keys = Vec::with_capacity(105);
    for pfx in ["TREND", "COUNTER", "HEDGE"] {
        let p = format!("{} | ", pfx);
        keys.extend(vec![
            // Base Persen (13)
            format!("{}TP 0.5M | SL NextSR", p),
            format!("{}TP 1M | SL NextSR", p),
            format!("{}NextSR only", p),
            format!("{}TP 0.5M | SL 1M", p),
            format!("{}TP 1M | SL 1M", p),
            format!("{}TP 1M | SL 0.5M", p),
            format!("{}TP F1 | SL F1", p),
            format!("{}TP F2 | SL F1", p),
            format!("{}TP F3 | SL F2", p),
            format!("{}TP F2 | SL NextSR", p),
            format!("{}Trail 0.5M/0.5M | SL NextSR", p),
            format!("{}Trail 1M/0.5M | SL NextSR", p),
            format!("{}Trail F2/F1 | SL F1", p),
            // Base Bar - TP (8)
            format!("{}TP BarFix B1 | SL NextSR", p),
            format!("{}TP BarFix B2 | SL NextSR", p),
            format!("{}TP BarFix B1 | SL F1", p),
            format!("{}TP BarProf B1 | SL NextSR", p),
            format!("{}TP BarProf B2 | SL NextSR", p),
            format!("{}TP BarProf B1 | SL F1", p),
            format!("{}TP BarDir BD | SL NextSR", p),
            format!("{}TP BarDir BD | SL F1", p),
            // Base Bar - SL (8)
            format!("{}TP 1M | SL BarLoss S1", p),
            format!("{}TP F2 | SL BarLoss S1", p),
            format!("{}Trail 1M/0.5M | SL BarLoss S1", p),
            format!("{}TP 1M | SL MaxBar S1", p),
            format!("{}TP F2 | SL MaxBar S2", p),
            format!("{}TP 1M | SL BarAdv SA", p),
            format!("{}TP F2 | SL BarAdv SA", p),
            format!("{}Trail F2/F1 | SL BarAdv SA", p),
            // Base Bar Murni (6)
            format!("{}TP BarFix B2 | SL BarLoss S1", p),
            format!("{}TP BarProf B1 | SL BarLoss S1", p),
            format!("{}TP BarProf B1 | SL MaxBar S2", p),
            format!("{}TP BarProf B1 | SL BarAdv SA", p),
            format!("{}TP BarDir BD | SL BarLoss S1", p),
            format!("{}TP BarDir BD | SL BarAdv SA", p),
        ]);
    }
    keys
}

pub fn pass_filter(cand: &Sim, strat_filter: &str) -> bool {
    match strat_filter {
        "Semua Strategi" => true,
        "Base Bar (Semua TP / SL Bar)" => cand.is_bar_any(),
        "Base Bar Murni (TP Bar + SL Bar)" => cand.is_bar_both(),
        "Base Bar - TP Saja" => cand.is_bar_tp(),
        "Base Bar - SL Saja" => cand.is_bar_sl(),
        "Base Persen (Semua Fixed % & Median %)" => cand.is_pct_any(),
        "Base Persen Tetap (Fixed F1/F2/F3 %)" => cand.is_fixed_pct(),
        "Base Persen Median (0.5M / 1M %)" => cand.is_median_pct(),
        _ => true,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LeaderboardRow {
    pub rank: usize,
    pub name: String,
    pub raw_name: String,
    pub key: String,
    pub is_selected: bool,
    pub net_pct: f64,
    pub net_pct_str: String,
    pub dd_eq: f64,
    pub dd_eq_str: String,
    pub dd_bal: f64,
    pub dd_bal_str: String,
    pub win_pct: String,
    pub trades: i32,
    pub pf: f64,
    pub pf_str: String,
    pub liq: String,
    pub badge: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CategoryWinner {
    pub category_code: String, // "#1 ALL", "#X BAR2", "#X B-TP", "#X B-SL", "#X FIX%", "#X MED%"
    pub label: String,
    pub name: String,
    pub key: String,
    pub rank: usize,
    pub net_pct: f64,
    pub net_pct_str: String,
    pub dd_eq: f64,
    pub dd_eq_str: String,
    pub dd_bal: f64,
    pub dd_bal_str: String,
    pub win_pct: String,
    pub trades: i32,
    pub pf: f64,
    pub pf_str: String,
    pub liq: String,
    pub color_code: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DetailRow {
    pub key: String,
    pub val: String,
    pub color: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BacktestResult {
    pub leaderboard: LeaderboardSection,
    pub detail: DetailSection,
    pub summary: SummarySection,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LeaderboardSection {
    pub title: String,
    pub headers: Vec<String>,
    pub rows: Vec<LeaderboardRow>,
    pub category_winners: Vec<CategoryWinner>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DetailSection {
    pub title: String,
    pub rows: Vec<DetailRow>,
    pub recent_trades: Vec<TradeRec>,
    pub open_positions: Vec<OpenPositionView>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SummarySection {
    pub total_sims: usize,
    pub filtered_sims: usize,
    pub candles_count: usize,
    pub start_date: String,
    pub end_date: String,
    pub last_price: f64,
}

pub fn run_backtest(
    times: &[i64],
    open_px: &[f64],
    high: &[f64],
    low: &[f64],
    close: &[f64],
    line_mode: &str,
    vol_type: &str,
    vol_length: usize,
    vol_mult: f64,
    realtime_update: bool,
    init_cap: f64,
    margin_pct: f64,
    margin_basis: &str,
    leverage: i32,
    fee_entry: f64,
    fee_exit: f64,
    mmr_pct: f64,
    one_way_rule: &str,
    max_open: usize,
    bt_days: i32,
    inc_hedge: bool,
    inc_trend: bool,
    inc_counter: bool,
    inc_bar_tp: bool,
    inc_bar_sl: bool,
    fixed1: f64,
    fixed2: f64,
    fixed3: f64,
    bar_tp1: i32,
    bar_tp2: i32,
    bar_tp_dir: i32,
    sl_bar1: i32,
    sl_bar2: i32,
    sl_bar_adv: i32,
    min_levels: usize,
    rank_by: &str,
    strat_filter: &str,
    top_n: usize,
    sel_strategy: &str,
) -> BacktestResult {
    let n = close.len();
    if n == 0 {
        panic!("Candle data is empty");
    }

    // 1. Volatility & Step S/R
    let vol_value = calculate_volatility(high, low, close, vol_type, vol_length, vol_mult);
    let confirmed_sr = calculate_step_sr(close, &vol_value, line_mode);

    // 2. Sims Suite (Base Persen + Base Bar)
    let mut sims = create_sim_suite(
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
        init_cap,
        sel_strategy,
    );

    let mut hi_arr = Vec::with_capacity(1000);
    let mut lo_arr = Vec::with_capacity(1000);
    let mut sim_level: Option<f64> = None;
    let mut sim_hi: Option<f64> = None;
    let mut sim_lo: Option<f64> = None;

    let timenow_ms = times[n - 1];

    let mut med_h = f64::NAN;
    let mut med_l = f64::NAN;

    for i in 0..n {
        let c_sr = confirmed_sr[i];
        let h = high[i];
        let l = low[i];
        let c = close[i];
        let o = open_px[i];
        let t = times[i];
        let in_win = bt_days == 0 || (t >= timenow_ms - (bt_days as i64) * 86_400_000);

        // Active SR sinkronisasi
        let active_sr = if realtime_update {
            // Intrabar step level logic
            let step_level = if line_mode == "Anchor" {
                c
            } else {
                let dir_sign = if c > c_sr { 1.0 } else { -1.0 };
                c_sr + dir_sign * vol_value[i]
            };
            if !vol_value[i].is_nan() && (c - c_sr).abs() > vol_value[i] {
                step_level
            } else {
                c_sr
            }
        } else {
            c_sr
        };

        let mut new_sr = false;
        let prev_sr = sim_level;

        match sim_level {
            None => {
                sim_level = Some(active_sr);
            }
            Some(curr) if active_sr != curr => {
                if let (Some(sh), Some(sl)) = (sim_hi, sim_lo) {
                    if curr != 0.0 {
                        hi_arr.push(((sh - curr) / curr * 100.0).max(0.0));
                        lo_arr.push(((curr - sl) / curr * 100.0).max(0.0));
                        med_h = median(&hi_arr);
                        med_l = median(&lo_arr);
                    }
                }
                sim_level = Some(active_sr);
                sim_hi = None;
                sim_lo = None;
                new_sr = true;
            }
            _ => {
                sim_hi = Some(sim_hi.map(|v| v.max(h)).unwrap_or(h));
                sim_lo = Some(sim_lo.map(|v| v.min(l)).unwrap_or(l));
            }
        }

        let n_smp = hi_arr.len();

        for s in &mut sims {
            if !s.liquid && (!s.book.is_empty() || new_sr) {
                s.process_bar(
                    new_sr,
                    active_sr,
                    prev_sr,
                    med_h,
                    med_l,
                    n_smp,
                    in_win,
                    i,
                    t,
                    o,
                    h,
                    l,
                    c,
                    init_cap,
                    margin_pct,
                    margin_basis,
                    leverage,
                    fee_entry,
                    fee_exit,
                    mmr_pct,
                    max_open,
                    min_levels,
                    one_way_rule,
                );
            }
        }
    }

    let last_c = close[n - 1];

    // Rank ALL strategies
    let mut scored: Vec<(usize, f64)> = sims.iter().enumerate().map(|(idx, s)| (idx, s.score(last_c, init_cap, rank_by))).collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let sorted_sims: Vec<Sim> = scored.into_iter().map(|(idx, _)| sims[idx].clone()).collect();

    // Collect filtered strategies & category winners
    let mut filt_sims: Vec<Sim> = Vec::with_capacity(sorted_sims.len());
    let mut filt_ranks: Vec<usize> = Vec::with_capacity(sorted_sims.len());

    let best_overall_sim = sorted_sims[0].clone();
    let best_overall_rank = 1usize;
    let mut best_bar_both: Option<(Sim, usize)> = None;
    let mut best_bar_tp: Option<(Sim, usize)> = None;
    let mut best_bar_sl: Option<(Sim, usize)> = None;
    let mut best_fixed_pct: Option<(Sim, usize)> = None;
    let mut best_med_pct: Option<(Sim, usize)> = None;

    for (r, cand) in sorted_sims.iter().enumerate() {
        let global_rank = r + 1;
        if pass_filter(cand, strat_filter) {
            filt_sims.push(cand.clone());
            filt_ranks.push(global_rank);
        }
        if cand.is_bar_both() && best_bar_both.is_none() {
            best_bar_both = Some((cand.clone(), global_rank));
        }
        if cand.is_bar_tp() && best_bar_tp.is_none() {
            best_bar_tp = Some((cand.clone(), global_rank));
        }
        if cand.is_bar_sl() && best_bar_sl.is_none() {
            best_bar_sl = Some((cand.clone(), global_rank));
        }
        if cand.is_fixed_pct() && best_fixed_pct.is_none() {
            best_fixed_pct = Some((cand.clone(), global_rank));
        }
        if cand.is_median_pct() && best_med_pct.is_none() {
            best_med_pct = Some((cand.clone(), global_rank));
        }
    }

    // Identify selected strategy for Visual / Detail
    let mut vis_sim: Option<Sim> = None;
    let mut sel_rank: Option<usize> = None;

    if sel_strategy == "Top #1 Sesuai Filter Leaderboard (Otomatis)" {
        if !filt_sims.is_empty() {
            vis_sim = Some(filt_sims[0].clone());
            sel_rank = Some(filt_ranks[0]);
        } else {
            vis_sim = Some(best_overall_sim.clone());
            sel_rank = Some(best_overall_rank);
        }
    } else if sel_strategy == "Top #1 Semua Strategi (Otomatis)" {
        vis_sim = Some(best_overall_sim.clone());
        sel_rank = Some(best_overall_rank);
    } else if sel_strategy == "Top #1 Base Bar Murni (TP Bar + SL Bar Terbaik)" {
        if let Some((s, r)) = &best_bar_both {
            vis_sim = Some(s.clone());
            sel_rank = Some(*r);
        } else {
            vis_sim = Some(best_overall_sim.clone());
            sel_rank = Some(best_overall_rank);
        }
    } else if sel_strategy == "Top #1 Base Bar TP (Bar TP Terbaik)" {
        if let Some((s, r)) = &best_bar_tp {
            vis_sim = Some(s.clone());
            sel_rank = Some(*r);
        } else {
            vis_sim = Some(best_overall_sim.clone());
            sel_rank = Some(best_overall_rank);
        }
    } else if sel_strategy == "Top #1 Base Bar SL (Bar SL Terbaik)" {
        if let Some((s, r)) = &best_bar_sl {
            vis_sim = Some(s.clone());
            sel_rank = Some(*r);
        } else {
            vis_sim = Some(best_overall_sim.clone());
            sel_rank = Some(best_overall_rank);
        }
    } else if sel_strategy == "Top #1 Base Persen Tetap (Fixed % Terbaik)" {
        if let Some((s, r)) = &best_fixed_pct {
            vis_sim = Some(s.clone());
            sel_rank = Some(*r);
        } else {
            vis_sim = Some(best_overall_sim.clone());
            sel_rank = Some(best_overall_rank);
        }
    } else if sel_strategy == "Top #1 Base Persen Median (Median % Terbaik)" {
        if let Some((s, r)) = &best_med_pct {
            vis_sim = Some(s.clone());
            sel_rank = Some(*r);
        } else {
            vis_sim = Some(best_overall_sim.clone());
            sel_rank = Some(best_overall_rank);
        }
    } else if sel_strategy != "None" {
        for (r_idx, s) in sorted_sims.iter().enumerate() {
            if s.key == sel_strategy {
                vis_sim = Some(s.clone());
                sel_rank = Some(r_idx + 1);
                break;
            }
        }
    }

    // Build Leaderboard
    let ld_headers = vec![
        "#".to_string(),
        "Strategi".to_string(),
        "Net %".to_string(),
        "DD Eq %".to_string(),
        "DD Bal %".to_string(),
        "Win %".to_string(),
        "Trade".to_string(),
        "PF".to_string(),
        "Liq".to_string(),
    ];

    let shown_n = top_n.min(filt_sims.len());
    let mut ld_rows = Vec::with_capacity(shown_n);

    for r in 0..shown_n {
        let b = &filt_sims[r];
        let glb_rank = filt_ranks[r];
        let nt = b.net_pct(last_c, init_cap);
        let n_trades = b.n_win + b.n_loss;
        let is_sel = vis_sim.as_ref().map(|v| v.key == b.key).unwrap_or(false);
        let nm_tx = if is_sel {
            format!("[PILIH] {}", b.name)
        } else {
            b.name.clone()
        };
        let win_pct_str = if n_trades > 0 {
            format!("{:.1}%", (b.n_win as f64) * 100.0 / (n_trades as f64))
        } else {
            "-".to_string()
        };
        let pf_val = b.pf();

        ld_rows.push(LeaderboardRow {
            rank: glb_rank,
            name: nm_tx,
            raw_name: b.name.clone(),
            key: b.key.clone(),
            is_selected: is_sel,
            net_pct: (nt * 100.0).round() / 100.0,
            net_pct_str: format!("{:+0.2}%", nt),
            dd_eq: (b.dd_eq * 100.0).round() / 100.0,
            dd_eq_str: format!("{:.2}%", b.dd_eq),
            dd_bal: (b.dd_bal * 100.0).round() / 100.0,
            dd_bal_str: format!("{:.2}%", b.dd_bal),
            win_pct: win_pct_str,
            trades: n_trades,
            pf: (pf_val * 100.0).round() / 100.0,
            pf_str: format!("{:.2}", pf_val),
            liq: if b.liquid { "YA".to_string() } else { "-".to_string() },
            badge: b.category_badge().to_string(),
        });
    }

    // Build 6 Category Winners comparison rows
    let mut cat_winners: Vec<CategoryWinner> = Vec::with_capacity(6);

    let create_cat_winner = |s: &Sim, r: usize, code: &str, lbl: &str, col: &str| {
        let nt = s.net_pct(last_c, init_cap);
        let n_trades = s.n_win + s.n_loss;
        CategoryWinner {
            category_code: code.to_string(),
            label: lbl.to_string(),
            name: s.name.clone(),
            key: s.key.clone(),
            rank: r,
            net_pct: (nt * 100.0).round() / 100.0,
            net_pct_str: format!("{:+0.2}%", nt),
            dd_eq: (s.dd_eq * 100.0).round() / 100.0,
            dd_eq_str: format!("{:.2}%", s.dd_eq),
            dd_bal: (s.dd_bal * 100.0).round() / 100.0,
            dd_bal_str: format!("{:.2}%", s.dd_bal),
            win_pct: if n_trades > 0 {
                format!("{:.1}%", (s.n_win as f64) * 100.0 / (n_trades as f64))
            } else {
                "-".to_string()
            },
            trades: n_trades,
            pf: (s.pf() * 100.0).round() / 100.0,
            pf_str: format!("{:.2}", s.pf()),
            liq: if s.liquid { "YA".to_string() } else { "-".to_string() },
            color_code: col.to_string(),
        }
    };

    // 1. #1 ALL
    cat_winners.push(create_cat_winner(
        &best_overall_sim,
        1,
        "#1 ALL",
        "[TERBAIK ALL]",
        "gold",
    ));

    // 2. BAR2 (Base Bar Murni)
    if let Some((s, r)) = &best_bar_both {
        cat_winners.push(create_cat_winner(
            s,
            *r,
            &format!("#{} BAR2", r),
            "[BASE BAR TP+SL]",
            "bar2",
        ));
    }

    // 3. B-TP (Base Bar TP)
    if let Some((s, r)) = &best_bar_tp {
        cat_winners.push(create_cat_winner(
            s,
            *r,
            &format!("#{} B-TP", r),
            "[BASE BAR TP]",
            "btp",
        ));
    }

    // 4. B-SL (Base Bar SL)
    if let Some((s, r)) = &best_bar_sl {
        cat_winners.push(create_cat_winner(
            s,
            *r,
            &format!("#{} B-SL", r),
            "[BASE BAR SL]",
            "bsl",
        ));
    }

    // 5. FIX% (Base Persen Fixed)
    if let Some((s, r)) = &best_fixed_pct {
        cat_winners.push(create_cat_winner(
            s,
            *r,
            &format!("#{} FIX%", r),
            "[BASE PERSEN FIXED]",
            "fix",
        ));
    }

    // 6. MED% (Base Persen Median)
    if let Some((s, r)) = &best_med_pct {
        cat_winners.push(create_cat_winner(
            s,
            *r,
            &format!("#{} MED%", r),
            "[BASE PERSEN MEDIAN]",
            "med",
        ));
    }

    // Build Detail View
    let bs = vis_sim.as_ref().unwrap_or(&best_overall_sim);
    let disp_rnk = sel_rank.unwrap_or(1);

    let (bf, _) = bs.scan_at(last_c);
    let eq_now = if bs.liquid { 0.0 } else { bs.bal + bf };
    let um = bs.used_m();
    let net = eq_now - init_cap;
    let net_p = (net / init_cap) * 100.0;
    let n_tr = bs.n_win + bs.n_loss;

    let hdr_prefix = if vis_sim.is_some() {
        format!("DETAIL PILIHAN (Rank #{}): ", disp_rnk)
    } else {
        "DETAIL TOP #1 (Pilih Strategi = None): ".to_string()
    };
    let dt_header = format!("{}{}", hdr_prefix, bs.name);

    let liq_time_str = bs
        .liq_time
        .map(|lt| {
            DateTime::from_timestamp_millis(lt)
                .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let md_h_val = if med_h.is_nan() { 0.0 } else { med_h };
    let md_l_val = if med_l.is_nan() { 0.0 } else { med_l };

    let cat_desc = if bs.is_bar_both() {
        "Base Bar Murni (TP+SL Bar)"
    } else if bs.is_bar_tp() {
        "Base Bar TP"
    } else if bs.is_bar_sl() {
        "Base Bar SL"
    } else if bs.is_fixed_pct() {
        "Base Persen Tetap"
    } else if bs.is_median_pct() {
        "Base Persen Median"
    } else {
        "NextSR Only"
    };

    let mode_desc = if bs.mode == 0 {
        "Hedge Mode".to_string()
    } else {
        format!("One-Way ({})", one_way_rule)
    };

    let dt_rows = vec![
        DetailRow {
            key: "Peringkat Strategi".into(),
            val: format!("#{} dari {} ({})", disp_rnk, sorted_sims.len(), rank_by),
            color: "highlight".into(),
        },
        DetailRow {
            key: "Mode & Kategori".into(),
            val: format!("{} | {}", mode_desc, cat_desc),
            color: if bs.is_bar_both() {
                "good".into()
            } else if bs.is_bar_any() {
                "highlight".into()
            } else {
                "neutral".into()
            },
        },
        DetailRow {
            key: "Saldo Awal / Akhir".into(),
            val: format!("{:.2} / {:.2} USDT", init_cap, if bs.liquid { 0.0 } else { bs.bal }),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Equity (Floating)".into(),
            val: format!("{:.2} USDT", eq_now),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Net Profit (Round-Trip)".into(),
            val: format!("{:.2} ({:+0.2}%)", net, net_p),
            color: if net >= 0.0 { "good".into() } else { "bad".into() },
        },
        DetailRow {
            key: "Margin / Entry & Lev".into(),
            val: format!(
                "{:.1}% ({}) | {}x",
                margin_pct,
                if margin_basis == "Initial Balance" { "Awal" } else { "Saldo" },
                leverage
            ),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Fee Entry / Exit TP".into(),
            val: format!("{:.3}% / {:.3}% (2 Arah)", fee_entry, fee_exit),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Median H / |L| (SR selesai)".into(),
            val: format!("{:.3}% / {:.3}%", md_h_val, md_l_val),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Entry Long / Short".into(),
            val: format!("{} / {}", bs.n_long, bs.n_short),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Exit TP Persen / Bar-TP".into(),
            val: format!("{} / {}", bs.n_tp - bs.n_bar_tp, bs.n_bar_tp),
            color: "highlight".into(),
        },
        DetailRow {
            key: "Exit SL Persen / Bar-SL / Trail / NextSR".into(),
            val: format!(
                "{} / {} / {} / {}",
                bs.n_stop - bs.n_trail - bs.n_bar_sl,
                bs.n_bar_sl,
                bs.n_trail,
                bs.n_next
            ),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Win / Loss (Net Fee)".into(),
            val: format!(
                "{} / {}{}",
                bs.n_win,
                bs.n_loss,
                if n_tr > 0 {
                    format!(" ({:.1}%)", (bs.n_win as f64) * 100.0 / (n_tr as f64))
                } else {
                    "".to_string()
                }
            ),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Profit Factor (Net Fee)".into(),
            val: format!("{:.2}", bs.pf()),
            color: if bs.pf() >= 1.0 { "good".into() } else { "bad".into() },
        },
        DetailRow {
            key: "Posisi Open".into(),
            val: format!("{} ({:+0.2} USDT)", bs.book.len(), bf),
            color: if bf >= 0.0 { "good".into() } else { "bad".into() },
        },
        DetailRow {
            key: "Margin Terpakai".into(),
            val: format!(
                "{:.2} USDT{}",
                um,
                if eq_now > 0.0 {
                    format!(" ({:.1}%)", um / eq_now * 100.0)
                } else {
                    "".to_string()
                }
            ),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Total Fee Dibayar".into(),
            val: format!("{:.2} USDT", bs.fees),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Entry Dilewati (Skip)".into(),
            val: format!("{}", bs.n_skip),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Max DD Equity / Balance".into(),
            val: format!("{:.2}% / {:.2}%", bs.dd_eq, bs.dd_bal),
            color: if bs.dd_eq > 0.0 { "bad".into() } else { "neutral".into() },
        },
        DetailRow {
            key: "Likuidasi (Saldo Habis)".into(),
            val: if bs.liquid {
                format!("YA - {}", liq_time_str)
            } else {
                "TIDAK".into()
            },
            color: if bs.liquid { "bad".into() } else { "good".into() },
        },
        DetailRow {
            key: "SR Selesai (Sampel)".into(),
            val: format!("{}", hi_arr.len()),
            color: "neutral".into(),
        },
        DetailRow {
            key: "Terbaik Base Bar (TP+SL)".into(),
            val: best_bar_both
                .as_ref()
                .map(|(s, r)| format!("#{} {}", r, s.key))
                .unwrap_or_else(|| "-".into()),
            color: "good".into(),
        },
        DetailRow {
            key: "Terbaik Base Persen Fixed".into(),
            val: best_fixed_pct
                .as_ref()
                .map(|(s, r)| format!("#{} {}", r, s.key))
                .unwrap_or_else(|| "-".into()),
            color: "highlight".into(),
        },
        DetailRow {
            key: "Kode Dropdown".into(),
            val: bs.key.clone(),
            color: "highlight".into(),
        },
    ];

    let mut open_positions = Vec::with_capacity(bs.book.len());
    for p in &bs.book {
        let e_time = DateTime::from_timestamp_millis(p.entry_time)
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let fl_pct = (p.side as f64) * (last_c - p.entry) / p.entry * 100.0;
        let est_fee_out = p.qty * last_c * fee_entry / 100.0;
        let fl_net_usdt = (p.side as f64) * p.qty * (last_c - p.entry) - p.fee_in - est_fee_out;

        open_positions.push(OpenPositionView {
            side: if p.side == 1 { "BUY".into() } else { "SELL".into() },
            entry_time: e_time,
            entry_px: (p.entry * 10000.0).round() / 10000.0,
            qty: (p.qty * 10000.0).round() / 10000.0,
            margin: (p.margin * 100.0).round() / 100.0,
            fee_in: (p.fee_in * 1000.0).round() / 1000.0,
            tp_px: p.tp.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "-".into()),
            sl_px: p.sl.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "NextSR".into()),
            tp_desc: p.tp_desc.clone(),
            sl_desc: p.sl_desc.clone(),
            float_pct: format!("{:+0.2}%", fl_pct),
            float_usdt: format!("{:+0.2} USDT", fl_net_usdt),
            pnl_color: if fl_net_usdt >= 0.0 { "good".into() } else { "bad".into() },
        });
    }

    let mut recent_trades = bs.trades.clone();
    recent_trades.reverse();

    let dt_start = DateTime::from_timestamp_millis(times[0])
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    let dt_end = DateTime::from_timestamp_millis(times[n - 1])
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();

    BacktestResult {
        leaderboard: LeaderboardSection {
            title: format!(
                "LEADERBOARD | Filter: {} ({}/{}) | Rank: {}",
                strat_filter,
                filt_sims.len(),
                sims.len(),
                rank_by
            ),
            headers: ld_headers,
            rows: ld_rows,
            category_winners: cat_winners,
        },
        detail: DetailSection {
            title: dt_header,
            rows: dt_rows,
            recent_trades,
            open_positions,
        },
        summary: SummarySection {
            total_sims: sims.len(),
            filtered_sims: filt_sims.len(),
            candles_count: n,
            start_date: dt_start,
            end_date: dt_end,
            last_price: last_c,
        },
    }
}
