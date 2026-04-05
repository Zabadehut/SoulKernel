# MCP — SoulKernel

Fichier : [`.cursor/mcp.json`](./mcp.json). Interpolation Cursor : [`${workspaceFolder}`](https://cursor.com/docs/context/mcp), secrets : `${env:GITHUB_TOKEN}`.

## Inspiration (AstraMD / AstraNote)

| Besoin | Référence | Ce dépôt |
|--------|-----------|----------|
| Fichiers workspace explicites | AstraMD `.cursor/README-MCP.md` | **filesystem** |
| HTTP hors navigateur | AstraMD (`mcp-fetch-server`) | **fetch** |
| Mémoire MCP officielle | Les deux projets | **memory** |
| Docs à jour (Tauri, crates) | AstraMD **context7** | **context7** |
| UI `ui/*.html` + webview | AstraMD / AstraNote **chrome-devtools** | **chrome-devtools** (sans télémétrie CrUX) |
| Issues / PR / API GitHub | AstraNote `.cursor/mcp.json` | **github** (token requis) |
| Rapport live complet SoulKernel | spécifique dépôt | **soulkernel-report** |
| Index graphe / GitNexus | AstraNote `docs/MCP_AND_INDEXING.md` | **non** (optionnel ; lourd ; Rocky 9 → conteneur `:z`) |

## Autonomie agent + moins de tokens

1. **Indexation Cursor** : *Settings → Features → Codebase Indexing* ; dans le chat préférer **`@codebase`** ou ciblage de fichiers plutôt que coller tout le dépôt.
2. **`.cursorignore`** à la racine : exclut `target/`, artefacts lourds → index plus léger, réponses plus ciblées.
3. **GitHub MCP** : définir `GITHUB_TOKEN` dans l’environnement Cursor (PAT avec scopes selon besoin). Sans token, désactiver le serveur **github** dans l’UI MCP pour éviter les erreurs au démarrage.

Variables optionnelles : **`CONTEXT7_API_KEY`** ([dashboard](https://context7.com/dashboard)).

## MCP SoulKernel Report

Le serveur `soulkernel-report` lit directement :

- le fichier courant `observability_samples.jsonl`
- les rotations `observability_samples-*.jsonl.gz`

Il expose via MCP :

- `get_live_report` : rapport live complet le plus récent
- `get_metric_snapshot` : extrait compact des métriques clés
- `get_timeline_samples` : ticks de timeline récents, avec archives optionnelles
- `get_observability_status` : état des fichiers, fraîcheur, rotation, archives

Usage recommandé :

1. lancer `soulkernel-lite`
2. laisser l’app écrire l’observabilité
3. utiliser le serveur MCP `soulkernel-report` depuis Cursor

Le chemin de lecture suit l’OS :

- Windows : `%APPDATA%\\SoulKernel\\telemetry\\observability_samples.jsonl`
- Linux/macOS : `$XDG_DATA_HOME/SoulKernel/telemetry/observability_samples.jsonl` ou `~/.local/share/SoulKernel/telemetry/observability_samples.jsonl`

## Build multi-OS

- CI : `.github/workflows/ci.yml` — `cargo clippy` + `cargo test` sur Ubuntu (deps WebKit Tauri), Windows, macOS.
- Développement local : voir `README.md` ; chemins : privilégier `std::path::Path` / APIs existantes dans `src/platform/`.

## Dépannage

- **Output → MCP** dans Cursor pour les logs.
- Paquet npm **404** : vérifier avec `npm view <nom>` (certains tutos citent des paquets `@modelcontextprotocol/…` inexistants).
- **GitHub MCP en erreur** : absent ou invalide `GITHUB_TOKEN` → retirer le bloc `github` de `mcp.json` ou exporter un PAT valide.
