Ce dossier est destiné au runtime Python embarqué pour la feature Meross.

Layout attendu :

- Windows : `runtime/python/windows/python.exe`
- Linux : `runtime/python/linux/bin/python3`
- macOS : `runtime/python/macos/bin/python3`

Le runtime doit inclure `meross-iot` dans son environnement packagé.

SoulKernel préfère automatiquement ce runtime embarqué avant `python_bin`, `python3`, `python` et `py`.

Préparation automatique :

```bash
python scripts/prepare_embedded_python.py
```

Le script télécharge un runtime portable `python-build-standalone`, l'installe ici, ajoute `meross-iot`, puis nettoie les composants inutiles au runtime final.
