//! audit.rs — JSONL audit log writer
//!
//! Writes structured audit entries to a persistent JSONL file.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

pub type SharedAudit = Arc<Mutex<AuditState>>;

#[derive(Default)]
pub struct AuditState {
    pub path: Option<std::path::PathBuf>,
}

#[derive(serde::Serialize)]
struct AuditEntry {
    ts_ms: u64,
    category: String,
    action: String,
    level: Option<String>,
    data: Option<serde_json::Value>,
}

static AUDIT_FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

pub fn now_ms_local() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

pub fn default_audit_path() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return std::path::PathBuf::from(appdata)
            .join("SoulKernel")
            .join("audit")
            .join("soulkernel_audit.jsonl");
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return std::path::PathBuf::from(xdg)
                .join("SoulKernel")
                .join("audit")
                .join("soulkernel_audit.jsonl");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("SoulKernel")
                .join("audit")
                .join("soulkernel_audit.jsonl");
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("soulkernel_audit.jsonl")
}

fn ensure_audit_file(audit: &SharedAudit) -> Result<&'static Mutex<std::fs::File>, String> {
    if AUDIT_FILE.get().is_none() {
        let mut guard = audit.lock().map_err(|e| e.to_string())?;
        if guard.path.is_none() {
            guard.path = Some(default_audit_path());
        }
        let path = guard
            .path
            .as_ref()
            .cloned()
            .ok_or_else(|| "audit path unavailable".to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let _ = AUDIT_FILE.set(Mutex::new(file));
    }
    AUDIT_FILE
        .get()
        .ok_or_else(|| "audit logger init failed".to_string())
}

pub fn audit_write(
    audit: &SharedAudit,
    category: &str,
    action: &str,
    level: Option<&str>,
    data: Option<serde_json::Value>,
) -> Result<(), String> {
    let file_mutex = ensure_audit_file(audit)?;
    let ts_ms = now_ms_local();
    let entry = AuditEntry {
        ts_ms,
        category: category.to_string(),
        action: action.to_string(),
        level: level.map(|s| s.to_string()),
        data,
    };
    let line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    let mut file = file_mutex.lock().map_err(|e| e.to_string())?;
    use std::io::Write;
    writeln!(file, "{}", line).map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_audit_log_path(audit: State<'_, SharedAudit>) -> Result<String, String> {
    {
        let mut g = audit.lock().map_err(|e| e.to_string())?;
        if g.path.is_none() {
            g.path = Some(default_audit_path());
        }
    }
    let _ = ensure_audit_file(&*audit)?;
    let g = audit.lock().map_err(|e| e.to_string())?;
    g.path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| "audit path unavailable".to_string())
}

#[tauri::command]
pub fn audit_log_event(
    category: String,
    action: String,
    level: Option<String>,
    data: Option<serde_json::Value>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    audit_write(&*audit, &category, &action, level.as_deref(), data)
}
