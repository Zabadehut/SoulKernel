'use strict';
/**
 * MemLab scenario — SoulKernel live-dashboard (Node.js, port 8787)
 *
 * Règle MemLab : back() NE DOIT PAS appeler page.goto() ni recharger la page.
 * Pour un dashboard SPA sans routing interne, back() remet l'état initial
 * via interactions UI (fermeture de filtres actifs, scroll en haut).
 */

const BASE_URL = process.env.SOULKERNEL_DASHBOARD_URL ?? 'http://localhost:8787';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** @type {import('@memlab/core').IScenario} */
const scenario = {
  url() {
    return BASE_URL;
  },

  // ── s2 : interaction — stress cycles de polling + renderer ───────────────
  async action(page) {
    // Premier cycle de fetch /api/timeline (~3 s par cycle)
    await sleep(2000);
    await sleep(6000);

    // Clics sur les 3 premiers boutons (filtre, zoom…)
    const buttons = await page.$$('button');
    for (const btn of buttons.slice(0, 3)) {
      try {
        await btn.click();
        await sleep(400);
      } catch { /* bouton absent ou non cliquable */ }
    }

    // Un dernier cycle pour laisser le GC potentiel agir
    await sleep(3000);
  },

  // ── s3 : retour à l'état initial SANS recharger la page ──────────────────
  // MemLab interdit page.goto() ici. On remet l'état via UI :
  //  1. Ré-appuie sur les mêmes boutons pour les désactiver
  //  2. Scroll en haut de page
  //  3. Attend que le polling repasse en régime établi
  async back(page) {
    // Désactive les filtres ouverts (re-clic = toggle)
    const buttons = await page.$$('button');
    for (const btn of buttons.slice(0, 3)) {
      try {
        await btn.click();
        await sleep(200);
      } catch { /* ignore */ }
    }

    // Scroll en haut — remet la vue à l'état "chargement"
    await page.evaluate(() => window.scrollTo(0, 0));
    await sleep(2000);
  },

  leakFilter(node) {
    const name = node.name ?? '';
    if (name === 'Array' && node.retainedSize > 5 * 1024 * 1024) return true;
    if (name.includes('Detached')) return true;
    return false;
  },
};

module.exports = scenario;
