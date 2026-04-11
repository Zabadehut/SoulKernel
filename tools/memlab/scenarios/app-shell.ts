/**
 * MemLab scenario — SoulKernel Svelte shell (Vite dev server, port 1420)
 *
 * Détecte :
 *  - Stores Svelte non unsubscribés (on_destroy oublié)
 *  - Listeners Tauri/CustomEvent non retirés entre navigations
 *  - Accumulation dans les reactive arrays (kpi_history, last_actions)
 *
 * Pré-requis :
 *   cd ui && npm run dev   (Vite écoute sur 1420 par défaut)
 *   cd tools/memlab && npm run check:app
 *
 * Note : en mode Tauri natif, le WebView n'est pas accessible depuis l'extérieur.
 * Ce scénario cible la version web du shell (npm run dev).
 */

import type { IScenario, Page } from "@memlab/core";

const BASE_URL = process.env.SOULKERNEL_APP_URL ?? "http://localhost:1420";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const scenario: IScenario = {
  url(): string {
    return BASE_URL;
  },

  async action(page: Page): Promise<void> {
    // Attente du rendu initial complet (App.svelte + SoulKernelShell)
    await sleep(3000);

    // Navigation entre les panneaux si des onglets/boutons existent
    const tabs = await page.$$("[data-panel], .panel-tab, nav a, button[data-nav]");
    for (const tab of tabs.slice(0, 5)) {
      try {
        await tab.click();
        await sleep(600);
      } catch { /* ignore */ }
    }

    // Déclenche un refresh manuel si disponible
    const refreshBtn = await page.$("button[data-action='refresh'], .refresh-btn");
    if (refreshBtn) {
      await refreshBtn.click();
      await sleep(1000);
    }

    // Laisse tourner les Svelte stores + setInterval plusieurs cycles (5 s de refresh)
    await sleep(10000);
  },

  async back(page: Page): Promise<void> {
    await page.goto(BASE_URL, { waitUntil: "networkidle2" });
    await sleep(2000);
  },

  leakFilter(node, _snapshot, _leakCluster): boolean {
    const name = node.name ?? "";

    // Grandes fonctions callback non libérées (store subscribers Svelte)
    if (name === "Function" && node.retainedSize > 512 * 1024) return true;

    // Listeners Tauri / CustomEvent orphelins
    if (name.includes("EventListener") || name.includes("TauriListener")) return true;

    // Tableaux croissants — kpi_history, last_actions, process_report
    if (name === "Array" && node.retainedSize > 2 * 1024 * 1024) return true;

    // Composants Svelte détruits mais non collectés
    if (name.includes("Detached HTMLDivElement") || name.includes("Detached HTMLCanvasElement")) {
      return true;
    }

    return false;
  },
};

export default scenario;
