#!/usr/bin/env python3
"""
Prépare un runtime Python embarqué minimal pour la feature Meross.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import stat
import subprocess
import tarfile
import tempfile
import urllib.request
from pathlib import Path


DEFAULT_PYTHON_SERIES = "3.12"
DEFAULT_MEROSS_SPEC = "meross-iot>=0.4.10.4,<0.5"
GITHUB_API_LATEST = "https://api.github.com/repos/astral-sh/python-build-standalone/releases/latest"

TARGETS = {
    "x86_64-pc-windows-msvc": {"platform_dir": "windows", "python_rel": "python.exe"},
    "x86_64-unknown-linux-gnu": {"platform_dir": "linux", "python_rel": "bin/python3"},
    "aarch64-apple-darwin": {"platform_dir": "macos", "python_rel": "bin/python3"},
    "x86_64-apple-darwin": {"platform_dir": "macos", "python_rel": "bin/python3"},
}


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def detect_default_target() -> str:
    sys_name = platform.system().lower()
    machine = platform.machine().lower()
    if sys_name == "windows":
        return "x86_64-pc-windows-msvc"
    if sys_name == "linux":
        return "x86_64-unknown-linux-gnu"
    if sys_name == "darwin":
        return "aarch64-apple-darwin" if machine in {"arm64", "aarch64"} else "x86_64-apple-darwin"
    raise SystemExit(f"OS non supporté: {platform.system()}")


def fetch_json(url: str) -> dict:
    req = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": "SoulKernel-embedded-python-prep",
        },
    )
    with urllib.request.urlopen(req) as resp:
        return json.load(resp)


def select_asset(release: dict, target: str, python_series: str) -> dict:
    assets = release.get("assets", [])
    suffixes = (
        f"-{target}-install_only_stripped.tar.gz",
        f"-{target}-install_only.tar.gz",
    )
    prefixes = (f"cpython-{python_series}.", f"cpython-{python_series}+")
    for suffix in suffixes:
        for asset in assets:
            name = asset.get("name", "")
            if name.endswith(suffix) and name.startswith(prefixes):
                return asset
    raise SystemExit(
        f"Aucun runtime python-build-standalone trouvé pour target={target} et série={python_series}"
    )


def download(url: str, dest: Path) -> None:
    req = urllib.request.Request(url, headers={"User-Agent": "SoulKernel-embedded-python-prep"})
    with urllib.request.urlopen(req) as resp, dest.open("wb") as fh:
        shutil.copyfileobj(resp, fh)


def safe_rmtree(path: Path) -> None:
    if not path.exists():
        return

    def onerror(func, p, _exc_info):
        try:
            os.chmod(p, stat.S_IWRITE)
        except OSError:
            pass
        func(p)

    shutil.rmtree(path, onerror=onerror)


def flatten_single_root(src_dir: Path, dest_dir: Path) -> None:
    children = list(src_dir.iterdir())
    root = children[0] if len(children) == 1 and children[0].is_dir() else src_dir
    safe_rmtree(dest_dir)
    dest_dir.mkdir(parents=True, exist_ok=True)
    for item in root.iterdir():
        shutil.move(str(item), dest_dir / item.name)


def extract_archive(archive: Path, dest_dir: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="soulkernel-py-extract-") as tmp:
        tmpdir = Path(tmp)
        with tarfile.open(archive, "r:gz") as tf:
            tf.extractall(tmpdir)
        flatten_single_root(tmpdir, dest_dir)


def embedded_python_path(dest_dir: Path, target: str) -> Path:
    return dest_dir / TARGETS[target]["python_rel"]


def run(cmd: list[str], env: dict[str, str] | None = None) -> None:
    print("+", " ".join(str(part) for part in cmd))
    subprocess.run(cmd, check=True, env=env)


def ensure_pip(python_bin: Path) -> None:
    try:
        run([str(python_bin), "-m", "pip", "--version"])
    except subprocess.CalledProcessError:
        run([str(python_bin), "-m", "ensurepip", "--upgrade"])


def install_meross(python_bin: Path, package_spec: str) -> None:
    env = dict(os.environ)
    env.setdefault("PIP_DISABLE_PIP_VERSION_CHECK", "1")
    run([str(python_bin), "-m", "pip", "install", "--upgrade", "pip"], env=env)
    run(
        [str(python_bin), "-m", "pip", "install", "--no-cache-dir", "--upgrade", package_spec],
        env=env,
    )
    # Évite le résolveur c-ares dans le runtime embarqué Windows: aiohttp retombera
    # ainsi sur le résolveur système threadé, plus robuste en bundle portable.
    run(
        [str(python_bin), "-m", "pip", "uninstall", "-y", "aiodns", "pycares"],
        env=env,
    )


def prune_runtime(dest_dir: Path, target: str) -> None:
    if TARGETS[target]["platform_dir"] == "windows":
        roots = [dest_dir / "Lib", dest_dir / "Lib" / "site-packages"]
    else:
        roots = []
        for version_dir in (dest_dir / "lib").glob("python*"):
            roots.extend([version_dir, version_dir / "site-packages"])

    removable_prefixes = ("pip-", "setuptools-", "wheel-")
    removable_exact = {
        "ensurepip",
        "idlelib",
        "tkinter",
        "turtledemo",
        "test",
        "tests",
        "pip",
        "setuptools",
        "wheel",
        "pkg_resources",
    }

    for root in roots:
        if not root.exists():
            continue
        for child in root.iterdir():
            if child.name in removable_exact or child.name.startswith(removable_prefixes):
                safe_rmtree(child) if child.is_dir() else child.unlink(missing_ok=True)
        for cache in root.rglob("__pycache__"):
            safe_rmtree(cache)


def write_metadata(dest_dir: Path, release_tag: str, asset_name: str, asset_url: str, package_spec: str) -> None:
    meta = {
        "release_tag": release_tag,
        "asset_name": asset_name,
        "asset_url": asset_url,
        "packages": [package_spec],
    }
    (dest_dir / ".soulkernel-python.json").write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")


def runtime_ready(python_bin: Path) -> bool:
    if not python_bin.exists():
        return False
    try:
        subprocess.run(
            [str(python_bin), "-c", "import meross_iot; print('ok')"],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return True
    except Exception:
        return False


def main() -> None:
    parser = argparse.ArgumentParser(description="Prépare le runtime Python embarqué SoulKernel")
    parser.add_argument(
        "--target",
        default=os.environ.get("SOULKERNEL_EMBEDDED_PYTHON_TARGET") or detect_default_target(),
    )
    parser.add_argument(
        "--python-series",
        default=os.environ.get("SOULKERNEL_EMBEDDED_PYTHON_SERIES", DEFAULT_PYTHON_SERIES),
    )
    parser.add_argument(
        "--package-spec",
        default=os.environ.get("SOULKERNEL_MEROSS_PIP_SPEC", DEFAULT_MEROSS_SPEC),
    )
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()

    if args.target not in TARGETS:
        raise SystemExit(f"Target non supporté: {args.target}")

    dest_dir = repo_root() / "runtime" / "python" / TARGETS[args.target]["platform_dir"]
    python_bin = embedded_python_path(dest_dir, args.target)

    if runtime_ready(python_bin) and not args.force:
        print(f"Runtime déjà prêt: {python_bin}")
        return

    release = fetch_json(GITHUB_API_LATEST)
    asset = select_asset(release, args.target, args.python_series)
    asset_name = asset["name"]
    asset_url = asset["browser_download_url"]

    with tempfile.TemporaryDirectory(prefix="soulkernel-py-download-") as tmp:
        archive = Path(tmp) / asset_name
        print(f"Téléchargement: {asset_name}")
        download(asset_url, archive)
        extract_archive(archive, dest_dir)

    python_bin = embedded_python_path(dest_dir, args.target)
    if not python_bin.exists():
        raise SystemExit(f"Interpréteur embarqué introuvable après extraction: {python_bin}")

    ensure_pip(python_bin)
    install_meross(python_bin, args.package_spec)
    prune_runtime(dest_dir, args.target)
    write_metadata(
        dest_dir,
        release.get("tag_name", ""),
        asset_name,
        asset_url,
        args.package_spec,
    )
    print(f"Runtime Python prêt: {python_bin}")


if __name__ == "__main__":
    main()
