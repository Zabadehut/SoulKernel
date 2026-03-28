# Prise Meross (ex. MSS315ZF) — conso murale pour SoulKernel

## Rôle

Les prises **Meross** mesurent la puissance **côté secteur** (ce qui compte pour ta facture). SoulKernel peut **prioriser** cette valeur par rapport au **RAPL** (CPU) pour les **W affichés** et l’**intégration kWh** dans la télémétrie, tant qu’un fichier local récent fournit les watts.

## Configuration

Fichier optionnel (répertoire config SoulKernel) :

| Plateforme | Chemin |
|------------|--------|
| Linux | `~/.config/soulkernel/meross.json` |
| Windows | `%APPDATA%\SoulKernel\meross.json` |
| macOS | `~/Library/Application Support/SoulKernel/meross.json` |

Exemple :

```json
{
  "enabled": true,
  "power_file": "/chemin/vers/meross_power.json",
  "max_age_ms": 15000
}
```

- **`enabled`** : si `false` ou fichier absent, seul le RAPL / capteurs OS habituels sont utilisés.
- **`power_file`** : JSON écrit par le pont (ci-dessous). Par défaut : `meross_power.json` dans le même répertoire que `meross.json`.
- **`max_age_ms`** : au-delà, la lecture est ignorée (données trop vieilles).

## Format du fichier puissance

Le pont (ou un test manuel) doit écrire un JSON du type :

```json
{
  "watts": 87.3,
  "ts_ms": 1730000000000
}
```

Clés acceptées pour la puissance : `watts`, `w` ou `power`.  
`ts_ms` est l’horodatage Unix **millisecondes** ; s’il est absent, le fichier est considéré comme valide dès la lecture (moins strict).

## Runtime Python embarqué

Le build Tauri embarque désormais un runtime Python minimal dédié à cette feature. SoulKernel cherche d'abord un interpréteur packagé dans les ressources, puis seulement un Python système (`python_bin`, `python3`, `python`, `py`).

Structure attendue dans le dépôt avant `cargo tauri build` :

| Plateforme | Binaire attendu |
|------------|-----------------|
| Windows | `runtime/python/windows/python.exe` |
| Linux | `runtime/python/linux/bin/python3` |
| macOS | `runtime/python/macos/bin/python3` |

Ce runtime contient au minimum :

- CPython exécutable
- stdlib nécessaire à `asyncio`, `json`, `ssl`
- le package `meross-iot` dans son `site-packages`

Le script `scripts/prepare_embedded_python.py` prépare automatiquement ce runtime pour Linux, macOS et Windows à partir de `astral-sh/python-build-standalone`, puis installe `meross-iot` dedans. Les workflows CI/release l'exécutent avant `cargo tauri build`, donc les bundles publiés incluent déjà ce runtime et l'utilisateur final n'a rien à installer.

## Pont Python (fallback / développement)

Le dépôt fournit `scripts/meross_mss315_bridge.py`, qui s’appuie sur la bibliothèque communautaire **`meross-iot`** (protocole cloud Meross — **pas** le firmware Meross).

```bash
pip install --user meross-iot
export MEROSS_EMAIL='vous@example.com'
export MEROSS_PASSWORD='***'
export MEROSS_REGION='eu'   # eu | us | ap
python3 scripts/meross_mss315_bridge.py --out ~/.config/soulkernel/meross_power.json
```

Lance-le en tâche de fond ou via systemd / Planificateur Windows. Adapte l’URL régionale (`iotx-eu.meross.com`, etc.) selon ton compte.

Pour préparer manuellement le runtime embarqué avant un build local :

```bash
python scripts/prepare_embedded_python.py
```

**Sécurité** : ne commite jamais identifiants ni `meross.json` avec secrets ; utilise des variables d’environnement ou un fichier hors dépôt.

## MSS315ZF

Le modèle **MSS315ZF** est une variante matérielle ; côté API Meross il se comporte comme les autres prises énergie compatibles **Electricity** dans l’écosystème Meross. En cas d’échec du script, vérifie région cloud, MFA, et que l’appareil apparaît dans l’app Meross officielle.
