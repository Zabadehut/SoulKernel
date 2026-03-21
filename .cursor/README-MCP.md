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
| Index graphe / GitNexus | AstraNote `docs/MCP_AND_INDEXING.md` | **non** (optionnel ; lourd ; Rocky 9 → conteneur `:z`) |

## Autonomie agent + moins de tokens

1. **Indexation Cursor** : *Settings → Features → Codebase Indexing* ; dans le chat préférer **`@codebase`** ou ciblage de fichiers plutôt que coller tout le dépôt.
2. **`.cursorignore`** à la racine : exclut `target/`, artefacts lourds → index plus léger, réponses plus ciblées.
3. **GitHub MCP** : définir `GITHUB_TOKEN` dans l’environnement Cursor (PAT avec scopes selon besoin). Sans token, désactiver le serveur **github** dans l’UI MCP pour éviter les erreurs au démarrage.

Variables optionnelles : **`CONTEXT7_API_KEY`** ([dashboard](https://context7.com/dashboard)).

## Build multi-OS

- CI : `.github/workflows/ci.yml` — `cargo clippy` + `cargo test` sur Ubuntu (deps WebKit Tauri), Windows, macOS.
- Développement local : voir `README.md` ; chemins : privilégier `std::path::Path` / APIs existantes dans `src/platform/`.

## Dépannage

- **Output → MCP** dans Cursor pour les logs.
- Paquet npm **404** : vérifier avec `npm view <nom>` (certains tutos citent des paquets `@modelcontextprotocol/…` inexistants).
- **GitHub MCP en erreur** : absent ou invalide `GITHUB_TOKEN` → retirer le bloc `github` de `mcp.json` ou exporter un PAT valide.
