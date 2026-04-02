# Energy Evidence TODO

Ce document suit la feuille de route "sans bullshit" de SoulKernel sur la mesure, l'attribution et l'optimisation energetique.

## Regles

- `measured` = valeur issue d'un capteur ou d'une telemetrie native fiable.
- `derived` = valeur calculee depuis des signaux reels et un modele explicite.
- `budgeted` = repartition d'un residuel, utile pour la lecture, jamais pour une preuve stricte.
- `unknown` = aucune donnee defendable.
- Ne jamais afficher `derived` ou `budgeted` comme `measured`.
- Ne jamais presenter `host` et `wall` comme comparables si le watt host vient de la meme source murale.

## Chantiers critiques

- [x] Unifier partout la separation `host_power_watts` / `wall_power_watts`.
- [ ] Tagger chaque chiffre d'audit avec `evidence_level`, `evidence_source`, `confidence_score`.
- [ ] Afficher dans toutes les UIs une legende claire:
  - `Mesure reelle`
  - `Derive`
  - `Budget`
  - `Inconnu`
- [ ] Ajouter un score global de fiabilite du bilan energetique courant.
- [ ] Bloquer les comparaisons host<->wall quand un des deux cote est absent ou recycle la meme source.

## Inventaire materiel

- [ ] Lister tous les sous-systemes dans l'inventaire lite et Tauri:
  - displays
  - display outputs
  - gpu
  - storage
  - network
  - power
  - endpoints
  - audio endpoints
  - bluetooth
  - usb hubs
  - type-c / thunderbolt / docks
- [ ] Distinguer:
  - ports disponibles
  - ports occupes
  - devices presents
  - devices actifs
- [x] Ajouter un champ `active_state` standardise:
  - `idle`
  - `connected`
  - `active`
  - `online`
  - `unknown`
- [x] Ajouter un champ `physical_link_hint`:
  - `usb2`
  - `usb3`
  - `usb-c`
  - `thunderbolt`
  - `hdmi`
  - `displayport`
  - `jack`
  - `pcie`
  - `sata`
  - `nvme`

## Windows

- [ ] Ajouter un fallback Win32 natif au-dela de PowerShell/WMI:
  - `EnumDisplayDevices`
  - SetupAPI / CFGMgr32
  - eventuellement APIs audio endpoints
- [ ] Mapper les sorties video physiques `HDMI` / `DP` / `DVI` / `VGA` avec meilleur taux de succes.
- [ ] Distinguer hubs USB, peripherals USB, Bluetooth, HID, audio endpoints, camera.
- [ ] Determiner quand `Get-PnpDevice` renvoie mieux que WMI et prioriser proprement.
- [ ] Ajouter des tests de non-regression sur exemples JSON Windows.

## Linux

- [ ] Consolider `sysfs`:
  - `/sys/class/drm`
  - `/sys/class/typec`
  - `/sys/class/power_supply`
  - `/sys/bus/usb/devices`
  - `/sys/class/thunderbolt` si disponible
- [ ] Ajouter la detection des docks / hubs USB-C / PD negociations.
- [ ] Exploiter les valeurs `power_now`, `current_now`, `voltage_now` quand presentes.
- [ ] Marquer explicitement les cas `platform_measured` vs `pd_estimated`.

## macOS

- [ ] Renforcer IOKit / IORegistry pour:
  - endpoints audio
  - displays
  - power adapters
  - thunderbolt / usb-c
- [ ] Eviter de dependre uniquement de `system_profiler`.
- [ ] Identifier les sources natives qui donnent des etats fiables sans extrapolation abusive.

## Metrologie energetique

- [ ] Definir un schema commun:
  - `machine_total_measured_w`
  - `host_internal_measured_w`
  - `wall_external_measured_w`
  - `gpu_measured_or_reconciled_w`
  - `processes_derived_w`
  - `peripherals_derived_w`
  - `platform_residual_w`
- [ ] Rendre explicite la formule de reconciliation:
  - total mesure
  - composants mesures
  - derives
  - residuel
- [ ] Ajouter un seuil de refus:
  - si l'erreur de reconciliation est trop forte, afficher `insufficient_evidence`.
- [ ] Ajouter la distinction `point-in-time` vs `window-average`.
- [ ] Exporter la preuve complete dans le JSON lite et le rapport principal.

## Per-port / per-entity

- [ ] N'afficher des watts reels par port que si une telemetrie native reelle existe.
- [ ] Pour les ports sans watt reel:
  - afficher `N/A`
  - ou `W derive` si le mode derive est active
- [ ] Ajouter un mode d'affichage selectif:
  - `strict`
  - `hybrid`
  - `derived`
- [ ] Lier l'etat des entites aux activites observees:
  - debit reseau
  - I/O disque
  - GPU / displays
  - endpoints audio en lecture
  - type-c power delivery

## Processus et SoulKernel

- [ ] Continuer la decomposition:
  - machine
  - processes
  - runtime SoulKernel
  - webview
  - peripheriques
  - plateforme residuelle
- [ ] Relier l'audit de puissance a l'objectif SoulKernel:
  - reduire le calcul inutile
  - deplacer vers SoulRAM ce qui est utile en retention
  - eviter de detruire/recharger inutilement
- [ ] Ajouter une lecture explicite `gain attendu` vs `gain mesure`.

## SoulRAM / Green IT

- [ ] Ajouter une section `preuves Green IT`:
  - CPU diff
  - RAM diff
  - energie mesuree
  - energie economisee estimee
  - energie economisee mesuree si baseline exploitable
- [ ] Distinguer clairement:
  - `stabilisation`
  - `optimisation`
  - `economies mesurees`
- [ ] Ne jamais presenter un gain Green IT sans niveau de preuve.

## UI / UX

- [ ] Ajouter un panneau `Evidence` dans le power audit.
- [ ] Ajouter des chips par noeud:
  - `measured`
  - `derived`
  - `budgeted`
  - `unknown`
- [ ] Ajouter un resume machine:
  - `wall`
  - `host`
  - `explained`
  - `residual`
  - `confidence`
- [ ] Ajouter un mode `strict evidence` dans la lite UI.
  - Base posee: les items d'inventaire exportent deja `measurement_scope`, `active_state`, `physical_link_hint`, `confidence_score`, `attribution_kind`.
- [ ] Afficher `N/A` plutot que `0 W` quand la mesure n'existe pas.

## Export / Schema

- [ ] Aligner le schema lite et Tauri sur un modele unique de preuve energetique.
- [ ] Ajouter dans l'export:
  - `evidence_level`
  - `attribution_kind`
  - `confidence_score`
  - `measurement_scope`
  - Base posee: les noeuds `device_inventory.*` exposent deja ces champs.
- [ ] Conserver la separation `host` vs `wall` dans tous les exports.
- [ ] Ajouter un bloc `limitations`.

## Validation

- [ ] Construire des fixtures JSON:
  - Windows sans watt host
  - Windows avec watt host
  - Linux avec RAPL seul
  - Linux avec wall meter
  - Linux avec USB-PD
  - macOS sans source externe
- [ ] Ajouter des tests de regression UI pour les cas:
  - `host only`
  - `wall only`
  - `host + wall`
  - `no power`
- [ ] Ajouter des captures de reference pour comparer les ecrans d'audit.

## Long terme

- [ ] Evaluer une couche plugin/vendor pour le vrai per-port telemetry:
  - docks intelligents
  - PMIC / EC exposes
  - USB-PD controllers
  - cartes meres avec telemetrie exposee
- [ ] Ajouter un mode hardware-assisted quand du materiel compatible est detecte.
- [ ] Garder un mode universel qui reste honnete meme sans capteurs avances.
