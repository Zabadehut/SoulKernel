//! Catalogue de scénarios workload (50 profils) : coefficients 𝒲, méta UI et hints SoulRAM par OS.
//! Données : `workload_scenes.json` (sérialisé au build).

use crate::formula::WorkloadProfile;
use serde::Deserialize;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
struct WorkloadSceneJson {
    id: String,
    label: String,
    category: String,
    alpha: [f64; 5],
    duration_estimate_s: f64,
    burst: bool,
    hardware_focus: String,
    soulram_linux: String,
    soulram_windows: String,
    soulram_macos: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadSceneDto {
    pub id: String,
    pub label: String,
    pub category: String,
    pub alpha: [f64; 5],
    pub duration_estimate_s: f64,
    pub burst: bool,
    pub hardware_focus: String,
    pub soulram_linux: String,
    pub soulram_windows: String,
    pub soulram_macos: String,
}

pub struct WorkloadCatalogEntry {
    pub profile: WorkloadProfile,
    pub category: String,
    pub label: String,
    pub burst: bool,
    pub hardware_focus: String,
    pub soulram_linux: String,
    pub soulram_windows: String,
    pub soulram_macos: String,
}

fn load_entries() -> Vec<WorkloadCatalogEntry> {
    let raw: Vec<WorkloadSceneJson> =
        serde_json::from_str(include_str!("workload_scenes.json")).expect("workload_scenes.json");
    assert_eq!(
        raw.len(),
        50,
        "catalogue: attendu 50 scénarios"
    );
    let mut out = Vec::with_capacity(raw.len());
    for j in raw {
        let sum: f64 = j.alpha.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "alpha pour {} doit sommer à 1, got {}",
            j.id,
            sum
        );
        out.push(WorkloadCatalogEntry {
            profile: WorkloadProfile {
                name: j.id.clone(),
                alpha: j.alpha,
                duration_estimate_s: j.duration_estimate_s,
            },
            category: j.category,
            label: j.label,
            burst: j.burst,
            hardware_focus: j.hardware_focus,
            soulram_linux: j.soulram_linux,
            soulram_windows: j.soulram_windows,
            soulram_macos: j.soulram_macos,
        });
    }
    out
}

fn entries_cached() -> &'static [WorkloadCatalogEntry] {
    static S: OnceLock<Vec<WorkloadCatalogEntry>> = OnceLock::new();
    S.get_or_init(load_entries)
}

pub fn all_profiles() -> Vec<WorkloadProfile> {
    entries_cached()
        .iter()
        .map(|e| e.profile.clone())
        .collect()
}

pub fn from_name(name: &str) -> Option<WorkloadProfile> {
    entries_cached()
        .iter()
        .find(|e| e.profile.name == name)
        .map(|e| e.profile.clone())
}

pub fn is_burst(name: &str) -> bool {
    entries_cached()
        .iter()
        .find(|e| e.profile.name == name)
        .map(|e| e.burst)
        .unwrap_or(false)
}

pub fn list_scenes_for_ui() -> Vec<WorkloadSceneDto> {
    entries_cached()
        .iter()
        .map(|e| WorkloadSceneDto {
            id: e.profile.name.clone(),
            label: e.label.clone(),
            category: e.category.clone(),
            alpha: e.profile.alpha,
            duration_estimate_s: e.profile.duration_estimate_s,
            burst: e.burst,
            hardware_focus: e.hardware_focus.clone(),
            soulram_linux: e.soulram_linux.clone(),
            soulram_windows: e.soulram_windows.clone(),
            soulram_macos: e.soulram_macos.clone(),
        })
        .collect()
}
