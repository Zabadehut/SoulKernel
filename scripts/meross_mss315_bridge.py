#!/usr/bin/env python3
"""
Pont optionnel Meross -> fichier JSON pour SoulKernel (puissance murale).

Dépendance : pip install --user 'meross-iot>=0.4.10.4'

Variables d'environnement :
  MEROSS_EMAIL, MEROSS_PASSWORD
  MEROSS_REGION   (eu | us | ap, défaut eu)
  MEROSS_OUT      (chemin sortie JSON)
  MEROSS_DEVICE_TYPE (optionnel, ex. mss315)
  MEROSS_HTTP_PROXY  (optionnel, ex. http://proxy:8080)
  MEROSS_MFA_CODE    (optionnel)
  MEROSS_CREDS_CACHE (optionnel, json sérialisé MerossCloudCreds)

Activez la fusion côté SoulKernel : ~/.config/soulkernel/meross.json
  {"enabled": true}

Référence matériel : MSS315ZF (prise Meross).
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time


def default_out() -> str:
    if sys.platform == "win32":
        base = os.environ.get("APPDATA", "")
        if base:
            return os.path.join(base, "SoulKernel", "meross_power.json")
    home = os.environ.get("HOME") or ""
    return os.path.join(home, ".config", "soulkernel", "meross_power.json")


def default_creds_cache() -> str:
    if sys.platform == "win32":
        base = os.environ.get("APPDATA", "")
        if base:
            return os.path.join(base, "SoulKernel", "meross_cloud_creds.json")
    home = os.environ.get("HOME") or ""
    return os.path.join(home, ".config", "soulkernel", "meross_cloud_creds.json")


def _api_base(region: str) -> str:
    r = region.lower().strip()
    return {
        "eu": "https://iotx-eu.meross.com",
        "us": "https://iotx-us.meross.com",
        "ap": "https://iotx-ap.meross.com",
    }.get(r, "https://iotx-eu.meross.com")


def _pick_plug(manager, preferred: str | None):
    if preferred:
        found = manager.find_devices(device_type=preferred.lower().strip())
        if found:
            return found[0]
    for dtype in ("mss315", "mss315zf", "mss310", "mss305", "mss210"):
        found = manager.find_devices(device_type=dtype)
        if found:
            return found[0]
    raise RuntimeError(
        "Aucune prise Meross reconnue — indique --device-type ou MEROSS_DEVICE_TYPE."
    )


async def poll_once(manager, dev, out_path: str) -> dict:
    await dev.async_update()
    metrics = await dev.async_get_instant_metrics(channel=0)
    w = float(metrics.power)
    payload = {"watts": w, "ts_ms": int(time.time() * 1000)}
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(payload, f)
    return payload


async def run_session(out_path: str, once: bool, interval: float, device_type: str | None) -> None:
    from meross_iot.http_api import MerossHttpClient
    from meross_iot.manager import MerossManager
    from meross_iot.model.credentials import MerossCloudCreds

    email = os.environ.get("MEROSS_EMAIL", "").strip()
    password = os.environ.get("MEROSS_PASSWORD", "")
    if not email or not password:
        raise RuntimeError("MEROSS_EMAIL et MEROSS_PASSWORD sont requis.")

    region = (os.environ.get("MEROSS_REGION") or "eu").strip()
    http_proxy = (os.environ.get("MEROSS_HTTP_PROXY") or "").strip() or None
    mfa_code = (os.environ.get("MEROSS_MFA_CODE") or "").strip() or None
    creds_cache_path = (os.environ.get("MEROSS_CREDS_CACHE") or default_creds_cache()).strip()
    http_client = None

    if creds_cache_path and os.path.exists(creds_cache_path):
        try:
            with open(creds_cache_path, "r", encoding="utf-8") as f:
                cached = MerossCloudCreds.from_json(f.read())
            http_client = await MerossHttpClient.async_from_cloud_creds(
                creds=cached,
                http_proxy=http_proxy,
            )
        except Exception as e:
            print(f"meross cached creds invalid: {e}", file=sys.stderr)

    if http_client is None:
        http_client = await MerossHttpClient.async_from_user_password(
            api_base_url=_api_base(region),
            email=email,
            password=password,
            http_proxy=http_proxy,
            mfa_code=mfa_code,
        )
        if creds_cache_path:
            os.makedirs(os.path.dirname(creds_cache_path) or ".", exist_ok=True)
            with open(creds_cache_path, "w", encoding="utf-8") as f:
                f.write(http_client.cloud_credentials.to_json())
    manager = MerossManager(http_client=http_client)
    await manager.async_init()
    await manager.async_device_discovery()
    dev = _pick_plug(manager, device_type or os.environ.get("MEROSS_DEVICE_TYPE"))

    try:
        while True:
            try:
                data = await poll_once(manager, dev, out_path)
                print(json.dumps(data), flush=True)
            except Exception as e:
                print(f"meross bridge error: {e}", file=sys.stderr)
            if once:
                break
            await asyncio.sleep(max(2.0, interval))
    finally:
        manager.close()
        await http_client.async_logout()


def main() -> None:
    p = argparse.ArgumentParser(description="Meross -> JSON watts pour SoulKernel")
    p.add_argument("--out", default=os.environ.get("MEROSS_OUT") or default_out())
    p.add_argument("--once", action="store_true")
    p.add_argument("--interval", type=float, default=8.0)
    p.add_argument("--device-type", default=os.environ.get("MEROSS_DEVICE_TYPE"))
    p.add_argument("--http-proxy", default=os.environ.get("MEROSS_HTTP_PROXY"))
    p.add_argument("--mfa-code", default=os.environ.get("MEROSS_MFA_CODE"))
    p.add_argument("--creds-cache", default=os.environ.get("MEROSS_CREDS_CACHE") or default_creds_cache())
    args = p.parse_args()

    if args.http_proxy:
        os.environ["MEROSS_HTTP_PROXY"] = args.http_proxy
    if args.mfa_code:
        os.environ["MEROSS_MFA_CODE"] = args.mfa_code
    if args.creds_cache:
        os.environ["MEROSS_CREDS_CACHE"] = args.creds_cache

    if sys.platform == "win32":
        asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())

    asyncio.run(run_session(args.out, args.once, args.interval, args.device_type))


if __name__ == "__main__":
    main()
