# Native-First Migration

SoulKernel bascule vers une architecture `Rust native first`.

## Cibles

- `crates/soulkernel-core`
  - coeur métier partagé
  - sans dépendance Tauri
  - utilisé par Tauri, headless et UI native
- `crates/soulkernel-headless`
  - collecte et audit sans UI
- `crates/soulkernel-lite`
  - UI native quotidienne à faible overhead
- Tauri
  - compat temporaire pour l’audit riche existant
- vue web optionnelle
  - prochaine étape
  - servie localement à la demande

## Ce qui est extrait dès maintenant

- métriques
- télémétrie
- process impact de base
- SoulRAM / plateforme
- bridge puissance externe
- benchmark
- règles d’orchestration
- audit JSONL générique

## Ordre de migration

1. créer `soulkernel-core` ✅
2. brancher `headless` ✅
3. brancher `lite` ✅
4. garder Tauri en compat ✅
5. déplacer les commandes Tauri vers des wrappers autour du core ✅
6. remplacer l’UI WebView permanente par une vue web locale optionnelle

## Notes

- `src/main.rs` consomme désormais `soulkernel_core::{benchmark, external_power, formula, metrics,
  orchestrator, platform, telemetry, workload_catalog}` directement — plus de doublons locaux
- `src/inventory` supprimé : `get_device_inventory` appelle `soulkernel_core::inventory::collect_device_inventory()`
  et override uniquement `displays` avec la détection Tauri/WebView (résolution, scale, primary)
- les chemins de données Tauri (télémétrie, lifetime) utilisent maintenant les fonctions core
  (`~/.local/share/SoulKernel/`) — cohérence garantie avec headless et lite
- seuls `mod audit` (commandes Tauri audit_log_event / get_audit_log_path) et `mod hud` (WebView HUD)
  restent locaux car strictement Tauri-spécifiques
- prochaine étape : vue web locale optionnelle servie par axum/tiny_http, consommée depuis le lite
