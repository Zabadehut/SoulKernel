'use strict';
/**
 * MemLab scenario — SoulKernel Svelte shell (Vite dev server, port 1420)
 *
 * Pré-requis : cd ui && npm run dev
 * Si le serveur n'est pas lancé, le scénario échoue avec un message clair.
 *
 * Règle MemLab : back() NE DOIT PAS appeler page.goto() ni recharger la page.
 */

const BASE_URL = process.env.SOULKERNEL_APP_URL ?? 'http://localhost:1420';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** @type {import('@memlab/core').IScenario} */
const scenario = {
  url() {
    return BASE_URL;
  },

  // ── s2 : interaction ─────────────────────────────────────────────────────
  async action(page) {
    // Vérifie qu'on n'est pas sur la page "server-unavailable"
    const bodyText = await page.evaluate(() => document.body.innerText ?? '');
    if (bodyText.includes('server-unavailable')) {
      await sleep(500);
      return;
    }

    // Attente du rendu Svelte complet
    await sleep(3000);

    // Navigation entre panneaux via onglets / boutons de nav
    const tabs = await page.$$('[data-panel], .panel-tab, nav a, button[data-nav]');
    for (const tab of tabs.slice(0, 5)) {
      try {
        await tab.click();
        await sleep(600);
      } catch { /* ignore */ }
    }

    // Refresh manuel si disponible
    const refreshBtn = await page.$('button[data-action="refresh"], .refresh-btn');
    if (refreshBtn) {
      await refreshBtn.click();
      await sleep(1000);
    }

    // Cycles Svelte stores + setInterval
    await sleep(8000);
  },

  // ── s3 : retour SANS page.goto() ────────────────────────────────────────
  async back(page) {
    // Ferme les panneaux ouverts (clic sur le premier onglet = onglet "home")
    const firstTab = await page.$('[data-panel]:first-child, nav a:first-child');
    if (firstTab) {
      try {
        await firstTab.click();
        await sleep(500);
      } catch { /* ignore */ }
    }

    // Scroll en haut
    await page.evaluate(() => window.scrollTo(0, 0));
    await sleep(1500);
  },

  leakFilter(node) {
    const name = node.name ?? '';
    if (name === 'Function' && node.retainedSize > 512 * 1024) return true;
    if (name.includes('EventListener') || name.includes('TauriListener')) return true;
    if (name === 'Array' && node.retainedSize > 2 * 1024 * 1024) return true;
    if (name.includes('Detached HTML')) return true;
    return false;
  },
};

module.exports = scenario;
