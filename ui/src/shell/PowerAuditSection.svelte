<section class="audit-view" id="powerAuditView">
  <div class="audit-mode-bar">
    <div class="audit-mode-copy">
      <div class="audit-mode-title">Mode de lecture</div>
      <div class="audit-mode-sub">Audit brut pour la preuve, vue énergétique pour lire usage observé, watts directs et parts estimées sans les confondre.</div>
    </div>
    <div class="view-switch audit-mode-switch">
      <button type="button" class="view-btn active" id="btnAuditShowRaw">
        <span class="view-ico"><i data-lucide="scroll-text"></i></span> Audit brut
      </button>
      <button type="button" class="view-btn" id="btnAuditShowEnergy">
        <span class="view-ico"><i data-lucide="network"></i></span> Vue énergétique
      </button>
    </div>
  </div>

  <div class="audit-subview audit-subview--energy" id="auditEnergyView"></div>

  <div class="audit-subview audit-subview--raw is-active" id="auditRawView">
  <div class="audit-shell">
    <div class="audit-column">
      <div class="advisor-panel audit-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico" style="color:var(--io)"><i data-lucide="radar"></i></span><span>Réconciliation puissance</span>
        </div>
        <div class="advisor-text">
          Réunit la source active, les métriques machine observées, les processus dominants et la part expliquée du total.
        </div>
        <div class="external-hero" style="margin-top:.45rem">
          <div class="external-hero-line">
            <div class="external-hero-status" id="auditPowerHeadline">En attente de métriques…</div>
            <div class="external-runtime-chip" id="auditPowerConfidence">Confiance inconnue</div>
          </div>
          <div class="external-hero-sub" id="auditPowerSummary">
            SoulKernel validera ici la boucle mur / machine / processus / périphériques quand les sources seront disponibles.
          </div>
        </div>
        <div class="audit-kpi-grid">
          <div class="raw-cell"><div class="raw-label">Mur / externe</div><div class="raw-val" id="auditWallPower">—</div></div>
          <div class="raw-cell"><div class="raw-label">Source active</div><div class="raw-val" id="auditPowerSource">—</div></div>
          <div class="raw-cell"><div class="raw-label">Puissance host</div><div class="raw-val" id="auditHostPower">—</div></div>
          <div class="raw-cell"><div class="raw-label">Part expliquée</div><div class="raw-val" id="auditExplainedShare">—</div></div>
          <div class="raw-cell"><div class="raw-label">Top process cumulés</div><div class="raw-val" id="auditTopPower">—</div></div>
          <div class="raw-cell"><div class="raw-label">Part non attribuée</div><div class="raw-val" id="auditUnattributed">—</div></div>
        </div>
      </div>

      <div class="advisor-panel audit-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico" style="color:var(--cpu)"><i data-lucide="server-cog"></i></span><span>Machine & OS</span>
        </div>
        <div class="target-hint">Métriques natives observées de la machine entière. Les W n’apparaissent que si la plateforme ou le matériel les expose réellement.</div>
        <div class="audit-grid-two" id="auditHostSubsystems"></div>
      </div>

      <div class="advisor-panel audit-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico" style="color:var(--gpu)"><i data-lucide="usb"></i></span><span>Sources externes & périphériques</span>
        </div>
        <div class="target-hint">Source externe active ou absente, fraîcheur, bridge/config, et niveau de preuve de la puissance disponible à l’instant T.</div>
        <div class="audit-grid-two" id="auditExternalSources"></div>
        <div class="audit-list" id="auditDetectedDevices" style="margin-top:.55rem"></div>
      </div>
    </div>

    <div class="audit-column">
      <div class="advisor-panel audit-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico" style="color:var(--mem)"><i data-lucide="cpu"></i></span><span>Processus & groupes</span>
        </div>
        <div class="target-hint">Observé d’abord, estimé en second. CPU/GPU/RAM/I/O sont lus nativement quand disponibles, les W par processus restent une attribution.</div>
        <div class="audit-list" id="auditProcessGroups"></div>
        <div class="audit-list" id="auditTopProcesses" style="margin-top:.55rem"></div>
      </div>

      <div class="advisor-panel audit-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico" style="color:var(--warning)"><i data-lucide="panel-top"></i></span><span>Overhead SoulKernel</span>
        </div>
        <div class="target-hint">Sépare l’hôte SoulKernel, la WebView et les rôles runtime internes du moteur.</div>
        <div class="audit-list" id="auditOverheadBreakdown"></div>
      </div>
    </div>
  </div>
  </div>
</section>
