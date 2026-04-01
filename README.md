# SoulKernel

**Orchestrateur Performance Dome** — migration en cours vers une architecture **Rust native first**, cross‑plateforme. Le frontend Tauri reste présent en compatibilité temporaire pour l’audit riche.

**Vision** : SoulKernel orchestre l’activité des OS (charge, mémoire, priorités, politiques) pour réduire ou stabiliser la consommation électrique du PC ; la mesure **au secteur** (prise connectée) sert de validation — voir [`docs/VISION.md`](docs/VISION.md) et [`docs/MEROSS.md`](docs/MEROSS.md) (ex. prise **MSS315ZF** Meross via `scripts/meross_mss315_bridge.py` et runtime Python embarqué optionnel dans le bundle).

## Preuve que ça fonctionne

- **Avant / Après** : quand le dôme est actif, l’UI affiche les métriques système (CPU, RAM, σ) avant activation → maintenant.
- **Historique des gains** : chaque activation enregistre π et 𝒟 ; résumé de session (moyenne 𝒟).
- **Export fichier** : bouton « Exporter (fichier) » → dialogue « Enregistrer sous » → JSON (historique + snapshot avant + résumé). Idéal pour garder une trace ou démontrer l’effet.
- **Copier résumé** : copie dans le presse‑papier (tableau texte) pour coller dans un rapport ou une note.

## Architecture

```
SoulKernel/
├── crates/
│   ├── soulkernel-core/      ← coeur métier partagé, sans dépendance Tauri
│   ├── soulkernel-headless/  ← collecte + audit JSONL sans UI permanente
│   └── soulkernel-lite/      ← UI native Rust légère (egui/eframe)
├── src/
│   ├── main.rs          ← entrée Tauri historique + invoke handlers
│   ├── audit.rs         ← wrappers Tauri sur l’audit du core
│   ├── hud.rs           ← System HUD overlay management
│   └── lib.rs           ← ré-export de `soulkernel-core`
├── ui/
│   ├── package.json     ← Vite 6 + Svelte 5 (`npm run dev` / `npm run build` → `ui/dist`)
│   ├── index.html       ← entrée Vite (fenêtre principale)
│   ├── hud.html         ← entrée Vite (overlay HUD)
│   ├── src/             ← `App.svelte`, styles, logique UI (bootstrap legacy en TS)
│   ├── hud.html         ← HUD overlay window
│   ├── styles/
│   │   └── design-tokens.css  ← tokens + styles Lucide (.lucide)
│   └── vendor/
│       └── lucide.min.js      ← Lucide UMD (offline, même version que `index.html`)
├── scripts/
│   ├── sync-lucide-ui.sh        ← régénère ui/vendor/lucide.min.js (mise à jour Lucide)
│   ├── meross_mss315_bridge.py  ← prise Meross (optionnel) → JSON watts pour validation secteur
│   ├── rocky-tauri-dev.sh       ← Rocky 9 : shell dans l’image Fedora Tauri (voir § Dev)
│   ├── Containerfile.fedora-tauri
│   ├── trusted-sign.ps1         ← modèle optionnel (Trusted Signing / CI signé, si tu configures)
│   └── cargo-msvc.example.cmd  ← Modèle MSVC Windows (copier en `cargo-msvc.cmd` local, voir .gitignore)
├── gen/schemas/         ← Schémas Tauri (référence outils / IDE)
├── icons/               ← icon.ico, icon.png
├── Cargo.toml
├── tauri.conf.json
└── build.rs
```

L’interface web n’existe **que** sous `ui/` — pas de copie à la racine du dépôt.
Le découpage détaillé de la migration est documenté dans [`docs/migration-native-first.md`](docs/migration-native-first.md).

## Développement

- **CI multi-OS** : `.github/workflows/ci.yml` exécute `cargo clippy` et `cargo test` sur Ubuntu (dépendances WebKit Tauri), Windows et macOS. Ce workflow **ne crée pas** de page [Releases](https://github.com/Zabadehut/SoulKernel/releases) ni n’y dépose de fichiers — il ne fait que valider le code à chaque push/PR.
- **Releases GitHub** : `.github/workflows/release.yml` lance `cargo tauri build` et publie les bundles **uniquement** lorsque tu pousses un **tag Git** du type `v1.1.2` (même numéro que `version` dans `Cargo.toml` / `tauri.conf.json`). Exemple :
  ```bash
  git tag v1.1.2   # adapter à la version du manifeste
  git push origin v1.1.2
  ```
  Ensuite, onglet **Actions** puis **Release** : les installateurs (.msi, .dmg, .AppImage/.deb selon config Tauri) apparaissent sous **Releases** une fois le workflow vert. Le workflow prépare aussi un **runtime Python embarqué** pour la feature Meross avant le bundle Tauri, afin que l’utilisateur final n’ait pas à installer Python ni `meross-iot`. À la fin du workflow, un job ajoute les **empreintes SHA256** dans la description de la release et publie le fichier **`SHA256SUMS`** (intégrité des fichiers ; ce n’est pas une signature éditeur Windows).
- **Windows — signature** : les [Releases](https://github.com/Zabadehut/SoulKernel/releases) sont construites avec **`--no-sign`** tant qu’aucun certificat n’est dans le dépôt (pas d’avertissement côté CI). **Build local** : `cargo tauri build --no-sign` si tu n’as pas encore de certificat. Quand tu auras un **certificat OV** ou un compte **Trusted Signing** éligible, tu pourras retirer `--no-sign` dans `.github/workflows/release.yml`, réactiver `bundle.windows.signCommand` / thumbprint dans `tauri.conf.json` et utiliser le modèle `scripts/trusted-sign.ps1` si besoin.
- **MCP Cursor** (autonomie agent, docs, GitHub optionnel) : `.cursor/mcp.json` et `.cursor/README-MCP.md` — indexation légère : `.cursorignore` + `@codebase` dans le chat.

## Téléchargements — confiance et sécurité par OS

Les binaires sur [Releases](https://github.com/Zabadehut/SoulKernel/releases) sont **produits par la CI** à partir du code public. Tant qu’il n’y a **pas de certificat de signature de code** (payant), les systèmes peuvent afficher des avertissements : c’est **normal** ; les empreintes **SHA256** permettent seulement de vérifier que le fichier n’a pas été modifié **après** publication sur GitHub (intégrité), pas d’établir l’identité légale d’un éditeur comme le ferait un certificat.

### Vérifier une release (SHA256)

1. Télécharge l’installateur **et** le fichier **`SHA256SUMS`** attaché à la même release.
2. Dans le dossier qui contient les deux :
   ```bash
   sha256sum -c SHA256SUMS
   ```
   Sous Windows (PowerShell), si `sha256sum` n’est pas dispo, compare manuellement avec la liste dans la description de la release ou utilise un outil tiers pour calculer le hash du fichier.

### Windows

- **SmartScreen / « Windows a protégé votre PC » / éditeur inconnu** : arrive souvent sur les `.exe` / `.msi` **téléchargés depuis Internet** sans signature reconnue. Clique sur **Plus d’infos** puis **Exécuter quand même** si tu fais confiance au dépôt et à la release.
- **Fichier bloqué** : clic droit sur le fichier → **Propriétés** → coche **Débloquer** si la case est présente → OK.
- **Antivirus** : en cas de faux positif rare, signale-le à l’éditeur de l’AV ou utilise une exclusion **à ta discrétion** (ne recommande pas d’exclusions génériques à toute la communauté).

### macOS

- Après téléchargement, si macOS refuse d’ouvrir l’app : **Réglages système → Confidentialité et sécurité** et autorise l’ouverture, ou clic droit sur l’app / le `.dmg` → **Ouvrir** → confirmer.
- Si Gatekeeper mentionne un développeur non identifié : même logique — ouvrir depuis le **Finder** avec clic droit **Ouvrir** la première fois.

### Linux

- **AppImage** : après téléchargement, rendre exécutable puis lancer :
  ```bash
  chmod +x SoulKernel_*.AppImage
  ./SoulKernel_*.AppImage
  ```
- **.deb** : installe avec ton gestionnaire de paquets ou `sudo apt install ./fichier.deb` (Debian/Ubuntu).

### Quand un certificat sera disponible

Une fois un **certificat de signature de code** (individuel ou autre) obtenu, les installateurs pourront être **signés** ; les avertissements du type « éditeur inconnu » deviennent en général **moins fréquents** après accumulation de réputation. En attendant, la combinaison **CI publique + SHA256** vise à rassurer la communauté sur l’**intégrité** des fichiers.

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

# Linux extra deps (Debian/Ubuntu)
sudo apt install libwebkit2gtk-4.1-dev libssl-dev libgtk-3-dev libglib2.0-dev pkg-config

# Linux extra deps (Fedora — adapté à Tauri v2 + WebKitGTK 4.1)
sudo dnf install -y pkgconf-pkg-config gcc gcc-c++ make cmake patchelf openssl-devel gtk3-devel glib2-devel \
  cairo-devel cairo-gobject-devel pango-devel gdk-pixbuf2-devel atk-devel librsvg2-devel \
  webkit2gtk4.1-devel libsoup3-devel libappindicator-gtk3-devel

# Python hôte pour préparer le runtime Meross embarqué
python3 --version

# ── Rocky Linux 9 / RHEL 9 / Alma 9 : lire ceci avant de perdre du temps ─────────
# Les dépôts EL9 restent en **glib 2.68.x**. Or gtk-rs / wry (Tauri 2) demandent
# **glib / gio / gobject >= 2.70** dans pkg-config. Résultat typique :
#   pkg-config ... 'glib-2.0 >= 2.70' → échec (« library not found »).
# Vérifier sur ta machine :
#   pkg-config --print-errors --cflags 'glib-2.0 >= 2.70'
# Si ça échoue alors que glib2-devel est installé : **tu ne pourras pas compiler
# Tauri v2 nativement sur cet OS** sans remplacer la stack système (déconseillé).
# Solutions : **Toolbox / Distrobox Fedora 40+**, **VM Ubuntu 22.04+**, ou **Podman**
# avec une image récente, en montant le dépôt :
#
# Image prête : voir `scripts/Containerfile.fedora-tauri` puis :
#   podman build -f scripts/Containerfile.fedora-tauri -t soulkernel-tauri-dev .
#   podman run -it --rm --network host -e DISPLAY -v /tmp/.X11-unix:/tmp/.X11-unix \
#     -v "$HOME/dev/SoulKernel:/work:Z" -w /work soulkernel-tauri-dev
#   # dans le conteneur : cargo tauri dev (Rust + tauri-cli sont dans l’image après rebuild du Containerfile)
#
# Sur l’hôte Rocky tu peux quand même faire **cargo clippy / cargo test** si les deps
# Rust seules suffisent ; dès qu’il faut **lier WebKit/GTK**, il faut l’environnement récent.
#
# **Valider la GUI sans `tauri dev` sur Rocky** : déclencher le workflow GitHub
# « Build Linux bundle (artifact) » (`.github/workflows/build-linux-artifact.yml`),
# puis télécharger l’artefact `soulkernel-linux-bundle` (`.deb` / `.AppImage` dans
# `target/release/bundle/` côté CI). Ce n’est pas du mode dev avec rechargement à chaud,
# mais c’est adapté aux hôtes où la stack glib < 2.70 empêche le build Tauri 2 natif.
# Si le binaire refuse de démarrer (glibc trop vieille sur l’hôte), tester sur une machine
# plus récente ou s’appuyer sur le conteneur Fedora pour builder/lancer localement.
```

### Prérequis Node.js (obligatoire pour `cargo tauri dev`)

L’UI est servie par **Vite** (`beforeDevCommand` = `npm --prefix ui run dev`). Sans `npm` dans le `PATH`, Tauri échoue immédiatement.

- **Rocky / RHEL / Alma 9 (hôte)** : `sudo dnf install -y nodejs npm`, puis une fois dans le dépôt : `npm ci --prefix ui` (ou `npm install --prefix ui`).
- **Image Fedora Tauri** (`scripts/Containerfile.fedora-tauri`) : **Node + npm** sont installés dans l’image ; après un pull qui modifie le `Containerfile`, refaire un `podman build …` puis **`npm ci --prefix ui`** dans `/work` au premier lancement.
- **Windows / macOS** : installer Node depuis [nodejs.org](https://nodejs.org/) ou votre gestionnaire habituel, puis `npm ci --prefix ui`.

### Dev
```bash
python scripts/prepare_embedded_python.py
cargo tauri dev

# Ou lancer l’app sans hot-reload :
cargo run
```

**UI 100 % offline** : Lucide via `ui/public/vendor/lucide.min.js` (aucun CDN au runtime). Mise à jour du bundle : `./scripts/sync-lucide-ui.sh` (copier le fichier généré vers `ui/public/vendor/`). Typo : pile monospace système, sans Google Fonts.

**Rocky 9 / EL9 — GUI Tauri + itération sur `ui/`** : sur l’hôte, **`cargo tauri dev` ne peut pas linker** WebKit (glib système &lt; 2.70). Ce n’est pas une limite de ton code : il faut exécuter le **même dépôt** dans une couche où glib/WebKit sont récents. Le flux le plus direct :

1. Une fois : `podman build -f scripts/Containerfile.fedora-tauri -t soulkernel-tauri-dev .`
2. À chaque session : `./scripts/rocky-tauri-dev.sh` (monte le repo, réseau = hôte, X11 pour afficher la fenêtre sur ton bureau Rocky).
3. Dans le conteneur : **`npm ci --prefix ui`** (première fois ou après changement des deps), puis **`cargo tauri dev`**. Tu édites `ui/` **sur Rocky** : le watcher recharge la webview quand les fichiers changent. Ne relance pas `./scripts/rocky-tauri-dev.sh` depuis l’intérieur du conteneur — ce script est réservé à l’hôte.

Si tu lances **`cargo tauri dev` sur l’hôte Rocky** (hors conteneur) avec une stack WebKit assez récente, installe quand même **Node + npm** comme ci-dessus pour satisfaire `beforeDevCommand`.

**GTK / « Failed to initialize GTK » dans le conteneur** : en général l’hôte n’autorise pas le **root** du conteneur sans cookie X11. Le script monte `~/.Xauthority` et force `GDK_BACKEND=x11`. Lance `./scripts/rocky-tauri-dev.sh` **sans** `sudo` depuis ta session graphique si possible. Si **SELinux** bloque encore le socket : `TAURI_PODMAN_LABEL_DISABLE=1 ./scripts/rocky-tauri-dev.sh` (uniquement en dev).

**Panique `Failed to initialize gtk backend!` / `Failed to initialize GTK`** (souvent juste après `Running target/debug/soulkernel`) : la WebView **ne démarre pas** car aucun affichage n’est utilisable.

| Situation | Piste |
|-----------|--------|
| Tu es **root** dans le conteneur Fedora sans avoir passé par `rocky-tauri-dev.sh` | Utiliser le script depuis l’hôte (il configure `DISPLAY`, `XAUTHORITY`, `GDK_BACKEND=x11`). |
| **Pas de variable `DISPLAY`** (SSH sans `-X`, session tty, serveur sans GUI) | Te connecter en session graphique locale, ou `export DISPLAY=:0` (ou l’écran correct) + droits X11 (`xhost +SI:localuser:root` en dernier recours, **dev seulement**). |
| Tu lances `cargo tauri dev` **sur l’hôte Rocky** sans Wayland/X11 fonctionnel | Préférer le flux **conteneur Fedora + rocky-tauri-dev** (WebKit récent + X11 vers ton bureau). |

Vite (`npm run dev`) peut être **OK** alors que le binaire Rust plante : le serveur front n’a pas besoin de GTK ; seul le processus Tauri en a besoin.

**Windows (MSVC)** : si `cargo` ne trouve pas les outils de lien, ouvrir une *Developer Command Prompt* ou s’inspirer de `scripts/cargo-msvc.example.cmd` (copie locale `cargo-msvc.cmd`, ignorée par git).

### Release (packaging pour distribution / vente)
```bash
python scripts/prepare_embedded_python.py
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

## Green IT — Preuve d'économie d'énergie

SoulKernel mesure et prouve l'impact énergétique en temps réel, **même sur desktop sans batterie**.

### Deux niveaux de preuve (jamais d'estimation)

| Source | Disponibilité | Ce qui est mesuré |
|--------|---------------|--------------------|
| **RAPL** (Intel/AMD powercap) | Tout x86 (desktop + laptop) | Watts réels → kWh, coût (€), kg CO₂ |
| **Batterie** (`power_supply`) | Laptops | Watts réels → kWh, coût, CO₂ |
| **Différentiel CPU** | **Universel** | CPU·heures économisées = ∫(CPU%_off − CPU%_on) × dt |

Le différentiel CPU est toujours disponible : il compare l'utilisation CPU mesurée quand le dôme est actif vs inactif. C'est un calcul sur données réelles, pas une estimation.

### Compteur vie entière

Dès le premier lancement, SoulKernel accumule dans `lifetime_gains.json` :
- Nombre total d'activations dome
- Heures de dome cumulées
- CPU·heures économisées (différentiel mesuré)
- kWh mesurés (quand RAPL/batterie disponible)
- kg CO₂ évités + coût économisé
- Intégrale réelle Σ(πₘ × dtₘ) — gain dome cumulé sur données réelles

Ces données sont affichées dans le panneau **GREEN IT · IMPACT** au centre de l'UI et dans le HUD compact.

## What gets written to the kernel

### Linux
- `/proc/sys/vm/swappiness`
- `/sys/devices/system/cpu/*/cpufreq/scaling_governor`
- `/sys/block/zram0/disksize`
- `/sys/block/<detected>/queue/scheduler` (auto-detected primary disk)
- `/sys/block/<detected>/queue/read_ahead_kb`
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
