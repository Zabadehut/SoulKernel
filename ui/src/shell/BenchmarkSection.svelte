<section class="benchmark-view" id="benchmarkView">
  <div class="benchmark-shell">
    <div class="benchmark-column">
      <div class="advisor-panel" id="benchPanel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico"><i data-lucide="terminal"></i></span><span>KPI externe A/B</span>
        </div>
        <input id="kpiCommand" class="target-select" type="text" value="cargo" placeholder="Commande KPI" />
        <input
          id="kpiArgs"
          class="target-select"
          type="text"
          value="check"
          placeholder="Arguments KPI"
          style="margin-top:.25rem"
        />
        <div class="kappa-row" style="margin-top:.35rem;gap:.5rem">
          <span class="kappa-name">Runs / etat</span>
          <input
            id="kpiRuns"
            class="target-select"
            type="number"
            min="5"
            max="20"
            value="5"
            style="width:74px;padding:.2rem .35rem"
          />
        </div>
        <div class="gains-actions" style="margin-top:.35rem">
          <button type="button" class="ctrl-btn btn-secondary" id="btnRunAB" style="font-size:.62rem;padding:.25rem .45rem"
            ><span class="btn-ico"><i data-lucide="play"></i></span> Run A/B</button
          >
          <button type="button" class="ctrl-btn btn-secondary" id="btnExportAB" style="font-size:.62rem;padding:.25rem .45rem"
            ><span class="btn-ico"><i data-lucide="download"></i></span> Exporter A/B</button
          >
          <button type="button" class="ctrl-btn btn-danger" id="btnClearAB" style="font-size:.62rem;padding:.25rem .45rem"
            ><span class="btn-ico"><i data-lucide="trash-2"></i></span> Reset A/B</button
          >
        </div>
        <div class="advisor-text" id="kpiBenchSummary" style="margin-top:.35rem">Aucun benchmark A/B</div>
        <div class="advisor-text" id="kpiBenchLearned" style="margin-top:.35rem">
          Apprentissage benchmark: aucun historique pertinent
        </div>
        <div class="advisor-text" style="margin-top:.35rem;font-size:.72rem;line-height:1.4;color:var(--muted)">
          Sous Windows, une sonde KPI lançait autrefois un processus console <strong>visible</strong> (pas de
          <code>CREATE_NO_WINDOW</code>) : chaque run pouvait faire clignoter une fenêtre. C’est corrigé côté moteur. Le
          résumé A/B ajoute des <strong>écarts médians après sonde</strong> sur RAM, GPU % et CPU % (positif = ressource
          moins chargée en phase ON) — ce sont des indicateurs complémentaires au temps wall-clock, avec du bruit
          possible.
        </div>
      </div>
      <div class="advisor-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico"><i data-lucide="database"></i></span><span>Base appliquee a l'application</span>
        </div>
        <div class="advisor-text">
          Le meilleur benchmark connu devient la base par defaut des reglages de l'application pour ce workload et cette
          commande KPI. La recommandation live ne part plus de zero: elle ajuste autour de cette base selon la charge
          reelle.
        </div>
      </div>
    </div>
    <div class="benchmark-column">
      <div class="advisor-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico" style="color:var(--warning)"><i data-lucide="zap"></i></span><span>Energie benchmark</span>
        </div>
        <div class="kappa-row" style="margin-top:.1rem;gap:.4rem"
          ><span class="kappa-name">Prix electricite (EUR/kWh)</span><input
            id="energyPrice"
            class="target-select"
            type="text"
            inputmode="decimal"
            value="0.22"
            placeholder="0,194"
            title="Accepte 0,194 ou 0.194"
            style="width:110px;padding:.2rem .35rem"
          /></div
        >
        <div class="kappa-row" style="margin-top:.25rem;gap:.4rem"
          ><span class="kappa-name">CO2 (kg/kWh)</span><input
            id="energyCo2"
            class="target-select"
            type="text"
            inputmode="decimal"
            value="0.05"
            placeholder="0,024"
            title="Accepte 0,024 ou 0.024"
            style="width:110px;padding:.2rem .35rem"
          /></div
        >
        <button type="button" class="ctrl-btn btn-secondary" id="btnApplyEnergyPricing" style="font-size:.62rem;padding:.25rem .45rem;margin-top:.3rem"
          ><span class="btn-ico"><i data-lucide="coins"></i></span> Appliquer tarif energie</button
        >
        <div class="advisor-text" id="energyPricingStatus" style="margin-top:.3rem">Tarif actif: chargement...</div>
        <div class="target-hint" style="margin-top:.2rem">
          Sauvegarde locale automatique (pricing.json). L'economie reelle s'affiche seulement si la puissance (W) est
          disponible.
        </div>
      </div>
      <div class="advisor-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico"><i data-lucide="scale"></i></span><span>Verdict benchmark</span>
        </div>
        <div class="advisor-text" id="benchmarkVerdict">Aucun benchmark charge.</div>
      </div>
      <div class="advisor-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico" style="color:var(--warning)"><i data-lucide="trophy"></i></span><span>Top benchmark</span>
        </div>
        <div class="bench-top-list" id="benchmarkTopList">
          <div class="advisor-text">Aucun classement disponible.</div>
        </div>
      </div>
    </div>
  </div>
</section>
