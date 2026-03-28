use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()))
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
    let enabled = std::env::var("SOULKERNEL_PREPARE_EMBEDDED_PYTHON")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    if !enabled {
        println!("cargo:warning=runtime Python embarqué désactivé par SOULKERNEL_PREPARE_EMBEDDED_PYTHON");
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
        panic!(
            "Impossible de préparer le runtime Python embarqué: aucun python hôte trouvé. \
Définis SOULKERNEL_BUILD_PYTHON ou désactive temporairement avec SOULKERNEL_PREPARE_EMBEDDED_PYTHON=0."
        );
    };

    let script = repo_root()
        .join("scripts")
        .join("prepare_embedded_python.py");
    let status = Command::new(host_python)
        .arg(script)
        .arg("--target")
        .arg(target)
        .status()
        .expect("Échec du lancement de prepare_embedded_python.py");
    if !status.success() {
        panic!("prepare_embedded_python.py a échoué");
    }
}

fn main() {
    println!("cargo:rerun-if-changed=scripts/prepare_embedded_python.py");
    println!("cargo:rerun-if-changed=runtime/python/README.md");
    println!("cargo:rerun-if-env-changed=SOULKERNEL_PREPARE_EMBEDDED_PYTHON");
    println!("cargo:rerun-if-env-changed=SOULKERNEL_BUILD_PYTHON");
    prepare_embedded_python();
    tauri_build::build()
}
