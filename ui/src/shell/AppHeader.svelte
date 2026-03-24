<header>
  <div class="logo">
    <img
      class="logo-img"
      src="/assets/brand/icon_32x32.png"
      width="28"
      height="28"
      alt=""
      aria-hidden="true"
    />
    <div>
      <div class="logo-text">SoulKernel</div>
      <div class="logo-sub">PERFORMANCE DOME ORCHESTRATOR</div>
    </div>
  </div>
  <div class="status-bar">
    <div
      class="view-switch"
      title="Pilotage = essentiel + contrôles · Standard = audit complet · Benchmark = preuve A/B"
    >
      <button type="button" class="view-btn" id="btnViewCompact"
        ><span class="view-ico"><i data-lucide="layout-dashboard"></i></span> Pilotage</button
      >
      <button type="button" class="view-btn active" id="btnViewDetailed"
        ><span class="view-ico"><i data-lucide="layers"></i></span> Standard</button
      >
      <button type="button" class="view-btn" id="btnViewBenchmark"
        ><span class="view-ico"><i data-lucide="flask-conical"></i></span> Benchmark</button
      >
      <button type="button" class="view-btn" id="btnHudToggle"
        ><span class="view-ico"><i data-lucide="panel-top"></i></span> HUD OFF</button
      >
    </div>
    <div class="platform-badge" id="platformBadge">Connecting…</div>
    <div class="status-pill pill-ok" id="sysStatus">SYSTEM OK</div>
    <div class="status-pill pill-warn" id="activityBadge" style="display:none">ACTIF</div>
    <div class="status-pill pill-off" id="domeStatus">DOME IDLE</div>
    <div class="clock" id="clk">--:--:--</div>
  </div>
</header>
