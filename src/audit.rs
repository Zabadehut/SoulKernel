use soulkernel_core::audit as core_audit;
use tauri::State;

pub type SharedAudit = core_audit::SharedAudit;
pub type AuditState = core_audit::AuditState;

pub use core_audit::{audit_write, default_audit_path, now_ms_local};

#[tauri::command]
pub fn get_audit_log_path(audit: State<'_, SharedAudit>) -> Result<String, String> {
    {
        let mut g = audit.lock().map_err(|e| e.to_string())?;
        if g.path.is_none() {
            g.path = Some(default_audit_path());
        }
    }
    let _ = audit_write(&*audit, "audit", "ensure_file", None, None);
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
