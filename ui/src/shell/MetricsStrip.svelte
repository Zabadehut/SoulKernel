<div class="metrics-strip" id="metricsStrip" role="region" aria-label="Synthèse σ π r(t)">
  <div class="ms-seg ms-sigma">
    <span class="ms-ico-wrap" id="stripSigmaIconWrap"><i data-lucide="sigma"></i></span>
    <span class="ms-label">σ</span>
    <span class="ms-val" id="stripSigma">—</span>
  </div>
  <div class="ms-seg ms-pi">
    <span class="ms-ico-wrap" style="color:var(--primary)"><i data-lucide="pi"></i></span>
    <span class="ms-label">π</span>
    <span class="ms-val" id="stripPi">—</span>
    <div class="ms-pi-wrap" style="--pi-pct:0" aria-hidden="true">
      <div class="ms-pi-ring"></div>
      <div class="ms-pi-arc" id="stripPiArc"></div>
    </div>
  </div>
  <div class="ms-seg" title="CPU global machine · usage observé">
    <span class="ms-ico-wrap" style="color:var(--cpu)"><i data-lucide="cpu"></i></span>
    <span class="ms-label">CPU</span>
    <div class="ms-mini-track"><div class="ms-mini-fill ms-mini-fill--cpu" id="stripCpuFill"></div></div>
  </div>
  <div class="ms-seg" title="RAM machine · occupation observée">
    <span class="ms-ico-wrap" style="color:var(--mem)"><i data-lucide="memory-stick"></i></span>
    <span class="ms-label">RAM</span>
    <div class="ms-mini-track"><div class="ms-mini-fill ms-mini-fill--mem" id="stripMemFill"></div></div>
  </div>
  <div class="ms-seg" title="I/O machine · activité observée">
    <span class="ms-ico-wrap" style="color:var(--gpu)"><i data-lucide="arrow-down-up"></i></span>
    <span class="ms-label">I/O</span>
    <div class="ms-mini-track"><div class="ms-mini-fill ms-mini-fill--io" id="stripIoFill"></div></div>
  </div>
</div>
