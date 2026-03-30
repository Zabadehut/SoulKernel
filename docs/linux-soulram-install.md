# Linux SoulRAM Provisioning

Sous Linux, SoulKernel peut piloter un vrai backend `zRAM`, mais une application GUI ne doit pas compter sur des privileges runtime pour creer seule `/dev/zram0`.

Le provisionnement propre se fait une fois cote systeme:

```bash
sudo ./scripts/install-linux-soulram.sh
```

Ce script installe:

- `/usr/local/lib/soulkernel/soulram-provision.sh`
- `/etc/systemd/system/soulkernel-soulram-provision.service`
- `/etc/udev/rules.d/99-soulkernel-zram.rules`

## Ce que fait le provisionnement

1. charge le module `zram` si necessaire
2. tente un `hot_add` si le noyau expose `zram-control`
3. cree `/dev/zram0` si `sysfs` expose deja les nombres majeur/minor
4. applique des permissions stables via `udev`

## Verification

Apres installation ou reboot:

```bash
ls -l /dev/zram0
cat /sys/class/block/zram0/dev
```

## Ligne rouge

- Linux: vrai backend `zRAM`
- Windows: backend equivalent `Memory Compression + WorkingSet Trim`
- macOS: backend equivalent `Compressed Memory + purge hints`

SoulRAM reste une meme intention produit, mais le mecanisme reel doit rester explicite par OS.
