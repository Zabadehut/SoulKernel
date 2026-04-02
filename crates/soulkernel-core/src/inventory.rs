use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use serde_json::Value;

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn command_for_inventory(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn trim_non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInventoryItem {
    pub kind: String,
    pub name: String,
    pub detail: Option<String>,
    pub status: Option<String>,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInventoryReport {
    pub platform: String,
    pub displays: Vec<DeviceInventoryItem>,
    pub gpus: Vec<DeviceInventoryItem>,
    pub storage: Vec<DeviceInventoryItem>,
    pub network: Vec<DeviceInventoryItem>,
    pub power: Vec<DeviceInventoryItem>,
    pub connected_endpoints: Vec<DeviceInventoryItem>,
    pub platform_features: Vec<String>,
}

#[cfg(target_os = "linux")]
fn collect_connected_endpoints() -> Vec<DeviceInventoryItem> {
    let mut items = Vec::new();

    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.contains('-') {
                continue;
            }
            let status = std::fs::read_to_string(entry.path().join("status"))
                .ok()
                .map(|s| s.trim().to_string());
            let modes = std::fs::read_to_string(entry.path().join("modes"))
                .ok()
                .map(|s| s.lines().take(2).collect::<Vec<_>>().join(", "));
            items.push(DeviceInventoryItem {
                kind: "display_output".to_string(),
                name,
                detail: modes.filter(|s| !s.is_empty()),
                status,
                evidence: "platform_detected".to_string(),
            });
        }
    }

    if let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") {
        for entry in entries.flatten() {
            let path = entry.path();
            let product = std::fs::read_to_string(path.join("product"))
                .ok()
                .map(|s| s.trim().to_string());
            if product.as_deref().unwrap_or("").is_empty() {
                continue;
            }
            let manufacturer = std::fs::read_to_string(path.join("manufacturer"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let speed = std::fs::read_to_string(path.join("speed"))
                .ok()
                .map(|s| format!("{} Mb/s", s.trim()))
                .filter(|s| !s.starts_with('0'));
            let vendor_id = std::fs::read_to_string(path.join("idVendor"))
                .ok()
                .map(|s| s.trim().to_string());
            let product_id = std::fs::read_to_string(path.join("idProduct"))
                .ok()
                .map(|s| s.trim().to_string());
            let mut detail_parts = Vec::new();
            if let Some(v) = manufacturer {
                detail_parts.push(v);
            }
            if let Some(v) = speed {
                detail_parts.push(v);
            }
            if let (Some(v), Some(p)) = (vendor_id, product_id) {
                detail_parts.push(format!("{v}:{p}"));
            }
            items.push(DeviceInventoryItem {
                kind: "usb_device".to_string(),
                name: product.unwrap_or_else(|| "USB device".to_string()),
                detail: if detail_parts.is_empty() {
                    None
                } else {
                    Some(detail_parts.join(" · "))
                },
                status: Some("connected".to_string()),
                evidence: "platform_detected".to_string(),
            });
        }
    }

    if let Ok(cards) = std::fs::read_to_string("/proc/asound/cards") {
        for line in cards.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("---") {
                continue;
            }
            if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                items.push(DeviceInventoryItem {
                    kind: "audio_card".to_string(),
                    name: trimmed.to_string(),
                    detail: None,
                    status: Some("detected".to_string()),
                    evidence: "platform_detected".to_string(),
                });
            }
        }
    }

    items
}

#[cfg(target_os = "windows")]
fn collect_connected_endpoints() -> Vec<DeviceInventoryItem> {
    let script = r#"
      $items = Get-CimInstance Win32_PnPEntity -ErrorAction SilentlyContinue |
        Where-Object { $_.PNPClass -in @('USB','Monitor','MEDIA') } |
        Select-Object Name, PNPClass, Status
      $items | ConvertTo-Json -Compress
    "#;
    let out = command_for_inventory("powershell")
        .args(["-NoProfile", "-Command", script])
        .output();
    if let Ok(out) = out {
        if out.status.success() {
            if let Ok(value) = serde_json::from_slice::<Value>(&out.stdout) {
                let rows = match value {
                    Value::Array(rows) => rows,
                    Value::Null => Vec::new(),
                    row => vec![row],
                };
                let items = rows
                    .into_iter()
                    .filter_map(|row| {
                        let name = row.get("Name")?.as_str()?.trim();
                        if name.is_empty() {
                            return None;
                        }
                        let class = row
                            .get("PNPClass")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_ascii_lowercase();
                        Some(DeviceInventoryItem {
                            kind: class,
                            name: name.to_string(),
                            detail: None,
                            status: trim_non_empty(
                                row.get("Status")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                            ),
                            evidence: "platform_detected".to_string(),
                        })
                    })
                    .collect::<Vec<_>>();
                if !items.is_empty() {
                    return items;
                }
            }
        }
    }

    let mut items = Vec::new();
    let out = command_for_inventory("wmic")
        .args([
            "path",
            "Win32_PnPEntity",
            "get",
            "Name,PNPClass,Status",
            "/format:csv",
        ])
        .output();
    let Ok(out) = out else {
        return items;
    };
    if !out.status.success() {
        return items;
    }
    for line in String::from_utf8_lossy(&out.stdout).lines().skip(1) {
        let cols = line.split(',').collect::<Vec<_>>();
        if cols.len() < 4 {
            continue;
        }
        let class = cols[2].trim();
        if !matches!(class, "USB" | "Monitor" | "MEDIA") {
            continue;
        }
        let name = cols[3].trim();
        if name.is_empty() {
            continue;
        }
        items.push(DeviceInventoryItem {
            kind: class.to_ascii_lowercase(),
            name: name.to_string(),
            detail: None,
            status: Some(
                cols.get(1)
                    .copied()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase(),
            ),
            evidence: "platform_detected".to_string(),
        });
    }
    items
}

#[cfg(target_os = "macos")]
fn collect_connected_endpoints() -> Vec<DeviceInventoryItem> {
    let mut json_items = Vec::new();
    let kinds = [
        ("SPUSBDataType", "usb_device"),
        ("SPAudioDataType", "audio_device"),
        ("SPThunderboltDataType", "thunderbolt"),
    ];
    for (datatype, kind) in kinds {
        let out = command_for_inventory("system_profiler")
            .args([datatype, "-json"])
            .output();
        let Ok(out) = out else { continue };
        if !out.status.success() {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&out.stdout) else {
            continue;
        };
        let Some(entries) = value.get(datatype).and_then(Value::as_array) else {
            continue;
        };
        for entry in entries {
            let Some(obj) = entry.as_object() else {
                continue;
            };
            let name = obj
                .get("_name")
                .and_then(Value::as_str)
                .or_else(|| obj.get("device_title").and_then(Value::as_str))
                .unwrap_or("")
                .trim();
            if name.is_empty() {
                continue;
            }
            let detail = obj
                .get("vendor_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    obj.get("manufacturer")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            json_items.push(DeviceInventoryItem {
                kind: kind.to_string(),
                name: name.to_string(),
                detail: trim_non_empty(detail),
                status: Some("connected".to_string()),
                evidence: "platform_detected".to_string(),
            });
        }
    }
    if !json_items.is_empty() {
        return json_items;
    }

    let mut items = Vec::new();
    let kinds = [
        ("SPUSBDataType", "usb_device"),
        ("SPAudioDataType", "audio_device"),
        ("SPThunderboltDataType", "thunderbolt"),
    ];
    for (datatype, kind) in kinds {
        let out = command_for_inventory("system_profiler")
            .arg(datatype)
            .output();
        let Ok(out) = out else { continue };
        if !out.status.success() {
            continue;
        }
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let trimmed = line.trim();
            if trimmed.ends_with(':')
                && !trimmed.starts_with("Data")
                && trimmed.len() > 1
                && !trimmed.contains("Bus")
            {
                items.push(DeviceInventoryItem {
                    kind: kind.to_string(),
                    name: trimmed.trim_end_matches(':').to_string(),
                    detail: None,
                    status: Some("connected".to_string()),
                    evidence: "platform_detected".to_string(),
                });
            }
        }
    }
    items
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn collect_connected_endpoints() -> Vec<DeviceInventoryItem> {
    Vec::new()
}

#[cfg(target_os = "linux")]
fn collect_displays() -> Vec<DeviceInventoryItem> {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.contains('-') {
                continue;
            }
            let status = trim_non_empty(std::fs::read_to_string(path.join("status")).ok());
            if status.as_deref() == Some("disconnected") {
                continue;
            }
            let modes = trim_non_empty(std::fs::read_to_string(path.join("modes")).ok())
                .map(|s| s.lines().take(2).collect::<Vec<_>>().join(", "));
            items.push(DeviceInventoryItem {
                kind: "display".to_string(),
                name,
                detail: modes,
                status,
                evidence: "platform_detected".to_string(),
            });
        }
    }
    items
}

#[cfg(target_os = "windows")]
fn collect_displays() -> Vec<DeviceInventoryItem> {
    let script = r#"
      function Decode-Uint16String($values) {
        if ($null -eq $values) { return $null }
        $bytes = @($values | Where-Object { $_ -ne 0 } | ForEach-Object { [byte]$_ })
        if ($bytes.Count -eq 0) { return $null }
        return ([System.Text.Encoding]::ASCII.GetString($bytes)).Trim([char]0).Trim()
      }
      $items = @()
      $active = Get-CimInstance -Namespace root\wmi WmiMonitorID -ErrorAction SilentlyContinue |
        Where-Object { $_.Active -eq $true }
      foreach ($monitor in $active) {
        $name = Decode-Uint16String $monitor.UserFriendlyName
        if ([string]::IsNullOrWhiteSpace($name)) {
          $name = Decode-Uint16String $monitor.ManufacturerName
        }
        if ([string]::IsNullOrWhiteSpace($name)) {
          $name = $monitor.InstanceName
        }
        if ([string]::IsNullOrWhiteSpace($name)) {
          continue
        }
        $items += [PSCustomObject]@{
          Name = $name
          ScreenWidth = $null
          ScreenHeight = $null
          Status = 'active'
          MonitorType = $monitor.InstanceName
        }
      }
      if ($items.Count -eq 0) {
        $items = Get-CimInstance Win32_DesktopMonitor -ErrorAction SilentlyContinue |
          Select-Object Name, ScreenWidth, ScreenHeight, Status, MonitorType
      }
      $items | ConvertTo-Json -Compress
    "#;
    let out = command_for_inventory("powershell")
        .args(["-NoProfile", "-Command", script])
        .output();
    if let Ok(out) = out {
        if out.status.success() {
            if let Ok(value) = serde_json::from_slice::<Value>(&out.stdout) {
                let rows = match value {
                    Value::Array(rows) => rows,
                    Value::Null => Vec::new(),
                    row => vec![row],
                };
                let items = rows
                    .into_iter()
                    .filter_map(|row| {
                        let name = row.get("Name")?.as_str()?.trim();
                        if name.is_empty() {
                            return None;
                        }
                        let width = row.get("ScreenWidth").and_then(Value::as_u64);
                        let height = row.get("ScreenHeight").and_then(Value::as_u64);
                        let monitor_type = row
                            .get("MonitorType")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty() && *s != "Generic PnP Monitor")
                            .map(str::to_string);
                        let detail = match (width, height, monitor_type) {
                            (Some(w), Some(h), Some(t)) if w > 0 && h > 0 => {
                                Some(format!("{w}x{h} · {t}"))
                            }
                            (Some(w), Some(h), None) if w > 0 && h > 0 => Some(format!("{w}x{h}")),
                            (_, _, Some(t)) => Some(t),
                            _ => None,
                        };
                        Some(DeviceInventoryItem {
                            kind: "display".to_string(),
                            name: name.to_string(),
                            detail,
                            status: trim_non_empty(
                                row.get("Status")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                            ),
                            evidence: "platform_detected".to_string(),
                        })
                    })
                    .collect::<Vec<_>>();
                if !items.is_empty() {
                    return items;
                }
            }
        }
    }

    let mut items = Vec::new();
    let out = command_for_inventory("wmic")
        .args([
            "path",
            "Win32_DesktopMonitor",
            "get",
            "Name,ScreenHeight,ScreenWidth,Status",
            "/format:csv",
        ])
        .output();
    let Ok(out) = out else {
        return items;
    };
    if !out.status.success() {
        return items;
    }
    for line in String::from_utf8_lossy(&out.stdout).lines().skip(1) {
        let cols = line.split(',').map(str::trim).collect::<Vec<_>>();
        if cols.len() < 5 {
            continue;
        }
        let name = cols[2];
        if name.is_empty() {
            continue;
        }
        let detail = match (cols.get(3), cols.get(4)) {
            (Some(h), Some(w)) if !h.is_empty() && !w.is_empty() => Some(format!("{w}x{h}")),
            _ => None,
        };
        items.push(DeviceInventoryItem {
            kind: "display".to_string(),
            name: name.to_string(),
            detail,
            status: trim_non_empty(cols.get(1).map(|s| s.to_string())),
            evidence: "platform_detected".to_string(),
        });
    }
    items
}

#[cfg(target_os = "macos")]
fn collect_displays() -> Vec<DeviceInventoryItem> {
    let out = command_for_inventory("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .output();
    if let Ok(out) = out {
        if out.status.success() {
            if let Ok(value) = serde_json::from_slice::<Value>(&out.stdout) {
                if let Some(gpus) = value.get("SPDisplaysDataType").and_then(Value::as_array) {
                    let mut items = Vec::new();
                    for gpu in gpus {
                        let Some(ndrvs) = gpu.get("spdisplays_ndrvs").and_then(Value::as_array)
                        else {
                            continue;
                        };
                        for display in ndrvs {
                            let name = display
                                .get("_name")
                                .and_then(Value::as_str)
                                .or_else(|| {
                                    display
                                        .get("_spdisplays_display-product-name")
                                        .and_then(Value::as_str)
                                })
                                .unwrap_or("")
                                .trim();
                            if name.is_empty() {
                                continue;
                            }
                            let resolution = display
                                .get("_spdisplays_resolution")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(str::to_string);
                            let ui_looks = display
                                .get("_spdisplays_ui_looks_like")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(str::to_string);
                            let detail = match (resolution, ui_looks) {
                                (Some(r), Some(ui)) => Some(format!("{r} · UI {ui}")),
                                (Some(r), None) => Some(r),
                                (None, Some(ui)) => Some(format!("UI {ui}")),
                                (None, None) => None,
                            };
                            let status = if display
                                .get("spdisplays_main")
                                .and_then(Value::as_str)
                                .is_some_and(|v| v.eq_ignore_ascii_case("yes"))
                            {
                                Some("primary".to_string())
                            } else if display
                                .get("spdisplays_online")
                                .and_then(Value::as_str)
                                .is_some_and(|v| v.eq_ignore_ascii_case("yes"))
                            {
                                Some("online".to_string())
                            } else {
                                Some("detected".to_string())
                            };
                            items.push(DeviceInventoryItem {
                                kind: "display".to_string(),
                                name: name.to_string(),
                                detail,
                                status,
                                evidence: "platform_detected".to_string(),
                            });
                        }
                    }
                    if !items.is_empty() {
                        return items;
                    }
                }
            }
        }
    }

    let mut items = Vec::new();
    let out = command_for_inventory("system_profiler")
        .arg("SPDisplaysDataType")
        .output();
    let Ok(out) = out else {
        return items;
    };
    if !out.status.success() {
        return items;
    }
    let mut current_name: Option<String> = None;
    let mut current_resolution: Option<String> = None;
    let mut current_status: Option<String> = None;
    let flush_current = |items: &mut Vec<DeviceInventoryItem>,
                         current_name: &mut Option<String>,
                         current_resolution: &mut Option<String>,
                         current_status: &mut Option<String>| {
        if let Some(name) = current_name.take() {
            items.push(DeviceInventoryItem {
                kind: "display".to_string(),
                name,
                detail: current_resolution.take(),
                status: current_status.take(),
                evidence: "platform_detected".to_string(),
            });
        }
    };
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.ends_with(':')
            && !trimmed.starts_with("Displays")
            && !trimmed.starts_with("Graphics")
            && !trimmed.starts_with("Chipset")
            && !trimmed.starts_with("Vendor")
            && !trimmed.starts_with("Device")
            && !trimmed.starts_with("Bus")
            && !trimmed.starts_with("VRAM")
            && !trimmed.starts_with("Metal")
            && !trimmed.starts_with("Resolution")
            && !trimmed.starts_with("UI Looks like")
            && !trimmed.starts_with("Main Display")
            && !trimmed.starts_with("Mirror")
            && !trimmed.starts_with("Online")
            && !trimmed.starts_with("Automatically Adjust")
            && !trimmed.starts_with("Connection Type")
        {
            flush_current(
                &mut items,
                &mut current_name,
                &mut current_resolution,
                &mut current_status,
            );
            current_name = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("Resolution:") {
            current_resolution = trim_non_empty(Some(value.to_string()));
        } else if let Some(value) = trimmed.strip_prefix("UI Looks like:") {
            let ui_looks = value.trim();
            current_resolution = Some(match current_resolution.take() {
                Some(existing) => format!("{existing} · UI {ui_looks}"),
                None => format!("UI {ui_looks}"),
            });
        } else if let Some(value) = trimmed.strip_prefix("Main Display:") {
            current_status = trim_non_empty(Some(value.to_string())).map(|v| {
                if v.eq_ignore_ascii_case("yes") {
                    "primary".to_string()
                } else {
                    "active".to_string()
                }
            });
        } else if let Some(value) = trimmed.strip_prefix("Online:") {
            if current_status.is_none() {
                current_status = trim_non_empty(Some(value.to_string())).map(|v| {
                    if v.eq_ignore_ascii_case("yes") {
                        "online".to_string()
                    } else {
                        v
                    }
                });
            }
        }
    }
    flush_current(
        &mut items,
        &mut current_name,
        &mut current_resolution,
        &mut current_status,
    );
    items
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn collect_displays() -> Vec<DeviceInventoryItem> {
    Vec::new()
}

pub fn collect_device_inventory() -> DeviceInventoryReport {
    let platform = crate::platform::info();
    let raw = crate::metrics::collect().ok().map(|m| m.raw);

    let connected_endpoints = collect_connected_endpoints();
    let displays = {
        let detected = collect_displays();
        if detected.is_empty() {
            connected_endpoints
                .iter()
                .filter(|item| item.kind == "display_output" || item.kind == "monitor")
                .cloned()
                .collect::<Vec<_>>()
        } else {
            detected
        }
    };

    let gpus = raw
        .as_ref()
        .map(|raw| {
            raw.gpu_devices
                .iter()
                .map(|gpu| DeviceInventoryItem {
                    kind: "gpu".to_string(),
                    name: gpu
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("GPU {}", gpu.index)),
                    detail: Some(format!(
                        "{} · util {} · power {}",
                        gpu.vendor.clone().unwrap_or_else(|| "vendor —".to_string()),
                        gpu.utilization_pct
                            .map(|v| format!("{v:.1} %"))
                            .unwrap_or_else(|| "—".to_string()),
                        gpu.power_watts
                            .map(|v| format!("{v:.1} W"))
                            .unwrap_or_else(|| "—".to_string()),
                    )),
                    status: gpu.kind.clone(),
                    evidence: gpu
                        .confidence
                        .clone()
                        .unwrap_or_else(|| "observed_usage".to_string()),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let disks = sysinfo::Disks::new_with_refreshed_list();
    let storage = disks
        .list()
        .iter()
        .map(|disk| DeviceInventoryItem {
            kind: "storage".to_string(),
            name: disk.name().to_string_lossy().to_string(),
            detail: Some(format!(
                "{} · {} / {} GiB",
                disk.file_system().to_string_lossy(),
                ((disk.total_space().saturating_sub(disk.available_space())) as f64
                    / 1024.0
                    / 1024.0
                    / 1024.0)
                    .round(),
                (disk.total_space() as f64 / 1024.0 / 1024.0 / 1024.0).round()
            )),
            status: Some(format!("{:?}", disk.kind()).to_lowercase()),
            evidence: "platform_detected".to_string(),
        })
        .collect::<Vec<_>>();

    let networks = sysinfo::Networks::new_with_refreshed_list();
    let network = networks
        .iter()
        .map(|(name, data)| DeviceInventoryItem {
            kind: "network".to_string(),
            name: name.to_string(),
            detail: Some(format!(
                "rx {} B · tx {} B",
                data.received(),
                data.transmitted()
            )),
            status: Some("detected".to_string()),
            evidence: "platform_detected".to_string(),
        })
        .collect::<Vec<_>>();

    let mut power = Vec::new();
    if let Some(raw) = raw.as_ref() {
        if let Some(source) = raw.power_watts_source.clone() {
            power.push(DeviceInventoryItem {
                kind: "power_source".to_string(),
                name: source,
                detail: raw
                    .power_watts
                    .map(|v| format!("{v:.2} W machine"))
                    .or_else(|| Some("W machine indisponibles".to_string())),
                status: Some("active".to_string()),
                evidence: "platform_measured".to_string(),
            });
        }
        if let Some(on_battery) = raw.on_battery {
            power.push(DeviceInventoryItem {
                kind: "power_mode".to_string(),
                name: if on_battery {
                    "battery".to_string()
                } else {
                    "ac".to_string()
                },
                detail: raw
                    .battery_percent
                    .map(|v| format!("{v:.0} %"))
                    .or_else(|| Some("niveau inconnu".to_string())),
                status: Some("detected".to_string()),
                evidence: "platform_detected".to_string(),
            });
        }
    }

    DeviceInventoryReport {
        platform: platform.os,
        displays,
        gpus,
        storage,
        network,
        power,
        connected_endpoints,
        platform_features: platform.features,
    }
}
