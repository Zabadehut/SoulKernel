//! hud.rs — System HUD overlay management
//!
//! Types, helpers and Tauri commands for the always-on-top HUD window.

use std::sync::{Arc, Mutex};
use tauri::PhysicalPosition;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

use crate::audit::{audit_write, now_ms_local, SharedAudit};

/// Sur Linux (GTK), création de fenêtre WebView, moniteurs et émissions vers webview
/// doivent s'exécuter sur le fil principal. Les commandes `invoke` Tauri sont sinon
/// exécutées sur un pool de threads — ce qui peut provoquer un panic/abort dans wry.
pub(crate) fn dispatch_on_main_thread<R: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce() -> R + Send + 'static,
) -> Result<R, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    handle
        .run_on_main_thread(move || {
            let out = f();
            let _ = tx.send(out);
        })
        .map_err(|e| e.to_string())?;
    rx.recv()
        .map_err(|_| "HUD: fil principal interrompu avant la fin de l'opération".to_string())
}

// ─── Types ────────────────────────────────────────────────────────────────────

pub type SharedHud = Arc<Mutex<HudRuntimeState>>;
pub type SharedHudTx = Arc<Mutex<Option<mpsc::UnboundedSender<HudOverlayData>>>>;
pub type SharedHudData = Arc<Mutex<Option<HudOverlayData>>>;
pub type SharedHudHealth = Arc<Mutex<HudHealthState>>;

#[derive(Clone)]
pub struct HudRuntimeState {
    pub visible: bool,
    pub interactive: bool,
    pub preset: String,
    pub opacity: f64,
    pub display_index: Option<usize>,
    /// `screen` = % de la résolution native de l’écran actif ; `content` = ajustement au contenu (WebView) ; `manual` = px logiques.
    pub size_mode: String,
    pub screen_width_pct: f64,
    pub screen_height_pct: f64,
    pub manual_width: f64,
    pub manual_height: f64,
    /// Clés autorisées : dome, sigma, pi, cpu, ram, target, power, energy
    pub visible_metrics: Vec<String>,
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
    pub size_mode: String,
    pub screen_width_pct: f64,
    pub screen_height_pct: f64,
    pub manual_width: f64,
    pub manual_height: f64,
    pub visible_metrics: Vec<String>,
    pub active_screen_width: Option<u32>,
    pub active_screen_height: Option<u32>,
    pub active_screen_scale: Option<f64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct HudPresentationPayload {
    pub preset: String,
    pub opacity: f64,
    pub size_mode: String,
    pub screen_width_pct: f64,
    pub screen_height_pct: f64,
    pub manual_width: f64,
    pub manual_height: f64,
    pub visible_metrics: Vec<String>,
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
        // Taille initiale pour le mode « contenu » ; `hud.html` ajuste ensuite (set_hud_window_size).
        "mini" => (320.0, 240.0),
        "detailed" => (520.0, 360.0),
        _ => (420.0, 260.0),
    }
}

pub const HUD_METRIC_KEYS: [&str; 8] = [
    "dome", "sigma", "pi", "cpu", "ram", "target", "power", "energy",
];

pub fn default_visible_metrics() -> Vec<String> {
    HUD_METRIC_KEYS.iter().map(|s| (*s).to_string()).collect()
}

fn normalize_size_mode(s: &str) -> String {
    match s {
        "screen" | "content" | "manual" => s.to_string(),
        _ => "screen".to_string(),
    }
}

pub fn normalize_visible_metrics(v: Vec<String>) -> Vec<String> {
    let allowed: std::collections::HashSet<&str> = HUD_METRIC_KEYS.iter().copied().collect();
    let mut out: Vec<String> = v
        .into_iter()
        .filter(|s| allowed.contains(s.as_str()))
        .collect();
    if out.is_empty() {
        return default_visible_metrics();
    }
    let order: std::collections::HashMap<&str, usize> = HUD_METRIC_KEYS
        .iter()
        .enumerate()
        .map(|(i, k)| (*k, i))
        .collect();
    out.sort_by_key(|k| order.get(k.as_str()).copied().unwrap_or(999));
    out
}

/// Taille logique (points) de la fenêtre HUD selon le mode et l’écran actif.
pub fn compute_hud_logical_size(
    app: &AppHandle,
    hs: &HudRuntimeState,
) -> Result<(f64, f64), String> {
    let mode = normalize_size_mode(&hs.size_mode);
    match mode.as_str() {
        "manual" => {
            let w = hs.manual_width.clamp(240.0, 1600.0);
            let h = hs.manual_height.clamp(120.0, 1200.0);
            Ok((w, h))
        }
        "content" => Ok(preset_to_size(&hs.preset)),
        "screen" | _ => {
            let Some(display) = pick_display(app, hs.display_index)? else {
                return Ok(preset_to_size(&hs.preset));
            };
            let sf = display.scale_factor.max(0.5);
            let pw = display.width as f64 * (hs.screen_width_pct / 100.0).clamp(0.05, 0.95);
            let ph = display.height as f64 * (hs.screen_height_pct / 100.0).clamp(0.05, 0.95);
            let w = (pw / sf).clamp(200.0, 1600.0);
            let h = (ph / sf).clamp(120.0, 1200.0);
            Ok((w, h))
        }
    }
}

/// Réinitialise le compteur « stale » du watchdog quand le HUD est (re)montré — évite les recréations
/// en boucle si le raccourci clavier ouvre la fenêtre sans passer par `open_system_hud`.
pub fn reset_hud_health_for_show(hud_health: &SharedHudHealth) {
    if let Ok(mut h) = hud_health.lock() {
        let now = now_ms_local();
        h.last_ready_ms = now;
        h.last_reload_ms = 0;
        h.reload_count = 0;
    }
}

pub fn list_displays_internal(app: &tauri::AppHandle) -> Result<Vec<DisplayInfo>, String> {
    // Ne pas faire échouer tout le HUD si `primary_monitor` est indisponible (Wayland, VM, drivers).
    let primary_name = app
        .primary_monitor()
        .ok()
        .flatten()
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
    let builder = tauri::WebviewWindowBuilder::new(app, "hud", url)
        .title("SoulKernel HUD")
        .initialization_script(
            r#"
            (() => {
              try {
                const s = document.createElement('style');
                s.textContent = 'html,body{background:#0b0f14!important;color:#d6e2f0!important;font:12px ui-monospace,monospace;}';
                document.documentElement.appendChild(s);
                const p = document.createElement('div');
                p.id = '__sk_hud_boot';
                p.textContent = 'SoulKernel · HUD…';
                p.style.cssText = 'position:fixed;left:12px;top:12px;z-index:2147483647;opacity:.85;font-size:11px;letter-spacing:.08em;color:#00d4ff';
                document.documentElement.appendChild(p);
                setTimeout(() => { try { p.remove(); } catch (_) {} }, 2500);
              } catch (_) {}
            })();
            "#,
        )
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false);
    // `WebviewWindowBuilder::transparent` n’existe pas sur macOS (Tauri 2).
    #[cfg(not(target_os = "macos"))]
    let builder = builder.transparent(false);
    builder
        .inner_size(420.0, 260.0)
        .position(14.0, 58.0)
        .build()
        .map_err(|e| e.to_string())
}

/// Tao/Linux (`tao` `WindowRequest::CursorIgnoreEvents`) appelle `gdk::Window::input_shape_combine_region`
/// sur `window.window().unwrap()` : si la fenêtre n’a jamais été affichée, il n’y a pas encore de
/// GdkWindow → panic à `event_loop.rs:448`. On force une courte réalisation GTK par show/hide.
fn sync_hud_cursor_pass_through(w: &tauri::WebviewWindow, interactive: bool) {
    let pass_through = !interactive;
    if !pass_through {
        let _ = w.set_ignore_cursor_events(false);
        return;
    }
    #[cfg(target_os = "linux")]
    {
        let was_visible = w.is_visible().unwrap_or(false);
        if !was_visible {
            let _ = w.show();
        }
        let _ = w.set_ignore_cursor_events(true);
        if !was_visible {
            let _ = w.hide();
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = w.set_ignore_cursor_events(true);
    }
}

pub fn apply_hud_window_mode(app: &AppHandle, hs: &HudRuntimeState) -> Result<(), String> {
    let w = ensure_hud_window(app)?;
    let (width, height) = compute_hud_logical_size(app, hs)?;
    let clamped_opacity = hs.opacity.clamp(0.3, 1.0);
    if let Some(display) = pick_display(app, hs.display_index)? {
        let x = display.x + 14;
        let y = display.y + 58;
        let _ = w.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
    }
    let _ = w.set_always_on_top(true);
    sync_hud_cursor_pass_through(&w, hs.interactive);
    let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    let active = pick_display(app, hs.display_index).ok().flatten();
    let vm = normalize_visible_metrics(hs.visible_metrics.clone());
    let _ = w.emit(
        "soulkernel://hud-config",
        HudConfigEvent {
            preset: hs.preset.clone(),
            interactive: hs.interactive,
            opacity: clamped_opacity,
            size_mode: normalize_size_mode(&hs.size_mode),
            screen_width_pct: hs.screen_width_pct.clamp(5.0, 95.0),
            screen_height_pct: hs.screen_height_pct.clamp(5.0, 95.0),
            manual_width: hs.manual_width,
            manual_height: hs.manual_height,
            visible_metrics: vm.clone(),
            active_screen_width: active.as_ref().map(|d| d.width),
            active_screen_height: active.as_ref().map(|d| d.height),
            active_screen_scale: active.as_ref().map(|d| d.scale_factor),
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
    reset_hud_health_for_show(&*hud_health);
    let snapshot = hud.lock().map_err(|e| e.to_string())?.clone();
    let app2 = app.clone();
    dispatch_on_main_thread(&app, move || {
        apply_hud_window_mode(&app2, &snapshot)?;
        if let Some(w) = app2.get_webview_window("hud") {
            let _ = w.show();
        }
        Ok::<(), String>(())
    })??;
    let hs = hud.lock().map_err(|e| e.to_string())?;
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
    let app2 = app.clone();
    dispatch_on_main_thread(&app, move || {
        if let Some(w) = app2.get_webview_window("hud") {
            w.hide().map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    })??;
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
    let snapshot = hud.lock().map_err(|e| e.to_string())?.clone();
    let interactive_audit = snapshot.interactive;
    let app2 = app.clone();
    dispatch_on_main_thread(&app, move || {
        apply_hud_window_mode(&app2, &snapshot)?;
        Ok::<(), String>(())
    })??;
    let _ = audit_write(
        &*audit,
        "hud",
        "interactive",
        Some("info"),
        Some(serde_json::json!({ "interactive": interactive_audit })),
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
    let snapshot = hud.lock().map_err(|e| e.to_string())?.clone();
    let preset_audit = snapshot.preset.clone();
    let opacity_audit = snapshot.opacity;
    let app2 = app.clone();
    dispatch_on_main_thread(&app, move || {
        apply_hud_window_mode(&app2, &snapshot)?;
        Ok::<(), String>(())
    })??;
    let _ = audit_write(
        &*audit,
        "hud",
        "preset",
        Some("info"),
        Some(serde_json::json!({
            "preset": preset_audit,
            "opacity": opacity_audit
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
    let app2 = app.clone();
    dispatch_on_main_thread(&app, move || {
        if app2.get_webview_window("hud").is_some() {
            app2
                .emit_to("hud", "soulkernel://hud", payload)
                .map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    })??;
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
pub fn get_system_hud_config(
    app: AppHandle,
    hud: State<'_, SharedHud>,
) -> Result<HudConfigEvent, String> {
    let hs = hud.lock().map_err(|e| e.to_string())?;
    let active = pick_display(&app, hs.display_index).ok().flatten();
    Ok(HudConfigEvent {
        preset: hs.preset.clone(),
        interactive: hs.interactive,
        opacity: hs.opacity,
        size_mode: normalize_size_mode(&hs.size_mode),
        screen_width_pct: hs.screen_width_pct.clamp(5.0, 95.0),
        screen_height_pct: hs.screen_height_pct.clamp(5.0, 95.0),
        manual_width: hs.manual_width,
        manual_height: hs.manual_height,
        visible_metrics: normalize_visible_metrics(hs.visible_metrics.clone()),
        active_screen_width: active.as_ref().map(|d| d.width),
        active_screen_height: active.as_ref().map(|d| d.height),
        active_screen_scale: active.as_ref().map(|d| d.scale_factor),
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
    let ready_count = {
        let mut health = hud_health.lock().map_err(|e| e.to_string())?;
        health.last_ready_ms = now;
        health.ready_count = health.ready_count.saturating_add(1);
        health.ready_count
    };

    let visible = hud.lock().map_err(|e| e.to_string())?.visible;
    if visible {
        let needs_shown_audit = {
            let app2 = app.clone();
            dispatch_on_main_thread(&app, move || {
                if let Some(w) = app2.get_webview_window("hud") {
                    let was_visible = w.is_visible().unwrap_or(false);
                    let _ = w.show();
                    !was_visible
                } else {
                    false
                }
            })?
        };
        if needs_shown_audit {
            let _ = audit_write(
                &*audit,
                "hud",
                "shown-after-ready",
                Some("info"),
                Some(serde_json::json!({
                    "ready_count": ready_count
                })),
            );
        }
    }

    let mut health = hud_health.lock().map_err(|e| e.to_string())?;
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

/// Ajuste la fenêtre HUD à la boîte englobante du contenu WebView (mode `content` uniquement).
#[tauri::command]
pub fn set_hud_window_size(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
    hud: State<'_, SharedHud>,
) -> Result<(), String> {
    {
        let hs = hud.lock().map_err(|e| e.to_string())?;
        if normalize_size_mode(&hs.size_mode) != "content" {
            return Ok(());
        }
    }
    let w = width.clamp(260.0, 920.0);
    let h = height.clamp(120.0, 960.0);
    let app2 = app.clone();
    dispatch_on_main_thread(&app, move || {
        if let Some(win) = app2.get_webview_window("hud") {
            let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(w, h)));
        }
        Ok::<(), String>(())
    })??;
    Ok(())
}

#[tauri::command]
pub fn set_system_hud_presentation(
    app: tauri::AppHandle,
    payload: HudPresentationPayload,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.preset = match payload.preset.as_str() {
            "mini" | "compact" | "detailed" => payload.preset,
            _ => "compact".to_string(),
        };
        hs.opacity = payload.opacity.clamp(0.3, 1.0);
        hs.size_mode = normalize_size_mode(&payload.size_mode);
        hs.screen_width_pct = payload.screen_width_pct.clamp(5.0, 95.0);
        hs.screen_height_pct = payload.screen_height_pct.clamp(5.0, 95.0);
        hs.manual_width = payload.manual_width.clamp(240.0, 1600.0);
        hs.manual_height = payload.manual_height.clamp(120.0, 1200.0);
        hs.visible_metrics = normalize_visible_metrics(payload.visible_metrics);
    }
    let snapshot = hud.lock().map_err(|e| e.to_string())?.clone();
    let preset_audit = snapshot.preset.clone();
    let size_mode_audit = snapshot.size_mode.clone();
    let app2 = app.clone();
    dispatch_on_main_thread(&app, move || {
        apply_hud_window_mode(&app2, &snapshot)?;
        Ok::<(), String>(())
    })??;
    let _ = audit_write(
        &*audit,
        "hud",
        "presentation",
        Some("info"),
        Some(serde_json::json!({
            "preset": preset_audit,
            "size_mode": size_mode_audit,
        })),
    );
    Ok(())
}

#[tauri::command]
pub fn list_displays(app: tauri::AppHandle) -> Result<Vec<DisplayInfo>, String> {
    let app2 = app.clone();
    Ok(dispatch_on_main_thread(&app, move || list_displays_internal(&app2))??)
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
    let snapshot = hud.lock().map_err(|e| e.to_string())?.clone();
    let display_audit = snapshot.display_index;
    let app2 = app.clone();
    dispatch_on_main_thread(&app, move || {
        apply_hud_window_mode(&app2, &snapshot)?;
        Ok::<(), String>(())
    })??;
    let _ = audit_write(
        &*audit,
        "hud",
        "display",
        Some("info"),
        Some(serde_json::json!({ "display_index": display_audit })),
    );
    Ok(())
}
