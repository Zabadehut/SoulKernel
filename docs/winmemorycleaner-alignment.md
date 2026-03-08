# WinMemoryCleaner Alignment (SoulKernel)

This note maps WinMemoryCleaner-style memory cleanup ideas to SoulKernel, then shows how they are applied across OSes.

## 1) Comparison

WinMemoryCleaner (concept level) focuses on:
- memory compression toggling (MMAgent)
- working-set trimming
- reclaiming cached pages/standby memory

SoulKernel now uses a similar strategy family without copying code:
- Windows: MMAgent + global working-set trim
- Linux: zRAM + swap/zRAM orchestration + cache hints
- macOS: kernel compressed memory + purge hint

## 2) Best-practice adoption in Windows backend

Implemented:
- admin relaunch prompt at app startup (UAC)
- explicit admin gating for MMAgent commands
- clearer failure messaging when memory compression enable fails (including restart-pending context)

## 3) Formula impact on all OSes

A cross-platform `memory_optimizer_factor()` is now injected in metrics.
It attenuates memory/compression contention in epsilon:

- mem epsilon scale: `1 - 0.35 * factor`
- compression epsilon scale: `1 - 0.25 * factor`

This means pi and dome gain D now reflect OS-specific memory reclaim capability.
