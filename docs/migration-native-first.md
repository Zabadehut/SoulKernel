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

1. créer `soulkernel-core`
2. brancher `headless`
3. brancher `lite`
4. garder Tauri en compat
5. déplacer progressivement les commandes Tauri vers des wrappers autour du core
6. remplacer ensuite l’UI WebView permanente par une vue web locale optionnelle

## Notes

- l’extraction actuelle garde encore le binaire Tauri sur ses modules locaux pour limiter le risque de régression immédiate
- la librairie racine ré-exporte désormais `soulkernel-core`
- l’étape suivante consiste à faire consommer explicitement le crate `soulkernel-core` par `src/main.rs`
