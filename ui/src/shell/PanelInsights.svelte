<div class="panel panel-insights">
  <div class="panel-title">
    <span class="pt-ico"><i data-lucide="shield-check"></i></span><span>PREUVES · SESSION · JOURNAL</span>
  </div>
  <div class="insights-shell">
    <div class="proof-trust-card">
      <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
        <span class="pt-ico"><i data-lucide="badge-check"></i></span><span>Ce que vous pouvez défendre devant un tiers</span>
      </div>
      <div class="advisor-text" style="color:var(--text)">
        <strong>1 — Mesures natives</strong> : CPU, RAM, GPU global machine, I/O, parfois W.<br />
        <strong>2 — Rejouable</strong> : Benchmark A/B sur votre commande KPI.<br />
        <strong>3 — Auditable</strong> : export JSON, snapshots, chemins de trace, et détail processus observé/estimé.
      </div>
      <div class="gains-actions" style="margin-top:.4rem">
        <button type="button" class="ctrl-btn btn-primary" id="btnCopyEvidence" style="font-size:.62rem;padding:.28rem .5rem"
          ><span class="btn-ico"><i data-lucide="clipboard-copy"></i></span> Copier preuve (méthode + chemins)</button
        >
        <button type="button" class="ctrl-btn btn-secondary" id="btnCopyGains" style="font-size:.62rem;padding:.28rem .5rem"
          ><span class="btn-ico"><i data-lucide="file-text"></i></span> Copier rapport session</button
        >
        <button type="button" class="ctrl-btn btn-secondary" id="btnExportGains" style="font-size:.62rem;padding:.28rem .5rem"
          ><span class="btn-ico"><i data-lucide="download"></i></span> Exporter JSON</button
        >
      </div>
    </div>

    <div class="insights-grid">
      <details class="fold" id="proofFold">
        <summary style="display:flex;align-items:center;gap:.35rem"
          ><span class="pt-ico" style="width:13px;height:13px"><i data-lucide="git-compare"></i></span> Instantané
          avant / après dôme</summary
        >
        <div class="proof-panel" id="proofPanel">
          <div class="proof-placeholder" id="proofPlaceholder">
            Activez le dôme pour comparer le snapshot matériel avant activation et l’état courant.
          </div>
          <div class="proof-compare" id="proofCompare" style="display:none">
            <div class="proof-col"><div class="proof-label">Avant dôme</div><div class="proof-metrics" id="proofBefore">—</div></div>
            <div class="proof-arrow">→</div>
            <div class="proof-col"><div class="proof-label">Maintenant</div><div class="proof-metrics" id="proofAfter">—</div></div>
          </div>
          <div class="proof-placeholder" id="proofSummary" style="display:none;margin-top:.35rem"></div>
        </div>
      </details>

      <details class="fold" open>
        <summary style="display:flex;align-items:center;gap:.35rem"
          ><span class="pt-ico" style="width:13px;height:13px"><i data-lucide="history"></i></span> Historique des
          activations dôme</summary
        >
        <div class="gains-panel">
          <div class="gains-summary" id="gainsSummary">— Aucune activation pour l’instant</div>
          <div class="gains-list" id="gainsList"></div>
        </div>
      </details>

      <details class="fold insights-log">
        <summary style="display:flex;align-items:center;gap:.35rem"
          ><span class="pt-ico" style="width:13px;height:13px"><i data-lucide="scroll-text"></i></span> Journal
          technique</summary
        >
        <div class="log-panel" id="logPanel"></div>
      </details>
    </div>
  </div>
</div>
