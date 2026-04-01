use serde::{Deserialize, Serialize};

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
    let mut items = Vec::new();
    let out = std::process::Command::new("wmic")
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
    let mut items = Vec::new();
    let kinds = [
        ("SPUSBDataType", "usb_device"),
        ("SPAudioDataType", "audio_device"),
        ("SPThunderboltDataType", "thunderbolt"),
    ];
    for (datatype, kind) in kinds {
        let out = std::process::Command::new("system_profiler")
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

pub fn collect_device_inventory() -> DeviceInventoryReport {
    let platform = crate::platform::info();
    let raw = crate::metrics::collect().ok().map(|m| m.raw);

    let connected_endpoints = collect_connected_endpoints();
    let displays = connected_endpoints
        .iter()
        .filter(|item| item.kind == "display_output" || item.kind == "monitor")
        .cloned()
        .collect::<Vec<_>>();

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
