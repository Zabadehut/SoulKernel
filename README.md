# SoulKernel

**Orchestrateur Performance Dome** — Tauri + Rust, cross‑plateforme. Donne un maximum d’amplitude au processus cible et **prouve les gains** (export + avant/après).

## Preuve que ça fonctionne

- **Avant / Après** : quand le dôme est actif, l’UI affiche les métriques système (CPU, RAM, σ) avant activation → maintenant.
- **Historique des gains** : chaque activation enregistre π et 𝒟 ; résumé de session (moyenne 𝒟).
- **Export fichier** : bouton « Exporter (fichier) » → dialogue « Enregistrer sous » → JSON (historique + snapshot avant + résumé). Idéal pour garder une trace ou démontrer l’effet.
- **Copier résumé** : copie dans le presse‑papier (tableau texte) pour coller dans un rapport ou une note.

## Architecture

```
SoulKernel/
├── src/
│   ├── main.rs          ← Tauri entry + invoke handlers
│   ├── metrics.rs       ← Hardware collection r(t)
│   ├── formula.rs       ← Math engine (pure)
│   ├── orchestrator.rs  ← Dome activate / rollback
│   └── platform/
│       ├── mod.rs       ← Cross-platform router
│       ├── linux.rs     ← /proc, /sys, cgroups v2, zRAM
│       ├── windows.rs   ← Job Objects, affinity, powercfg
│       └── macos.rs     ← QoS, pmset, IOKit
├── ui/
│   └── index.html       ← Frontend embarqué (zero deps)
├── icons/               ← Icône Windows (icon.ico)
├── Cargo.toml
├── tauri.conf.json
└── build.rs
```

## Tauri Commands (invoke)

| Command                    | Args                              | Returns            |
|----------------------------|-----------------------------------|--------------------|
| `get_metrics`              | —                                 | `ResourceState`    |
| `compute_formula`          | `state, profile, kappa`           | `FormulaResult`    |
| `activate_dome`            | `workload, kappa, sigma_max, eta, targetPid?` | `DomeResult` |
| `rollback_dome`            | —                                 | `Vec<String>`      |
| `list_processes`           | —                                 | `Vec<ProcessInfo>` |
| `get_snapshot_before_dome` | —                                 | `Option<ResourceState>` (preuve avant) |
| `export_gains_to_file`     | `content` (JSON string)           | chemin enregistré  |
| `platform_info`            | —                                 | `PlatformInfo`     |

## Build

### Prerequisites
```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Tauri CLI (v2)
cargo install tauri-cli

# Linux extra deps
sudo apt install libwebkit2gtk-4.0-dev libssl-dev libgtk-3-dev
```

### Dev
```bash
cargo tauri dev

# Ou lancer l’app sans hot-reload :
cargo run
```

### Release (packaging pour distribution / vente)
```bash
cargo tauri build
# Output: target/release/bundle/
# Linux:   .deb + .AppImage
# Windows: .msi + .exe
# macOS:   .dmg + .app
```
Le bundle utilise `productName`, `identifier` et la description dans `tauri.conf.json` pour les installateurs.

## Permissions (Linux)

Full orchestration requires either root or specific capabilities:

```bash
# Option 1: run as root
sudo ./soulkernel

# Option 2: grant capabilities (recommended)
sudo setcap cap_sys_admin,cap_sys_nice+ep ./soulkernel
```

## Formula

```
D*[τ₀,τ₁] = max_P [ ∫ π(t) dt  −  C_setup  −  C_rollback ]

where:
  π(t)   = (𝒲 · r(t)) · ∏_k (1−ε_k)^α_k · e^{−κΣ(t)}
  r(t)   = [C(t), M(t), Λ(t), B_io(t), G(t)]  ∈ [0,1]^5
  𝒲      = diag(α_C, α_M, α_Λ, α_io, α_G)  workload tensor
  Σ(t)   = PSI_cpu + PSI_mem + mem_pressure  (global stress)
  κ      = stability sensitivity parameter
```

## What gets written to the kernel

### Linux
- `/proc/sys/vm/swappiness`
- `/sys/devices/system/cpu/*/cpufreq/scaling_governor`
- `/sys/block/zram0/disksize`
- `/sys/block/sda/queue/scheduler`
- `/sys/block/sda/queue/read_ahead_kb`
- `/sys/fs/cgroup/soulkernel/{cpuset.cpus, cgroup.procs}`
- `/proc/sys/vm/drop_caches`

### Windows
- `powercfg /setactive <guid>`  (High Performance ↔ Balanced)
- `SetProcessAffinityMask()`
- `SetProcessWorkingSetSize()`
- `SetPriorityClass(HIGH_PRIORITY_CLASS)`
- `Disable-MMAgent -MemoryCompression`

### macOS
- `pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE)`
- `pmset -a autopoweroff 0` / `sleep 0`
- `launchctl limit cpu unlimited`
- `setiopolicy_np(IOPOL_IMPORTANT)`
