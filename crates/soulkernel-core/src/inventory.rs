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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInventoryItem {
    pub kind: String,
    pub name: String,
    pub detail: Option<String>,
    pub status: Option<String>,
    pub evidence: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_link_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution_kind: Option<String>,
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

fn normalize_active_state(status: Option<&str>) -> Option<String> {
    let normalized = status?.trim().to_ascii_lowercase();
    let mapped = match normalized.as_str() {
        "ok" | "up" | "ready" | "present" | "detected" | "primary" | "connected" => "connected",
        "active" | "online" | "charging" | "playing" | "running" => "active",
        "idle" | "standby" | "suspended" => "idle",
        "disconnected" | "offline" | "not present" => "unknown",
        other if other.is_empty() => return None,
        other => other,
    };
    Some(mapped.to_string())
}

fn infer_physical_link_hint(item: &DeviceInventoryItem) -> Option<String> {
    let kind = item.kind.to_ascii_lowercase();
    let mut haystack = format!("{} {}", kind, item.name.to_ascii_lowercase());
    if let Some(detail) = &item.detail {
        haystack.push(' ');
        haystack.push_str(&detail.to_ascii_lowercase());
    }
    if haystack.contains("thunderbolt") {
        Some("thunderbolt".to_string())
    } else if haystack.contains("usb-c")
        || haystack.contains("type-c")
        || haystack.contains("typec")
        || kind == "typec_port"
    {
        Some("usb-c".to_string())
    } else if haystack.contains("displayport")
        || haystack.contains(" dp")
        || haystack.contains("dp ")
    {
        Some("displayport".to_string())
    } else if haystack.contains("hdmi") {
        Some("hdmi".to_string())
    } else if haystack.contains("dvi") {
        Some("dvi".to_string())
    } else if haystack.contains("vga") {
        Some("vga".to_string())
    } else if haystack.contains("jack")
        || haystack.contains("headphone")
        || haystack.contains("speaker")
        || haystack.contains("microphone")
    {
        Some("jack".to_string())
    } else if haystack.contains("bluetooth") {
        Some("bluetooth".to_string())
    } else if haystack.contains("nvme") {
        Some("nvme".to_string())
    } else if haystack.contains("sata") {
        Some("sata".to_string())
    } else if haystack.contains("pcie") || haystack.contains("pci") {
        Some("pcie".to_string())
    } else if haystack.contains("usb3")
        || haystack.contains("usb 3")
        || haystack.contains("5000 mb/s")
    {
        Some("usb3".to_string())
    } else if haystack.contains("usb2")
        || haystack.contains("usb 2")
        || haystack.contains("480 mb/s")
    {
        Some("usb2".to_string())
    } else if kind.contains("network") {
        Some("ethernet".to_string())
    } else {
        None
    }
}

fn infer_measurement_scope(evidence: &str) -> String {
    match evidence {
        "platform_measured" => "measured",
        "pd_estimated" | "pd_negotiated" => "derived",
        "display_fallback" => "fallback",
        _ => "detected",
    }
    .to_string()
}

fn infer_confidence_score(evidence: &str) -> f64 {
    match evidence {
        "platform_measured" => 0.95,
        "pd_negotiated" => 0.75,
        "pd_estimated" => 0.55,
        "display_fallback" => 0.35,
        _ => 0.65,
    }
}

fn infer_attribution_kind(scope: &str) -> String {
    match scope {
        "measured" => "observed_telemetry",
        "derived" => "modeled_from_platform_signals",
        "fallback" => "fallback_inference",
        _ => "platform_presence",
    }
    .to_string()
}

fn enrich_inventory_item(item: &mut DeviceInventoryItem) {
    if item.active_state.is_none() {
        item.active_state = normalize_active_state(item.status.as_deref());
    }
    if item.measurement_scope.is_none() {
        item.measurement_scope = Some(infer_measurement_scope(&item.evidence));
    }
    if item.confidence_score.is_none() {
        item.confidence_score = Some(infer_confidence_score(&item.evidence));
    }
    if item.physical_link_hint.is_none() {
        item.physical_link_hint = infer_physical_link_hint(item);
    }
    if item.attribution_kind.is_none() {
        if let Some(scope) = item.measurement_scope.as_deref() {
            item.attribution_kind = Some(infer_attribution_kind(scope));
        }
    }
}

fn enrich_inventory_slice(items: &mut [DeviceInventoryItem]) {
    for item in items {
        enrich_inventory_item(item);
    }
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
                ..Default::default()
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
                ..Default::default()
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
                    ..Default::default()
                });
            }
        }
    }

    // USB-C / TypeC ports via sysfs (Linux ≥ 4.x, PD state ≥ 5.x)
    if let Ok(entries) = std::fs::read_dir("/sys/class/typec") {
        // Helper: parse "[active] other" format returned by typec sysfs files.
        let active = |s: String| -> String {
            let s = s.trim();
            if let (Some(a), Some(b)) = (s.find('['), s.find(']')) {
                s[a + 1..b].to_string()
            } else {
                s.to_string()
            }
        };
        for entry in entries.flatten() {
            let port_name = entry.file_name().to_string_lossy().to_string();
            // portX only — skip portX-partner, portX-cable, etc.
            if !port_name.starts_with("port") || port_name.contains('-') {
                continue;
            }
            let path = entry.path();
            let power_role = std::fs::read_to_string(path.join("power_role"))
                .ok()
                .map(active);
            let op_mode = std::fs::read_to_string(path.join("power_operation_mode"))
                .ok()
                .map(active);
            let data_role = std::fs::read_to_string(path.join("data_role"))
                .ok()
                .map(active);
            let partner_connected = path
                .parent()
                .map(|p| p.join(format!("{port_name}-partner")).exists())
                .unwrap_or(false);

            // Map PD operation mode → estimated watts + evidence tag.
            // usb_power_delivery means full PD negotiation in progress; real
            // PDO contract watts would require parsing the USB PD sysclass.
            let (watts_hint, evidence) = match op_mode.as_deref() {
                Some("usb_power_delivery") => (None, "pd_negotiated"),
                Some("5A") => (Some(25.0_f64), "pd_estimated"),
                Some("3.0A") => (Some(15.0_f64), "pd_estimated"),
                Some("1.5A") => (Some(7.5_f64), "pd_estimated"),
                Some("default") => (Some(2.5_f64), "pd_estimated"),
                _ => (None, "platform_detected"),
            };

            let mut detail_parts: Vec<String> = Vec::new();
            if let Some(m) = &op_mode {
                detail_parts.push(format!("PD: {m}"));
            }
            if let Some(dr) = &data_role {
                detail_parts.push(dr.clone());
            }
            if let Some(w) = watts_hint {
                detail_parts.push(format!("~{w:.0} W"));
            } else if !partner_connected {
                detail_parts.push("idle".to_string());
            }

            items.push(DeviceInventoryItem {
                kind: "typec_port".to_string(),
                name: port_name.clone(),
                detail: if detail_parts.is_empty() {
                    None
                } else {
                    Some(detail_parts.join(" · "))
                },
                status: Some(power_role.unwrap_or_else(|| {
                    if partner_connected {
                        "connected"
                    } else {
                        "idle"
                    }
                    .to_string()
                })),
                evidence: evidence.to_string(),
                ..Default::default()
            });
        }
    }

    // USB power supplies via /sys/class/power_supply — active USB chargers
    // with measurable current/voltage (laptop chargers, USB-PD bricks, etc.)
    if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let supply_type = std::fs::read_to_string(path.join("type"))
                .ok()
                .map(|s| s.trim().to_string());
            if !supply_type.as_deref().unwrap_or("").starts_with("USB") {
                continue;
            }
            let online = std::fs::read_to_string(path.join("online"))
                .ok()
                .and_then(|s| s.trim().parse::<u8>().ok())
                .unwrap_or(0);
            if online == 0 {
                continue;
            }
            let current_ua = std::fs::read_to_string(path.join("current_now"))
                .ok()
                .and_then(|s| s.trim().parse::<i64>().ok());
            let voltage_uv = std::fs::read_to_string(path.join("voltage_now"))
                .ok()
                .and_then(|s| s.trim().parse::<i64>().ok());
            let power_uw = std::fs::read_to_string(path.join("power_now"))
                .ok()
                .and_then(|s| s.trim().parse::<i64>().ok());
            // Prefer direct power_now; fall back to I×V (µA × µV → pW → W).
            let watts = power_uw
                .filter(|&p| p > 0)
                .map(|p| p as f64 / 1_000_000.0)
                .or_else(|| match (current_ua, voltage_uv) {
                    (Some(i), Some(v)) if i > 0 && v > 0 => {
                        Some(i as f64 * v as f64 / 1_000_000_000_000.0)
                    }
                    _ => None,
                });
            let evidence = if watts.is_some() {
                "platform_measured"
            } else {
                "platform_detected"
            };
            items.push(DeviceInventoryItem {
                kind: "usb_power_supply".to_string(),
                name,
                detail: watts.map(|w| format!("{w:.1} W")),
                status: Some("online".to_string()),
                evidence: evidence.to_string(),
                ..Default::default()
            });
        }
    }

    items
}

#[cfg(target_os = "windows")]
fn collect_connected_endpoints() -> Vec<DeviceInventoryItem> {
    let script = r#"
      function VideoOutputLabel($code) {
        switch ([int]$code) {
          -2 { 'internal' }
          -1 { 'other' }
          0 { 'vga' }
          4 { 'dvi' }
          5 { 'hdmi' }
          10 { 'displayport_embedded' }
          11 { 'displayport_external' }
          15 { 'miracast' }
          default { "video_$code" }
        }
      }
      $classes = @('USB','Monitor','MEDIA','Bluetooth','HIDClass','Image','Ports')
      $items = @()
      $items += Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue |
        Where-Object {
          $_.Class -in $classes -or
          $_.FriendlyName -match 'USB|Bluetooth|Audio|Speaker|Headset|Headphones|Microphone|HID|Camera|HDMI|DisplayPort|DP'
        } |
        ForEach-Object {
          [PSCustomObject]@{
            Name = if ([string]::IsNullOrWhiteSpace($_.FriendlyName)) { $_.InstanceId } else { $_.FriendlyName }
            PNPClass = $_.Class
            Status = $_.Status
            Manufacturer = ''
            Service = $_.InstanceId
          }
        }
      $items += Get-CimInstance Win32_PnPEntity -ErrorAction SilentlyContinue |
        Where-Object {
          $_.PNPClass -in $classes -or
          $_.Service -match 'BTH|USBSTOR|HidUsb|usbaudio|monitor|HdAudAddService|usbhub'
        } |
        Select-Object Name, @{Name='PNPClass';Expression={$_.PNPClass}}, Status, Manufacturer, Service
      $items += Get-CimInstance Win32_SoundDevice -ErrorAction SilentlyContinue |
        Select-Object Name, @{Name='PNPClass';Expression={'AudioEndpoint'}}, Status, Manufacturer, Service
      $items += Get-CimInstance Win32_USBHub -ErrorAction SilentlyContinue |
        Select-Object Name, @{Name='PNPClass';Expression={'USBHub'}}, Status, Manufacturer, PNPDeviceID
      $items += Get-CimInstance -Namespace root\wmi WmiMonitorConnectionParams -ErrorAction SilentlyContinue |
        Where-Object { $_.Active -eq $true } |
        ForEach-Object {
          [PSCustomObject]@{
            Name = $_.InstanceName
            PNPClass = 'DisplayOutput'
            Status = 'active'
            Manufacturer = ''
            Service = (VideoOutputLabel $_.VideoOutputTechnology)
          }
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
                        let class = row
                            .get("PNPClass")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_ascii_lowercase();
                        let detail = [
                            row.get("Manufacturer")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|s| !s.is_empty()),
                            row.get("Service")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|s| !s.is_empty()),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>();
                        let kind = match class.as_str() {
                            "displayoutput" => "display_output".to_string(),
                            "audioendpoint" => "audio_endpoint".to_string(),
                            "usbhub" => "usb_hub".to_string(),
                            "" => "endpoint".to_string(),
                            // Detect USB Type-C / UCSI controllers by name
                            "usb" => {
                                let n = name.to_ascii_lowercase();
                                if n.contains("type-c")
                                    || n.contains("type c")
                                    || n.contains("ucsi")
                                    || n.contains("usb-c")
                                {
                                    "typec_port".to_string()
                                } else {
                                    "usb_device".to_string()
                                }
                            }
                            _ => class,
                        };
                        Some(DeviceInventoryItem {
                            kind,
                            name: name.to_string(),
                            detail: if detail.is_empty() {
                                None
                            } else {
                                Some(detail.join(" · "))
                            },
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
            "Name,PNPClass,Status,Manufacturer,Service",
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
        if cols.len() < 6 {
            continue;
        }
        let class = cols[2].trim();
        let service = cols[4].trim();
        if !matches!(
            class,
            "USB" | "Monitor" | "MEDIA" | "Bluetooth" | "HIDClass" | "Image" | "Ports"
        ) && !matches!(
            service,
            "BTHUSB" | "USBSTOR" | "HidUsb" | "usbaudio" | "monitor" | "HdAudAddService"
        ) {
            continue;
        }
        let name = cols[5].trim();
        if name.is_empty() {
            continue;
        }
        let manufacturer = cols.get(3).copied().unwrap_or("").trim();
        items.push(DeviceInventoryItem {
            kind: if class.trim().is_empty() {
                "endpoint".to_string()
            } else if class.eq_ignore_ascii_case("USB") {
                let n = name.to_ascii_lowercase();
                if n.contains("type-c")
                    || n.contains("type c")
                    || n.contains("ucsi")
                    || n.contains("usb-c")
                {
                    "typec_port".to_string()
                } else {
                    "usb_device".to_string()
                }
            } else {
                class.to_ascii_lowercase()
            },
            name: name.to_string(),
            detail: trim_non_empty(
                (!manufacturer.is_empty())
                    .then(|| format!("{manufacturer} · {service}"))
                    .or_else(|| (!service.is_empty()).then(|| service.to_string())),
            ),
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
    // macOS power adapter wattage via SPPowerDataType JSON
    if let Ok(pwr_out) = command_for_inventory("system_profiler")
        .args(["SPPowerDataType", "-json"])
        .output()
    {
        if pwr_out.status.success() {
            if let Ok(val) = serde_json::from_slice::<Value>(&pwr_out.stdout) {
                if let Some(arr) = val.get("SPPowerDataType").and_then(Value::as_array) {
                    for entry in arr {
                        let Some(obj) = entry.as_object() else {
                            continue;
                        };
                        if let Some(w) = obj
                            .get("sppower_charger_adapter_wattage_id")
                            .and_then(Value::as_f64)
                        {
                            if w > 0.0 {
                                json_items.push(DeviceInventoryItem {
                                    kind: "power_adapter".to_string(),
                                    name: "AC Adapter".to_string(),
                                    detail: Some(format!("{w:.0} W")),
                                    status: Some("connected".to_string()),
                                    evidence: "platform_measured".to_string(),
                                });
                            }
                        }
                    }
                }
            }
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
    // macOS power adapter wattage via SPPowerDataType text fallback
    if let Ok(pwr_out) = command_for_inventory("system_profiler")
        .arg("SPPowerDataType")
        .output()
    {
        if pwr_out.status.success() {
            let text = String::from_utf8_lossy(&pwr_out.stdout);
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                if let Some(rest) = line.trim().strip_prefix("Wattage (W):") {
                    if let Ok(w) = rest.trim().parse::<f64>() {
                        // Find the nearest charger section heading above
                        let name = lines[..i]
                            .iter()
                            .rev()
                            .map(|l| l.trim())
                            .find(|l| l.to_ascii_lowercase().contains("charger"))
                            .map(|l| l.trim_end_matches(':').to_string())
                            .unwrap_or_else(|| "AC Adapter".to_string());
                        items.push(DeviceInventoryItem {
                            kind: "power_adapter".to_string(),
                            name,
                            detail: Some(format!("{w:.0} W")),
                            status: Some("connected".to_string()),
                            evidence: "platform_measured".to_string(),
                        });
                    }
                }
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
                ..Default::default()
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

/// Affine les estimations `pd_estimated` des ports TypeC en bornant
/// par le budget puissance résiduel réel : total_machine − composants connus.
///
/// Ne s'applique PAS quand `power_watts` vient de RAPL : RAPL mesure le
/// package CPU seul, pas la consommation totale machine (GPU discret, USB,
/// stockage, backlight ne sont pas dans RAPL).
fn refine_typec_power_budget(
    endpoints: &mut [DeviceInventoryItem],
    raw: &crate::metrics::RawMetrics,
    disk_count: usize,
) {
    // RAPL = package CPU uniquement → ne représente pas le total machine.
    let total_w = match (raw.power_watts, raw.power_watts_source.as_deref()) {
        (Some(_), Some(src)) if src.contains("rapl") => return,
        (Some(w), _) if w > 0.0 => w,
        _ => return,
    };

    // Consommateurs mesurés ou estimés.
    let gpu_w = raw.gpu_power_watts.unwrap_or(0.0);
    // CPU : fraction conservatrice usage × 50 % du total, plancher 5 W.
    let cpu_w = (raw.cpu_pct / 100.0 * total_w * 0.50)
        .max(5.0)
        .min(total_w * 0.85);
    // Stockage : ~1.5 W moyen par périphérique (mix SSD/HDD).
    let disk_w = disk_count as f64 * 1.5;
    // CM, ventilateurs, chipset, rétroéclairage : plancher fixe.
    let overhead_w = 8.0_f64;

    let usb_budget = (total_w - gpu_w - cpu_w - disk_w - overhead_w).max(0.0);

    // Ports actifs avec estimation PD (pas idle, pas déjà mesurés directement).
    let active: Vec<usize> = endpoints
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.kind == "typec_port"
                && e.evidence == "pd_estimated"
                && !e.detail.as_deref().unwrap_or("").contains("idle")
        })
        .map(|(i, _)| i)
        .collect();

    if active.is_empty() {
        return;
    }

    let per_port_w = usb_budget / active.len() as f64;

    for &idx in &active {
        let item = &mut endpoints[idx];
        // Plafond = maximum PD négocié initialement stocké dans le detail (~XX W).
        let pd_max = item
            .detail
            .as_deref()
            .and_then(|d| {
                d.split(" · ")
                    .find(|p| p.starts_with('~') && p.ends_with('W'))
                    .and_then(|p| {
                        p.trim_start_matches('~')
                            .trim_end_matches('W')
                            .trim()
                            .parse::<f64>()
                            .ok()
                    })
            })
            .unwrap_or(25.0);

        let refined_w = per_port_w.min(pd_max);

        if let Some(detail) = &item.detail {
            let stripped: String = detail
                .split(" · ")
                .filter(|p| !(p.starts_with('~') && p.ends_with('W')))
                .collect::<Vec<_>>()
                .join(" · ");
            item.detail = Some(if refined_w > 0.1 {
                format!("{stripped} · ~{refined_w:.1} W")
            } else {
                stripped
            });
        }
    }
}

pub fn collect_device_inventory() -> DeviceInventoryReport {
    let platform = crate::platform::info();
    let raw = crate::metrics::collect().ok().map(|m| m.raw);

    let mut connected_endpoints = collect_connected_endpoints();
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

    if connected_endpoints.is_empty() && !displays.is_empty() {
        connected_endpoints = displays
            .iter()
            .map(|display| DeviceInventoryItem {
                kind: "monitor".to_string(),
                name: display.name.clone(),
                detail: display.detail.clone(),
                status: display.status.clone(),
                evidence: "display_fallback".to_string(),
                ..Default::default()
            })
            .collect();
    }

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
                    ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
            });
        }
    }

    // Affiner les estimations TypeC avec le budget puissance résiduel réel.
    if let Some(r) = raw.as_ref() {
        refine_typec_power_budget(&mut connected_endpoints, r, storage.len());
    }

    let mut displays = displays;
    let mut gpus = gpus;
    let mut storage = storage;
    let mut network = network;
    let mut power = power;

    enrich_inventory_slice(&mut displays);
    enrich_inventory_slice(&mut gpus);
    enrich_inventory_slice(&mut storage);
    enrich_inventory_slice(&mut network);
    enrich_inventory_slice(&mut power);
    enrich_inventory_slice(&mut connected_endpoints);

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
