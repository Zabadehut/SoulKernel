use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()))
        .join("..")
        .join("..")
}

fn find_host_python() -> Option<String> {
    let mut candidates = Vec::new();
    if let Ok(py) = std::env::var("SOULKERNEL_BUILD_PYTHON") {
        let trimmed = py.trim();
        if !trimmed.is_empty() {
            candidates.push(trimmed.to_string());
        }
    }
    #[cfg(target_os = "windows")]
    {
        candidates.push("py".to_string());
        candidates.push("python".to_string());
        candidates.push("python3".to_string());
    }
    #[cfg(not(target_os = "windows"))]
    {
        candidates.push("python3".to_string());
        candidates.push("python".to_string());
    }
    candidates.dedup();

    for candidate in candidates {
        if Command::new(&candidate)
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Some(candidate);
        }
    }
    None
}

fn prepare_embedded_python() {
    let profile = std::env::var("PROFILE").unwrap_or_default();
    let allow_in_debug = std::env::var("SOULKERNEL_PREPARE_EMBEDDED_PYTHON_IN_DEBUG")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(false);
    if profile == "debug" && !allow_in_debug {
        return;
    }

    let enabled = std::env::var("SOULKERNEL_PREPARE_EMBEDDED_PYTHON")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    if !enabled {
        return;
    }

    let target = match std::env::var("TARGET") {
        Ok(v) => v,
        Err(_) => return,
    };
    let supported = matches!(
        target.as_str(),
        "x86_64-pc-windows-msvc"
            | "x86_64-unknown-linux-gnu"
            | "x86_64-apple-darwin"
            | "aarch64-apple-darwin"
    );
    if !supported {
        return;
    }

    let Some(host_python) = find_host_python() else {
        println!("cargo:warning=runtime Python embarqué ignoré: aucun python hôte trouvé");
        return;
    };

    let script = repo_root()
        .join("scripts")
        .join("prepare_embedded_python.py");
    let status = Command::new(host_python)
        .arg(script)
        .arg("--target")
        .arg(target)
        .status();
    match status {
        Ok(status) if status.success() => {}
        Ok(_) => println!("cargo:warning=prepare_embedded_python.py a échoué pour soulkernel-lite"),
        Err(err) => {
            println!("cargo:warning=impossible de lancer prepare_embedded_python.py: {err}")
        }
    }
}

fn target_profile_dir() -> Option<PathBuf> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").ok()?);
    out_dir.parent()?.parent()?.parent().map(PathBuf::from)
}

fn copy_file_if_exists(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(src, dest).map_err(|e| e.to_string())?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn copy_embedded_resources() {
    let Some(profile_dir) = target_profile_dir() else {
        return;
    };
    let root = repo_root();
    let script_src = root.join("scripts").join("meross_mss315_bridge.py");
    let script_dest = profile_dir.join("scripts").join("meross_mss315_bridge.py");
    if let Err(err) = copy_file_if_exists(&script_src, &script_dest) {
        println!("cargo:warning=copy bridge script failed: {err}");
    }

    #[cfg(target_os = "windows")]
    let runtime_src = root.join("runtime").join("python").join("windows");
    #[cfg(target_os = "linux")]
    let runtime_src = root.join("runtime").join("python").join("linux");
    #[cfg(target_os = "macos")]
    let runtime_src = root.join("runtime").join("python").join("macos");

    let runtime_dest = profile_dir.join("runtime").join("python").join(
        runtime_src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
    );
    if let Err(err) = copy_dir_recursive(&runtime_src, &runtime_dest) {
        println!("cargo:warning=copy embedded python failed: {err}");
    }
}

fn main() {
    println!("cargo:rerun-if-changed=../../scripts/prepare_embedded_python.py");
    println!("cargo:rerun-if-changed=../../scripts/meross_mss315_bridge.py");
    println!("cargo:rerun-if-changed=../../runtime/python/README.md");
    println!("cargo:rerun-if-env-changed=SOULKERNEL_PREPARE_EMBEDDED_PYTHON");
    println!("cargo:rerun-if-env-changed=SOULKERNEL_PREPARE_EMBEDDED_PYTHON_IN_DEBUG");
    println!("cargo:rerun-if-env-changed=SOULKERNEL_BUILD_PYTHON");
    prepare_embedded_python();
    copy_embedded_resources();
}
