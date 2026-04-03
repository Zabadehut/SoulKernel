pub fn pct(value: f64) -> String {
    format!("{value:.1} %")
}

pub fn opt_pct(value: Option<f64>) -> String {
    value.map(pct).unwrap_or_else(|| "N/A".to_string())
}

pub fn watts(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.1} W"))
        .unwrap_or_else(|| "N/A".to_string())
}

pub fn maybe_text(value: Option<&str>, fallback: &str) -> String {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| fallback.to_string())
}

pub fn mib_from_kb(kb: u64) -> String {
    format!("{:.0} MiB", kb as f64 / 1024.0)
}

pub fn mib_from_mb(mb: u64) -> String {
    if mb >= 1024 {
        format!("{:.1} GiB", mb as f64 / 1024.0)
    } else {
        format!("{} MiB", mb)
    }
}

pub fn gib_pair(used_mb: u64, total_mb: u64) -> String {
    format!(
        "{:.1} / {:.1} GiB",
        used_mb as f64 / 1024.0,
        total_mb as f64 / 1024.0
    )
}

pub fn runtime_short(run_time_s: u64) -> String {
    let h = run_time_s / 3600;
    let m = (run_time_s % 3600) / 60;
    let s = run_time_s % 60;
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

pub fn io_pair(read_b: u64, write_b: u64) -> String {
    fn mib(v: u64) -> f64 {
        v as f64 / (1024.0 * 1024.0)
    }
    format!("R {:.2} / W {:.2} MiB", mib(read_b), mib(write_b))
}
