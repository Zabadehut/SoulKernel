//! hud.rs — System HUD overlay management
//!
//! Types, helpers and Tauri commands for the always-on-top HUD window.

use std::sync::{Arc, Mutex};
use tauri::PhysicalPosition;
use tauri::{Emitter, Manager, State};
use tokio::sync::mpsc;

use crate::audit::{audit_write, now_ms_local, SharedAudit};

// ─── Types ────────────────────────────────────────────────────────────────────

pub type SharedHud = Arc<Mutex<HudRuntimeState>>;
pub type SharedHudTx = Arc<Mutex<Option<mpsc::UnboundedSender<HudOverlayData>>>>;
pub type SharedHudData = Arc<Mutex<Option<HudOverlayData>>>;
pub type SharedHudHealth = Arc<Mutex<HudHealthState>>;

pub struct HudRuntimeState {
    pub visible: bool,
    pub interactive: bool,
    pub preset: String,
    pub opacity: f64,
    pub display_index: Option<usize>,
}

pub struct HudHealthState {
    pub last_ready_ms: u64,
    pub last_reload_ms: u64,
    pub reload_count: u32,
    pub ready_count: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HudOverlayData {
    pub dome: String,
    pub sigma: String,
    pub pi: String,
    pub cpu: String,
    pub ram: String,
    pub target: String,
    pub power: String,
    pub energy: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HudConfigEvent {
    pub preset: String,
    pub interactive: bool,
    pub opacity: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DisplayInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

pub fn preset_to_size(preset: &str) -> (f64, f64) {
    match preset {
        "mini" => (280.0, 148.0),
        "detailed" => (460.0, 280.0),
        _ => (360.0, 210.0),
    }
}

pub fn list_displays_internal(app: &tauri::AppHandle) -> Result<Vec<DisplayInfo>, String> {
    let primary_name = app
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .and_then(|m| m.name().cloned());
    let mons = app.available_monitors().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(mons.len());
    for (idx, m) in mons.iter().enumerate() {
        let name = m
            .name()
            .cloned()
            .unwrap_or_else(|| format!("Display {}", idx + 1));
        out.push(DisplayInfo {
            index: idx,
            name: name.clone(),
            width: m.size().width,
            height: m.size().height,
            x: m.position().x,
            y: m.position().y,
            scale_factor: m.scale_factor(),
            is_primary: primary_name
                .as_ref()
                .map(|n| n.as_str() == name.as_str())
                .unwrap_or(false),
        });
    }
    Ok(out)
}

fn pick_display(
    app: &tauri::AppHandle,
    index: Option<usize>,
) -> Result<Option<DisplayInfo>, String> {
    let list = list_displays_internal(app)?;
    if list.is_empty() {
        return Ok(None);
    }
    if let Some(i) = index {
        if let Some(d) = list.iter().find(|d| d.index == i) {
            return Ok(Some(d.clone()));
        }
    }
    if let Some(d) = list.iter().find(|d| d.is_primary) {
        return Ok(Some(d.clone()));
    }
    Ok(list.into_iter().next())
}

fn ensure_hud_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("hud") {
        return Ok(w);
    }
    let url = tauri::WebviewUrl::App("hud.html".into());
    tauri::WebviewWindowBuilder::new(app, "hud", url)
        .title("SoulKernel HUD")
        .initialization_script(
            r#"
            (() => {
              try {
                const s = document.createElement('style');
                s.textContent = 'html,body{background:#0b1320 !important;color:#9dbad6;font:12px monospace;}';
                document.documentElement.appendChild(s);
                const p = document.createElement('div');
                p.id = '__sk_hud_boot';
                p.textContent = 'SoulKernel HUD booting...';
                p.style.cssText = 'position:fixed;left:8px;top:8px;z-index:2147483647;opacity:.8';
                document.documentElement.appendChild(p);
                setTimeout(() => { try { p.remove(); } catch (_) {} }, 3000);
              } catch (_) {}
            })();
            "#,
        )
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .transparent(false)
        .inner_size(360.0, 210.0)
        .position(14.0, 58.0)
        .build()
        .map_err(|e| e.to_string())
}

pub fn apply_hud_window_mode(
    app: &tauri::AppHandle,
    interactive: bool,
    preset: &str,
    opacity: f64,
    display_index: Option<usize>,
) -> Result<(), String> {
    let w = ensure_hud_window(app)?;
    let (width, height) = preset_to_size(preset);
    let clamped_opacity = opacity.clamp(0.3, 1.0);
    if let Some(display) = pick_display(app, display_index)? {
        let x = display.x + 14;
        let y = display.y + 58;
        let _ = w.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
    }
    let _ = w.set_always_on_top(true);
    let _ = w.set_ignore_cursor_events(!interactive);
    let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    let _ = w.emit(
        "soulkernel://hud-config",
        HudConfigEvent {
            preset: preset.to_string(),
            interactive,
            opacity: clamped_opacity,
        },
    );
    Ok(())
}

pub fn cleanup_hud_before_exit(app: &tauri::AppHandle) {
    if let Some(audit) = app.try_state::<SharedAudit>() {
        let _ = audit_write(&*audit, "hud", "exit-cleanup", Some("info"), None);
    }
    if let Some(hud_state) = app.try_state::<SharedHud>() {
        if let Ok(mut hs) = hud_state.lock() {
            hs.visible = false;
        }
    }
    if let Some(w) = app.get_webview_window("hud") {
        let _ = w.hide();
        let _ = w.close();
    }
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn open_system_hud(
    app: tauri::AppHandle,
    hud: State<'_, SharedHud>,
    hud_health: State<'_, SharedHudHealth>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.visible = true;
    }
    {
        let mut health = hud_health.lock().map_err(|e| e.to_string())?;
        let now = now_ms_local();
        health.last_ready_ms = now;
        health.last_reload_ms = 0;
        health.reload_count = 0;
    }
    let hs = hud.lock().map_err(|e| e.to_string())?;
    apply_hud_window_mode(
        &app,
        hs.interactive,
        &hs.preset,
        hs.opacity,
        hs.display_index,
    )?;
    if let Some(w) = app.get_webview_window("hud") {
        let _ = w.show();
    }
    let _ = audit_write(
        &*audit,
        "hud",
        "open",
        Some("info"),
        Some(serde_json::json!({
            "preset": hs.preset,
            "interactive": hs.interactive,
            "opacity": hs.opacity,
            "display_index": hs.display_index
        })),
    );
    Ok(())
}

#[tauri::command]
pub fn close_system_hud(
    app: tauri::AppHandle,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.visible = false;
    }
    if let Some(w) = app.get_webview_window("hud") {
        w.hide().map_err(|e| e.to_string())?;
    }
    let _ = audit_write(&*audit, "hud", "close", Some("info"), None);
    Ok(())
}

#[tauri::command]
pub fn set_system_hud_interactive(
    app: tauri::AppHandle,
    interactive: bool,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.interactive = interactive;
    }
    let hs = hud.lock().map_err(|e| e.to_string())?;
    apply_hud_window_mode(
        &app,
        hs.interactive,
        &hs.preset,
        hs.opacity,
        hs.display_index,
    )?;
    let _ = audit_write(
        &*audit,
        "hud",
        "interactive",
        Some("info"),
        Some(serde_json::json!({ "interactive": hs.interactive })),
    );
    Ok(())
}

#[tauri::command]
pub fn set_system_hud_preset(
    app: tauri::AppHandle,
    preset: String,
    opacity: f64,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.preset = match preset.as_str() {
            "mini" | "compact" | "detailed" => preset,
            _ => "compact".to_string(),
        };
        hs.opacity = opacity.clamp(0.3, 1.0);
    }
    let hs = hud.lock().map_err(|e| e.to_string())?;
    apply_hud_window_mode(
        &app,
        hs.interactive,
        &hs.preset,
        hs.opacity,
        hs.display_index,
    )?;
    let _ = audit_write(
        &*audit,
        "hud",
        "preset",
        Some("info"),
        Some(serde_json::json!({
            "preset": hs.preset,
            "opacity": hs.opacity
        })),
    );
    Ok(())
}

#[tauri::command]
pub fn set_system_hud_data(
    app: tauri::AppHandle,
    payload: HudOverlayData,
    hud_tx: State<'_, SharedHudTx>,
    hud_data: State<'_, SharedHudData>,
) -> Result<(), String> {
    {
        let mut latest = hud_data.lock().map_err(|e| e.to_string())?;
        *latest = Some(payload.clone());
    }
    if let Some(tx) = hud_tx.lock().map_err(|e| e.to_string())?.as_ref() {
        let _ = tx.send(payload);
        return Ok(());
    }
    if app.get_webview_window("hud").is_some() {
        app.emit_to("hud", "soulkernel://hud", payload)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_system_hud_data(
    hud_data: State<'_, SharedHudData>,
) -> Result<Option<HudOverlayData>, String> {
    let data = hud_data.lock().map_err(|e| e.to_string())?;
    Ok(data.clone())
}

#[tauri::command]
pub fn get_system_hud_config(hud: State<'_, SharedHud>) -> Result<HudConfigEvent, String> {
    let hs = hud.lock().map_err(|e| e.to_string())?;
    Ok(HudConfigEvent {
        preset: hs.preset.clone(),
        interactive: hs.interactive,
        opacity: hs.opacity,
    })
}

#[tauri::command]
pub fn set_system_hud_ready(
    app: tauri::AppHandle,
    ts_ms: Option<u64>,
    hud_health: State<'_, SharedHudHealth>,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    let now = ts_ms.unwrap_or_else(now_ms_local);
    let mut health = hud_health.lock().map_err(|e| e.to_string())?;
    health.last_ready_ms = now;
    health.ready_count = health.ready_count.saturating_add(1);

    let hs = hud.lock().map_err(|e| e.to_string())?;
    if hs.visible {
        if let Some(w) = app.get_webview_window("hud") {
            let was_visible = w.is_visible().unwrap_or(false);
            let _ = w.show();
            if !was_visible {
                let _ = audit_write(
                    &*audit,
                    "hud",
                    "shown-after-ready",
                    Some("info"),
                    Some(serde_json::json!({
                        "ready_count": health.ready_count
                    })),
                );
            }
        }
    }

    if health.reload_count > 0 {
        let _ = audit_write(
            &*audit,
            "hud",
            "recovered",
            Some("info"),
            Some(serde_json::json!({ "reload_count": health.reload_count })),
        );
        health.reload_count = 0;
    }
    Ok(())
}

#[tauri::command]
pub fn list_displays(app: tauri::AppHandle) -> Result<Vec<DisplayInfo>, String> {
    list_displays_internal(&app)
}

#[tauri::command]
pub fn set_system_hud_display(
    app: tauri::AppHandle,
    display_index: Option<usize>,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.display_index = display_index;
    }
    let hs = hud.lock().map_err(|e| e.to_string())?;
    apply_hud_window_mode(
        &app,
        hs.interactive,
        &hs.preset,
        hs.opacity,
        hs.display_index,
    )?;
    let _ = audit_write(
        &*audit,
        "hud",
        "display",
        Some("info"),
        Some(serde_json::json!({ "display_index": hs.display_index })),
    );
    Ok(())
}
