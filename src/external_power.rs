//! Lecture optionnelle d'une puissance « murale » (ex. prise Meross via pont JSON).
//! Ne remplace pas le pilotage OS : elle fournit une mesure de référence secteur quand disponible.

use serde::Deserialize;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_MAX_AGE_MS: u64 = 15_000;

#[derive(Debug, Clone, Deserialize)]
pub struct MerossFileConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub power_file: Option<String>,
    #[serde(default)]
    pub max_age_ms: Option<u64>,
}

impl Default for MerossFileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            power_file: None,
            max_age_ms: None,
        }
    }
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
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    )))]
    {
        None
    }
}

fn config_path() -> Option<PathBuf> {
    soulkernel_config_dir().map(|d| d.join("meross.json"))
}

fn default_power_file() -> Option<PathBuf> {
    soulkernel_config_dir().map(|d| d.join("meross_power.json"))
}

fn load_meross_config() -> Option<MerossFileConfig> {
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
