<!-- Colonne gauche : ressources r(t) + repli audit -->
<div class="panel panel-left">
  <div class="panel-title">
    <span class="pt-ico"><i data-lucide="activity"></i></span><span>HARDWARE r(t) · LIVE</span>
  </div>

  <div class="resource-item">
    <div class="res-header">
      <span class="res-header-start">
        <span class="res-metric-icon res-metric-icon--cpu" title="CPU — charge processeur r(t)"
          ><i data-lucide="cpu"></i></span
        >
        <span class="res-name" style="color:var(--cpu)">C(t) · CPU</span>
      </span>
      <span class="res-val" style="color:var(--cpu)" id="vCPU">—</span>
    </div>
    <div class="bar-track"><div class="bar-fill bar-cpu" id="bCPU" style="width:0"></div></div>
    <div class="res-label" id="lCPU">α<sub>C</sub> — polling…</div>
  </div>

  <div class="resource-item">
    <div class="res-header">
      <span class="res-header-start">
        <span class="res-metric-icon res-metric-icon--mem" title="RAM — mémoire vive r(t)"
          ><i data-lucide="memory-stick"></i></span
        >
        <span class="res-name" style="color:var(--mem)">M(t) · RAM</span>
      </span>
      <span class="res-val" style="color:var(--mem)" id="vMEM">—</span>
    </div>
    <div class="bar-track"><div class="bar-fill bar-mem" id="bMEM" style="width:0"></div></div>
    <div class="res-label" id="lMEM">polling…</div>
  </div>

  <div class="resource-item">
    <div class="res-header">
      <span class="res-header-start">
        <span class="res-metric-icon res-metric-icon--lam" title="Λ — compression mémoire (zRAM / swap)"
          ><i data-lucide="archive"></i></span
        >
        <span class="res-name" style="color:var(--io)">Λ(t) · COMPRESSION</span>
      </span>
      <span class="res-val" style="color:var(--io)" id="vLAM">—</span>
    </div>
    <div class="bar-track"><div class="bar-fill bar-lam" id="bLAM" style="width:0"></div></div>
    <div class="res-label" id="lLAM">zRAM / zSWAP ratio</div>
  </div>

  <div class="resource-item">
    <div class="res-header">
      <span class="res-header-start">
        <span class="res-metric-icon res-metric-icon--io" title="I/O — bande passante disque r(t)"
          ><i data-lucide="arrow-down-up"></i></span
        >
        <span class="res-name" style="color:var(--gpu)">B<sub>io</sub>(t) · I/O</span>
      </span>
      <span class="res-val" style="color:var(--gpu)" id="vIO">—</span>
    </div>
    <div class="bar-track"><div class="bar-fill bar-io" id="bIO" style="width:0"></div></div>
    <div class="res-label" id="lIO">polling…</div>
  </div>

  <div class="resource-item">
    <div class="res-header">
      <span class="res-header-start">
        <span class="res-metric-icon res-metric-icon--gpu" title="GPU — charge graphique r(t)"
          ><i data-lucide="monitor-play"></i></span
        >
        <span class="res-name" style="color:#ff6b6b">G(t) · GPU</span>
      </span>
      <span class="res-val" style="color:#ff6b6b" id="vGPU">—</span>
    </div>
    <div class="bar-track"><div class="bar-fill bar-gpu" id="bGPU" style="width:0"></div></div>
    <div class="res-label" id="lGPU">IOKit / sysfs / DXGI</div>
  </div>

  <details
    class="advanced-fold"
    style="margin-top:1.2rem;padding-top:.8rem;border-top:1px solid var(--s3)"
  >
    <summary style="display:flex;align-items:center;gap:.35rem"
      ><span class="pt-ico" style="width:13px;height:13px"><i data-lucide="microchip"></i></span> Données
      brutes & plateforme (audit, optionnel)</summary
    >
    <div class="advanced-stack" style="margin-top:.45rem">
      <div class="panel-title" style="margin-bottom:.4rem">
        <span class="pt-ico"><i data-lucide="gauge"></i></span><span>Compteurs matériel</span>
      </div>
      <div class="raw-grid">
        <div class="raw-cell"><div class="raw-label">MEM used</div><div class="raw-val" id="rawMemUsed">—</div></div>
        <div class="raw-cell"><div class="raw-label">MEM total</div><div class="raw-val" id="rawMemTotal">—</div></div>
        <div class="raw-cell"><div class="raw-label">zRAM used</div><div class="raw-val" id="rawZram">—</div></div>
        <div class="raw-cell"><div class="raw-label">PSI CPU</div><div class="raw-val" id="rawPsiCpu">—</div></div>
        <div class="raw-cell"><div class="raw-label">PSI MEM</div><div class="raw-val" id="rawPsiMem">—</div></div>
        <div class="raw-cell"><div class="raw-label">CPU %</div><div class="raw-val" id="rawCpuPct">—</div></div>
        <div class="raw-cell"><div class="raw-label">CPU clock</div><div class="raw-val" id="rawCpuClock">—</div></div>
        <div class="raw-cell"><div class="raw-label">RAM speed</div><div class="raw-val" id="rawRamClock">—</div></div>
        <div class="raw-cell"><div class="raw-label">I/O R/W</div><div class="raw-val" id="rawIoRw">—</div></div>
        <div class="raw-cell"><div class="raw-label">GPU %</div><div class="raw-val" id="rawGpuPct">—</div></div>
        <div class="raw-cell"><div class="raw-label">GPU core</div><div class="raw-val" id="rawGpuCoreClock">—</div></div>
        <div class="raw-cell"><div class="raw-label">GPU mem</div><div class="raw-val" id="rawGpuMemClock">—</div></div>
        <div class="raw-cell"><div class="raw-label">Power (W)</div><div class="raw-val" id="rawPowerW">—</div></div>
        <div class="raw-cell"><div class="raw-label">Opt real (A/B)</div><div class="raw-val" id="rawOptReal">—</div></div>
        <div class="raw-cell"><div class="raw-label">RAM·GB·H (tel.)</div><div class="raw-val" id="rawMemGbHTel">—</div></div>
        <div class="raw-cell"><div class="raw-label">Energy total</div><div class="raw-val" id="rawEnergyTotal">—</div></div>
        <div class="raw-cell"><div class="raw-label">Cost total</div><div class="raw-val" id="rawCostTotal">—</div></div>
        <div class="raw-cell"><div class="raw-label">Energy H/J/S/M/A</div><div class="raw-val" id="rawEnergyWindows">—</div></div>
        <div class="raw-cell"><div class="raw-label">Activite</div><div class="raw-val" id="rawActivity">—</div></div>
        <div class="raw-cell"><div class="raw-label">WebView hôte (ΣCPU)</div><div class="raw-val" id="rawWebviewCpu">—</div></div>
        <div class="raw-cell"><div class="raw-label">WebView hôte (RAM)</div><div class="raw-val" id="rawWebviewMem">—</div></div>
      </div>
      <div style="margin-top:.8rem;padding-top:.8rem;border-top:1px solid var(--s3)">
        <div class="panel-title" style="margin-bottom:.3rem">
          <span class="pt-ico"><i data-lucide="puzzle"></i></span><span>Fonctions plateforme</span>
        </div>
        <div class="feat-tags" id="featTags"></div>
      </div>
    </div>
  </details>
</div>
