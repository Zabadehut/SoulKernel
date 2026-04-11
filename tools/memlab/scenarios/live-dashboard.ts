/**
 * MemLab scenario — SoulKernel live-dashboard (Node.js, port 8787)
 *
 * Détecte :
 *  - Fuites dans les setInterval / setTimeout non nettoyés (polling)
 *  - Accumulation de samples dans les arrays Chart.js (timeline)
 *  - Références détachées suite aux mises à jour du DOM via innerHTML
 *
 * Pré-requis :
 *   cd tools/live-dashboard && node server.mjs &
 *   cd tools/memlab && npm run check:dashboard
 */

import type { IScenario, Page } from "@memlab/core";

const BASE_URL = process.env.SOULKERNEL_DASHBOARD_URL ?? "http://localhost:8787";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const scenario: IScenario = {
  // ── 1. Page de départ ────────────────────────────────────────────────────
  url(): string {
    return BASE_URL;
  },

  // ── 2. Interaction : stress les cycles de polling ─────────────────────────
  async action(page: Page): Promise<void> {
    // Initialisation charts + premier cycle de fetch /api/timeline
    await sleep(2000);

    // Plusieurs cycles de polling (3 s par cycle)
    await sleep(9000);

    // Clique sur les 3 premiers boutons pour stresser le re-render
    const buttons = await page.$$("button");
    for (const btn of buttons.slice(0, 3)) {
      try {
        await btn.click();
        await sleep(400);
      } catch { /* bouton absent ou non cliquable */ }
    }
  },

  // ── 3. Retour à l'état initial ────────────────────────────────────────────
  async back(page: Page): Promise<void> {
    await page.goto(BASE_URL, { waitUntil: "networkidle2" });
    await sleep(1500);
  },

  // ── 4. Filtre de fuite ───────────────────────────────────────────────────
  leakFilter(node, _snapshot, _leakCluster): boolean {
    const name = node.name ?? "";

    // Tableau > 5 MiB — probable accumulation de samples non bornée
    if (name === "Array" && node.retainedSize > 5 * 1024 * 1024) return true;

    // Nœud DOM détaché
    if (name.includes("Detached")) return true;

    return false;
  },
};

export default scenario;
