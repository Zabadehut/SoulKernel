# SoulKernel — OpenMemory Guide

## User Defined Namespaces
- [Leave blank — user populates]

## Overview
SoulKernel est un orchestrateur **Performance Dome** cross‑plateforme (Tauri + Rust). Il pilote les paramètres noyau/OS (CPU, RAM, I/O, GPU) selon un profil de charge (workload) et une formule de rendement π(t). Interface : une seule page HTML (zero deps) dans `ui/`.

## Architecture

**Tauri complète** : frontend embarqué + daemon Rust cross‑platform avec orchestration kernel réelle. Pas de fetch HTTP ni WebSocket : le JS tourne dans le WebView, les commandes passent par la boundary Rust/JS native (`invoke()`). En mode navigateur seul (fichier HTML ouvert à la main), pas de simulation — affichage 0/N/A + bandeau « Lancez cargo tauri dev ».

- **Binaire** : `src/main.rs` — point d’entrée Tauri, commandes `invoke()`.
- **Lib** : `src/lib.rs` — ré‑export des modules pour usage externe/tests.
- **Métriques** : `src/metrics.rs` — polling ~500 ms, sysinfo (cross‑platform) + `/proc/pressure/*` (PSI Linux) + RAM native par OS. Chaque valeur de r(t) vient du vrai OS.
- **Formule** : `src/formula.rs` — moteur math pur (π(t), gain dôme, B_idle, profils workload).
- **Orchestrateur** : `src/orchestrator.rs` — activation/rollback du dôme (écritures noyau via `platform`).
- **Plateforme** : `src/platform/` — routage OS : `mod.rs` (dispatch), `linux.rs`, `windows.rs`, `macos.rs`.

### Ce qui est vraiment connecté au hardware

| OS | Écritures / APIs réelles |
|----|---------------------------|
| **Linux** | `/proc/sys/vm/swappiness` (write direct), `/sys/devices/system/cpu/*/cpufreq/scaling_governor` (boucle CPUs), `/sys/block/zram0/disksize` (resize zRAM), `/sys/fs/cgroup/soulkernel/` (cgroup v2 + cpuset), `/proc/sys/vm/drop_caches` (flush page cache). |
| **Windows** | `powercfg /setactive` (High Performance), `SetProcessAffinityMask()`, `SetProcessWorkingSetSize()`, `SetPriorityClass(HIGH)`, `Disable-MMAgent -MemoryCompression`. |
| **macOS** | `pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE)`, `pmset -a sleep 0`, `setiopolicy_np(IOPOL_IMPORTANT)`. |

**Build** : `cargo install tauri-cli` puis `cargo tauri build` → .deb / .AppImage (Linux), .msi / .exe (Windows), .dmg (macOS).

## Composants
| Composant | Rôle |
|-----------|------|
| SoulKernelState | État partagé (dome actif, snapshot avant dôme, workload courant, target_pid pour prioriser un processus externe). |
| ResourceState | Vecteur r(t) + sigma, epsilon, RawMetrics. |
| WorkloadProfile | Nom, alpha [5], duration_estimate_s (es, compile, ai, backup, sqlite, oracle). |
| FormulaResult | π, brut, friction, brake, dome_gain, b_idle, rentable, dimension_weights. |
| PlatformInfo | os, kernel, features, has_cgroups_v2, has_zram, has_gpu_sysfs, is_root. |

## Métriques hardware (alignement multi-OS)
| Métrique | Windows | Linux | macOS |
|----------|---------|-------|-------|
| **RAM total/available** | `raw_system_memory()` → GlobalMemoryStatusEx | `raw_system_memory()` → /proc/meminfo (MemTotal, MemAvailable) | `raw_system_memory()` → sysctl hw.memsize + vm_stat (free+inactive) |
| **CPU** | sysinfo | sysinfo | sysinfo |
| **Swap** | sysinfo (pagefile) | sysinfo | sysinfo |
| **I/O** | sysinfo (somme disk_usage processus) | idem | idem |
| **GPU** | stub (WMI/DXGI à brancher) | /sys/class/drm, AMD gpu_busy_percent | system_profiler SPDisplaysDataType |
| **Compression / PSI** | swap pressure approché | zRAM + /proc/pressure/* | swap pressure approché |

Les trois OS utilisent une API native pour la RAM ; **aucun fallback** : si `raw_system_memory()` échoue, `collect()` retourne une erreur. **Aucune simulation** : toute métrique non disponible est `Option`/`None` (affichée N/A en UI), jamais une valeur fictive (0 % GPU, débit I/O simulé, etc.).

## Patterns
- Tauri 2 : `tauri.conf.json` schéma v2, `build.frontendDist: "ui"`, `tauri-plugin-shell` pour shell.
- Icône Windows : `icons/icon.ico` requis par tauri-build (script `icons/gen_icon.ps1`).
- Linux cgroup : `/sys/fs/cgroup/soulkernel` pour le cpuset du dôme.
- **Processus cible** : `list_processes` renvoie les processus (pid, name, cpu_usage) ; `activate_dome(targetPid)` applique au PID choisi (Windows) un **maximum d’amplitude et de performance** : affinity **tous les cœurs** (0xFFFF), working set **2–4 Go** (selon η), priorité HIGH, I/O haute. Si `targetPid` null = processus courant avec réglages conservateurs (affinity 0x0F0F, working set 1 Go). Rollback restaure le processus ciblé.
- **Preuve / vendable** : `get_snapshot_before_dome` expose l’état avant dôme pour l’UI « Avant / Après ». `export_gains_to_file(content)` ouvre une boîte de dialogue (rfd) et enregistre un JSON (historique + snapshot + session_summary). Packaging : `tauri.conf.json` bundle shortDescription/longDescription pour les installateurs.
