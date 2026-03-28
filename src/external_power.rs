//! Lecture optionnelle d'une puissance « murale » (ex. prise Meross via pont JSON).
//! Ne remplace pas le pilotage OS : elle fournit une mesure de référence secteur quand disponible.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_MAX_AGE_MS: u64 = 15_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerossFileConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub power_file: Option<String>,
    #[serde(default)]
    pub max_age_ms: Option<u64>,
    #[serde(default)]
    pub meross_email: Option<String>,
    #[serde(default)]
    pub meross_password: Option<String>,
    #[serde(default)]
    pub meross_region: Option<String>,
    #[serde(default)]
    pub meross_device_type: Option<String>,
    #[serde(default)]
    pub python_bin: Option<String>,
    #[serde(default)]
    pub bridge_interval_s: Option<f64>,
    #[serde(default)]
    pub autostart_bridge: bool,
}

impl Default for MerossFileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            power_file: None,
            max_age_ms: None,
            meross_email: None,
            meross_password: None,
            meross_region: None,
            meross_device_type: None,
            python_bin: None,
            bridge_interval_s: None,
            autostart_bridge: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalPowerStatus {
    pub config_path: String,
    pub power_file_path: String,
    pub enabled: bool,
    pub max_age_ms: u64,
    pub config_exists: bool,
    pub power_file_exists: bool,
    pub last_watts: Option<f64>,
    pub last_ts_ms: Option<u64>,
    pub is_fresh: bool,
    pub source_tag: String,
    pub autostart_bridge: bool,
    pub bridge_interval_s: f64,
    pub meross_region: String,
    pub meross_device_type: String,
    pub python_bin: String,
    pub default_python_hint: String,
    pub credentials_present: bool,
    pub bridge_log_path: String,
}

#[derive(Debug, Deserialize)]
struct PowerSnapshotFile {
    #[serde(default)]
    watts: Option<f64>,
    #[serde(default)]
    w: Option<f64>,
    #[serde(default)]
    power: Option<f64>,
    #[serde(default)]
    ts_ms: Option<u64>,
}

/// Répertoire de configuration SoulKernel (hors dépôt).
pub fn soulkernel_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return Some(PathBuf::from(appdata).join("SoulKernel"));
        }
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("SoulKernel"),
            );
        }
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(PathBuf::from(home).join(".config").join("soulkernel"));
        }
        return None;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

pub fn config_path() -> Option<PathBuf> {
    soulkernel_config_dir().map(|d| d.join("meross.json"))
}

pub fn default_power_file() -> Option<PathBuf> {
    soulkernel_config_dir().map(|d| d.join("meross_power.json"))
}

pub fn load_meross_config() -> Option<MerossFileConfig> {
    let p = config_path()?;
    let raw = std::fs::read_to_string(&p).ok()?;
    serde_json::from_str(&raw).ok()
}

fn read_snapshot(path: &std::path::Path) -> Option<PowerSnapshotFile> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn get_meross_config_or_default() -> MerossFileConfig {
    load_meross_config().unwrap_or_default()
}

pub fn save_meross_config(cfg: &MerossFileConfig) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Err("config path unavailable".to_string());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(cfg).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub fn get_external_power_status() -> ExternalPowerStatus {
    let cfg_path = config_path();
    let cfg = get_meross_config_or_default();
    let max_age_ms = cfg.max_age_ms.unwrap_or(DEFAULT_MAX_AGE_MS);
    let power_path = cfg
        .power_file
        .as_ref()
        .map(PathBuf::from)
        .or_else(default_power_file);
    let snapshot = power_path.as_ref().and_then(|p| read_snapshot(p));
    let last_watts = snapshot
        .as_ref()
        .and_then(|snap| snap.watts.or(snap.w).or(snap.power))
        .filter(|v| v.is_finite() && *v >= 0.0 && *v < 5000.0);
    let last_ts_ms = snapshot.as_ref().and_then(|snap| snap.ts_ms);
    let is_fresh = last_ts_ms
        .map(|ts| now_ms().saturating_sub(ts) <= max_age_ms)
        .unwrap_or(snapshot.is_some());

    let bridge_interval_s = cfg.bridge_interval_s.unwrap_or(8.0).clamp(2.0, 300.0);
    let meross_region = cfg
        .meross_region
        .as_deref()
        .unwrap_or("eu")
        .trim()
        .to_string();
    let meross_device_type = cfg
        .meross_device_type
        .as_deref()
        .unwrap_or("mss315")
        .trim()
        .to_string();
    let python_bin = cfg.python_bin.as_deref().unwrap_or("").trim().to_string();
    let default_python_hint = if cfg!(target_os = "windows") {
        "py".to_string()
    } else {
        "python3".to_string()
    };
    let credentials_present = cfg
        .meross_email
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
        && cfg
            .meross_password
            .as_deref()
            .map(|v| !v.is_empty())
            .unwrap_or(false);

    ExternalPowerStatus {
        config_path: cfg_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        power_file_path: power_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        enabled: cfg.enabled,
        max_age_ms,
        config_exists: cfg_path.as_ref().map(|p| p.exists()).unwrap_or(false),
        power_file_exists: power_path.as_ref().map(|p| p.exists()).unwrap_or(false),
        last_watts,
        last_ts_ms,
        is_fresh,
        source_tag: "meross_wall".to_string(),
        autostart_bridge: cfg.autostart_bridge,
        bridge_interval_s,
        meross_region,
        meross_device_type,
        python_bin,
        default_python_hint,
        credentials_present,
        bridge_log_path: soulkernel_config_dir()
            .map(|d| d.join("meross_bridge.log").to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

/// Si la config Meross est activée et le fichier récent, retourne les W à afficher / ingérer
/// à la place du RAPL. Sinon `None` (l’appelant garde la mesure plateforme).
pub fn merge_wall_power() -> Option<(f64, String)> {
    let cfg = load_meross_config()?;
    if !cfg.enabled {
        return None;
    }

    let path: PathBuf = cfg
        .power_file
        .as_ref()
        .map(PathBuf::from)
        .or_else(default_power_file)?;
    let snap = read_snapshot(&path)?;
    let w = snap
        .watts
        .or(snap.w)
        .or(snap.power)
        .filter(|v| v.is_finite() && *v >= 0.0 && *v < 5000.0)?;

    let max_age = cfg.max_age_ms.unwrap_or(DEFAULT_MAX_AGE_MS);
    if let Some(ts) = snap.ts_ms {
        let age = now_ms().saturating_sub(ts);
        if age > max_age {
            return None;
        }
    }

    Some((w, "meross_wall".to_string()))
}
