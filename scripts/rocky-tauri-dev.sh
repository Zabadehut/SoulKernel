#!/usr/bin/env bash
# Rocky 9 / RHEL 9 : lancer un shell (ou une commande) dans l’image Fedora Tauri,
# avec le dépôt monté — puis ex. `cargo tauri dev` pour la GUI + reload des fichiers `ui/`.
# À lancer uniquement sur l’hôte (pas depuis un shell déjà dans le conteneur).
set -euo pipefail

if [ -f /run/.containerenv ] || [ -f /.dockerenv ]; then
  echo "Ce script est pour l’hôte Rocky, pas pour l’intérieur du conteneur." >&2
  echo "Tu y es déjà : tape simplement « cargo tauri dev » (après rebuild de l’image si besoin)." >&2
  exit 1
fi

if ! command -v podman >/dev/null 2>&1; then
  echo "podman est introuvable sur cette machine." >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE="${TAURI_DEV_IMAGE:-soulkernel-tauri-dev}"

if [ -z "$(podman images -q "$IMAGE" 2>/dev/null || true)" ]; then
  echo "Image absente. Construire depuis la racine du dépôt :" >&2
  echo "  podman build -f scripts/Containerfile.fedora-tauri -t $IMAGE $ROOT" >&2
  exit 1
fi

# $HOME de l’utilisateur qui a la session graphique (correct si tu lances en sudo).
_graphical_home() {
  if [ -n "${SUDO_USER:-}" ] && command -v getent >/dev/null 2>&1; then
    getent passwd "$SUDO_USER" | cut -d: -f6
    return
  fi
  printf '%s' "$HOME"
}

HOST_HOME="$(_graphical_home)"
HUID="$(id -u)"
if [ "$HUID" -eq 0 ] && [ -n "${SUDO_USER:-}" ]; then
  HUID="$(id -u "$SUDO_USER" 2>/dev/null || echo 0)"
fi

podman_gui_args=()

# Auth X11 : sans ça, root dans le conteneur ne peut pas joindre le socket → gtk::init échoue.
xauth_src="${XAUTHORITY:-}"
if [ -z "$xauth_src" ] && [ -f "$HOST_HOME/.Xauthority" ]; then
  xauth_src="$HOST_HOME/.Xauthority"
fi
if [ -n "$xauth_src" ] && [ -f "$xauth_src" ]; then
  podman_gui_args+=(-v "$xauth_src:/root/.Xauthority:ro" -e XAUTHORITY=/root/.Xauthority)
else
  echo "Avertissement : pas de .Xauthority utilisable (essayé : \${XAUTHORITY} et $HOST_HOME/.Xauthority)." >&2
  echo "  GTK risque d’échouer : connecte-toi en session graphique ou exporte XAUTHORITY." >&2
fi

# Affichage : depuis une session Wayland, XWayland expose souvent DISPLAY=:0 ; on garde la valeur hôte.
if [ -z "${DISPLAY:-}" ]; then
  podman_gui_args+=(-e DISPLAY=:0)
else
  podman_gui_args+=(-e "DISPLAY=$DISPLAY")
fi

# Évite que GTK tente Wayland dans le conteneur (souvent non monté) alors que seul X11 est dispo via le socket.
podman_gui_args+=(-e GDK_BACKEND=x11)

if [ -n "${WAYLAND_DISPLAY:-}" ]; then
  podman_gui_args+=(-e "WAYLAND_DISPLAY=$WAYLAND_DISPLAY")
fi

# D-Bus session (thèmes, portails) — optionnel mais utile sur GNOME / KDE.
if [ -n "$HUID" ] && [ "$HUID" != "0" ] && [ -S "/run/user/$HUID/bus" ]; then
  podman_gui_args+=(-v "/run/user/$HUID/bus:/run/user/$HUID/bus:ro")
  podman_gui_args+=(-e "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$HUID/bus")
fi

# SELinux (Rocky) peut bloquer l’accès au socket X11 depuis le conteneur. Déblocage dev uniquement :
#   TAURI_PODMAN_LABEL_DISABLE=1 ./scripts/rocky-tauri-dev.sh
if [ "${TAURI_PODMAN_LABEL_DISABLE:-}" = 1 ]; then
  podman_gui_args+=(--security-opt label=disable)
fi

if command -v xhost >/dev/null 2>&1; then
  xhost +local: 2>/dev/null || true
fi

exec podman run -it --rm --network host \
  "${podman_gui_args[@]}" \
  -v /tmp/.X11-unix:/tmp/.X11-unix \
  -v "$ROOT:/work:Z" \
  -w /work \
  "$IMAGE" \
  "${@:-bash}"
