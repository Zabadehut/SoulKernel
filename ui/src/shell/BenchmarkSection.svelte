<section class="benchmark-view" id="benchmarkView">
  <div class="benchmark-shell">
    <div class="benchmark-column">
      <div class="advisor-panel" id="benchPanel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico"><i data-lucide="terminal"></i></span><span>A/B dôme — sonde OS ou commande</span>
        </div>
        <label class="kappa-name" for="kpiCommand" style="display:block;margin-top:.2rem">Sonde</label>
        <input
          id="kpiCommand"
          class="target-select"
          type="text"
          value="system"
          placeholder="system | chemin exécutable"
          title="system = observation OS intégrée (sans processus externe)"
        />
        <label class="kappa-name" for="kpiArgs" style="display:block;margin-top:.35rem">Args</label>
        <input
          id="kpiArgs"
          class="target-select"
          type="text"
          value="4000"
          placeholder="Durée ms (system) ou args commande"
          title="Avec system : durée d’observation en ms (500–120000)"
          style="margin-top:.15rem"
        />
        <div class="target-hint" style="margin-top:.35rem;font-size:.65rem;line-height:1.45;color:var(--muted)">
          <strong>system</strong> : SoulKernel échantillonne CPU, RAM, I/O disque (agrégat), σ, etc. pendant la durée
          indiquée — c’est le mode aligné sur « tester l’OS » sans lancer Rust/cargo. Une <strong>commande externe</strong>
          sert seulement si tu veux une charge reproductible (compilation, script) en plus du réglage dôme.
        </div>
        <div class="target-hint" style="margin-top:.25rem;font-size:.65rem;line-height:1.45;color:var(--warning)">
          Si tu lances <strong>cargo check / build</strong> sur <em>ce</em> dépôt pendant
          <code>cargo tauri dev</code>, le watcher peut redémarrer l’app.
        </div>
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
        <div id="kpiBenchProgressWrap" class="kpi-bench-progress" hidden aria-live="polite">
          <div class="kpi-bench-progress-head">
            <span class="kpi-bench-pulse" aria-hidden="true"></span>
            <span id="kpiBenchProgressTitle" class="kpi-bench-title">Benchmark en attente</span>
          </div>
          <div class="kpi-bench-bar-wrap">
            <div id="kpiBenchProgressBar" class="kpi-bench-bar-fill"></div>
          </div>
          <div id="kpiBenchProgressSub" class="kpi-bench-sub"></div>
          <pre id="kpiBenchProgressLog" class="kpi-bench-log"></pre>
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
        <div class="advisor-text compactable-copy" style="margin-top:.35rem;font-size:.72rem;line-height:1.4;color:var(--muted)">
          <strong>Protocole</strong> : alternance dôme OFF / ON (× runs), stabilisation, puis <em>sonde</em> —
          observation OS (<code>system</code>) ou exécution d’une commande. Les métriques affichées sont lues
          <em>après</em> chaque fenêtre de sonde.<br />
          <strong>Verdict</strong> : durée de la fenêtre (médiane / p95), puis médianes OFF vs ON de RAM, % CPU, % GPU, W,
          σ, températures si capteurs. « Cache » et réseau ne sont pas des séries agrégées dans le résumé actuel ; l’I/O
          disque apparaît dans la sonde <code>system</code> (texte) et partiellement via les métriques après sonde.
          Le score d’efficacité combine surtout temps + ressources (sans températures dans la formule).
        </div>
      </div>
      <div class="advisor-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico"><i data-lucide="database"></i></span><span>Base appliquee a l'application</span>
        </div>
        <div class="advisor-text">
          Le meilleur benchmark connu devient la base par défaut des réglages. La recommandation live ajuste ensuite
          autour de cette base selon la charge réelle.
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
          Sauvegarde locale automatique. L’économie réelle s’affiche seulement si la puissance (W) est disponible.
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
