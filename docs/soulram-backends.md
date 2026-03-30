# SoulRAM Backends

SoulRAM designe une intention produit unique: reduire la pression memoire sans promettre un mecanisme identique sur tous les OS.

## Backend reel par plateforme

- Linux: `Linux zRAM swap backend`
  Equivalence fonctionnelle: compression swap en RAM + soulagement memoire pilote par SoulKernel.
- Windows: `Windows Memory Compression + WorkingSet Trim`
  Equivalence fonctionnelle: compression memoire native Windows + trim prudent des working sets.
- macOS: `macOS Compressed Memory + purge hints`
  Equivalence fonctionnelle: compression memoire native macOS + hints prudents de purge cache.

## Ligne rouge

- Linux peut utiliser zRAM directement.
- Windows et macOS n'embarquent pas zRAM.
- SoulKernel doit donc afficher le backend reel de l'OS, pas laisser croire a un `zRAM partout`.

## Roadmap equivalente

### Linux

1. Consolider le backend zRAM quand `/sys/block/zram0` et les privileges sont disponibles.
2. Ajouter un mode de secours `zswap/quota` quand zRAM n'est pas disponible ou deja gere par la distribution.
3. Relier PSI memoire, zRAM/zswap et cgroup v2 pour des decisions plus fines par charge.

Provisionnement recommande:

```bash
sudo ./scripts/install-linux-soulram.sh
```

Voir aussi [linux-soulram-install.md](./linux-soulram-install.md).

### Windows

1. Stabiliser l'audit `Memory Compression + WorkingSet Trim`.
2. Distinguer plus finement trim global, trim cible et cooldowns dans les rapports.
3. Mesurer explicitement le cout/benefice du backend memoire Windows face au gain host.

### macOS

1. Assumer explicitement la compression memoire native du noyau.
2. Mieux suivre `memory pressure` et separer observation, purge cache et actions prudentes.
3. Exposer un backend macOS comparable aux autres OS sans promettre un zRAM inexistant.
