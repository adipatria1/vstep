use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandleData {
    pub times: Vec<i64>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub volume: Vec<f64>,
    pub symbol: String,
    pub interval: String,
    pub days: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FetchProgress {
    pub status: String,
    pub message: String,
    pub percent: f64,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_left: Option<u32>,
}

pub fn get_interval_ms(interval: &str) -> i64 {
    match interval {
        "1m" => 60 * 1000,
        "3m" => 3 * 60 * 1000,
        "5m" => 5 * 60 * 1000,
        "15m" => 15 * 60 * 1000,
        "30m" => 30 * 60 * 1000,
        "1h" => 60 * 60 * 1000,
        "2h" => 2 * 60 * 60 * 1000,
        "4h" => 4 * 60 * 60 * 1000,
        "6h" => 6 * 60 * 60 * 1000,
        "8h" => 8 * 60 * 60 * 1000,
        "12h" => 12 * 60 * 60 * 1000,
        "1d" => 24 * 60 * 60 * 1000,
        "3d" => 3 * 24 * 60 * 1000,
        "1w" => 7 * 24 * 60 * 1000,
        "1M" => 30 * 24 * 60 * 1000,
        _ => 60 * 60 * 1000,
    }
}

pub fn get_cache_path(symbol: &str, interval: &str, days: u32) -> PathBuf {
    let clean_sym = symbol.to_uppercase().replace('/', "");
    let mut dir = crate::get_data_dir().join("cache");
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir.push(format!("{}_{}_{}d.json", clean_sym, interval, days));
    dir
}

pub fn load_cached_data(symbol: &str, interval: &str, days: u32) -> Option<CandleData> {
    let path = get_cache_path(symbol, interval, days);
    if path.exists() {
        if let Ok(file) = File::open(&path) {
            let reader = BufReader::new(file);
            if let Ok(data) = serde_json::from_reader(reader) {
                return Some(data);
            }
        }
    }
    None
}

pub fn save_cache_data(data: &CandleData) {
    let path = get_cache_path(&data.symbol, &data.interval, data.days);
    if let Ok(file) = File::create(&path) {
        let writer = BufWriter::new(file);
        let _ = serde_json::to_writer(writer, data);
    }
}

pub fn fetch_binance_fapi_klines<F>(
    symbol: &str,
    interval: &str,
    days: u32,
    use_cache: bool,
    mut progress_cb: F,
) -> Result<CandleData, String>
where
    F: FnMut(FetchProgress),
{
    let clean_sym = symbol.trim().to_uppercase().replace('/', "");

    if use_cache {
        if let Some(cached) = load_cached_data(&clean_sym, interval, days) {
            if !cached.times.is_empty() {
                progress_cb(FetchProgress {
                    status: "loaded_from_cache".into(),
                    message: format!("Data dimuat dari disk cache: {} candles", cached.times.len()),
                    percent: 100.0,
                    count: cached.times.len(),
                    pause_left: None,
                });
                return Ok(cached);
            }
        }
    }

    let now_ms = Utc::now().timestamp_millis();
    let total_ms = (days as i64) * 86_400_000;
    let start_ms = now_ms - total_ms;
    let end_ms = now_ms;

    let url = "https://fapi.binance.com/fapi/v1/klines";
    let step_ms = get_interval_ms(interval);

    let ms_360_days = 360 * 86_400_000;
    let mut next_milestone_day = 360;
    let mut next_milestone_ms = start_ms + ms_360_days;

    let mut all_times = Vec::new();
    let mut all_open = Vec::new();
    let mut all_high = Vec::new();
    let mut all_low = Vec::new();
    let mut all_close = Vec::new();
    let mut all_volume = Vec::new();

    let mut curr_start = start_ms;
    let mut req_count = 0;

    progress_cb(FetchProgress {
        status: "fetching_started".into(),
        message: format!("Memulai fetch Binance Futures: {} [{}] untuk {} hari...", clean_sym, interval, days),
        percent: 0.0,
        count: 0,
        pause_left: None,
    });

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build();

    while curr_start < end_ms {
        let target_end = end_ms.min(next_milestone_ms);

        let req = agent
            .get(url)
            .query("symbol", &clean_sym)
            .query("interval", interval)
            .query("startTime", &curr_start.to_string())
            .query("endTime", &target_end.to_string())
            .query("limit", "1500");

        let resp = req.call().map_err(|e| format!("HTTP request error: {}", e))?;
        let raw_klines: Vec<Vec<serde_json::Value>> = resp
            .into_json()
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if raw_klines.is_empty() {
            if curr_start < target_end {
                curr_start = target_end + step_ms;
            } else {
                break;
            }
        }

        let mut last_time = curr_start;

        for k in &raw_klines {
            if k.len() >= 6 {
                let t = k[0].as_i64().unwrap_or(0);
                let o = k[1].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                let h = k[2].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                let l = k[3].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                let c = k[4].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                let v = k[5].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);

                all_times.push(t);
                all_open.push(o);
                all_high.push(h);
                all_low.push(l);
                all_close.push(c);
                all_volume.push(v);
                last_time = t;
            }
        }

        req_count += 1;
        curr_start = last_time + step_ms;

        let elapsed_ms = last_time - start_ms;
        let pct = (elapsed_ms as f64 / total_ms as f64 * 100.0).clamp(1.0, 99.0);
        let dt_str = DateTime::from_timestamp_millis(last_time)
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();

        progress_cb(FetchProgress {
            status: "fetching".into(),
            message: format!("Request #{}: {} candles diambil (sampai {})", req_count, all_times.len(), dt_str),
            percent: (pct * 10.0).round() / 10.0,
            count: all_times.len(),
            pause_left: None,
        });

        let reached_milestone = (curr_start >= next_milestone_ms) || (raw_klines.len() < 1500 && curr_start >= target_end);
        if reached_milestone && curr_start < end_ms {
            let pause_seconds = 60;
            for sec_left in (1..=pause_seconds).rev() {
                progress_cb(FetchProgress {
                    status: "milestone_pause".into(),
                    message: format!(
                        "Tercapai batas {} hari. Delay 1 menit sesuai aturan (tersisa {} detik)...",
                        next_milestone_day, sec_left
                    ),
                    percent: (pct * 10.0).round() / 10.0,
                    count: all_times.len(),
                    pause_left: Some(sec_left),
                });
                thread::sleep(Duration::from_secs(1));
            }
            next_milestone_day += 360;
            next_milestone_ms += ms_360_days;
        } else {
            // Delay 0.2 detik per request
            thread::sleep(Duration::from_millis(200));
        }

        if curr_start >= end_ms {
            break;
        }
    }

    if all_times.is_empty() {
        return Err("Tidak ada data kline yang ditemukan untuk parameter ini".into());
    }

    let result = CandleData {
        times: all_times,
        open: all_open,
        high: all_high,
        low: all_low,
        close: all_close,
        volume: all_volume,
        symbol: clean_sym,
        interval: interval.to_string(),
        days,
    };

    save_cache_data(&result);

    progress_cb(FetchProgress {
        status: "completed".into(),
        message: format!("Selesai! Total {} candles berhasil diambil.", result.times.len()),
        percent: 100.0,
        count: result.times.len(),
        pause_left: None,
    });

    Ok(result)
}

/// Fetch klines berdasarkan target jumlah candle (misal 10.000 candle)
pub fn fetch_binance_fapi_klines_by_count<F>(
    symbol: &str,
    interval: &str,
    target_count: usize,
    use_cache: bool,
    progress_cb: F,
) -> Result<CandleData, String>
where
    F: FnMut(FetchProgress),
{
    let step_ms = get_interval_ms(interval);
    let total_needed_ms = (target_count as i64) * step_ms;
    let days = ((total_needed_ms as f64) / 86_400_000.0).ceil().max(1.0) as u32;
    fetch_binance_fapi_klines(symbol, interval, days, use_cache, progress_cb)
}
