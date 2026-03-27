<div class="panel panel-workloads">
  <div class="panel-title">
    <span class="pt-ico"><i data-lucide="layers"></i></span><span>SCÉNARIO WORKLOAD 𝒲</span>
  </div>
  <div class="target-hint compactable-copy" style="margin:-.35rem 0 .55rem">
    50 scénarios couvrant usages PC courants : coefficients α pour π(t), durée T estimée, et hints SoulRAM par OS
    (Linux zRAM/swap, Windows compression+trim, macOS ratio+caches). Les actions réelles restent conditionnées par
    MemoryPolicy (cooldowns, pression) — pas d’effet « magique » garanti sur W/€ sans mesure électrique.
  </div>
  <div id="wlGrid" class="wl-scene-shell">
    <label class="wl-select-label" for="wlSelect">Scénario dynamique</label>
    <select id="wlSelect" class="wl-select" aria-label="Choisir un scénario workload"></select>
    <div id="wlSceneMeta" class="wl-scene-meta" aria-live="polite"></div>
    <div class="wl-legacy-grid-hint">Raccourcis (7 profils historiques)</div>
    <div class="workload-grid wl-legacy-grid">
      <div class="wl-btn active" data-wl="es">
        <span class="wl-ico" title="Elasticsearch"><i data-lucide="globe"></i></span>
        <div class="wl-name">ELASTICSEARCH</div>
        <div class="wl-hint">α data / recherche</div>
      </div>
      <div class="wl-btn" data-wl="compile">
        <span class="wl-ico" title="Compilation"><i data-lucide="wrench"></i></span>
        <div class="wl-name">COMPILATION</div>
        <div class="wl-hint">α CPU</div>
      </div>
      <div class="wl-btn" data-wl="gamer">
        <span class="wl-ico" title="Gamer"><i data-lucide="gamepad-2"></i></span>
        <div class="wl-name">GAMER</div>
        <div class="wl-hint">α GPU</div>
      </div>
      <div class="wl-btn" data-wl="ai">
        <span class="wl-ico" title="IA"><i data-lucide="brain-circuit"></i></span>
        <div class="wl-name">AI</div>
        <div class="wl-hint">α GPU train</div>
      </div>
      <div class="wl-btn" data-wl="backup">
        <span class="wl-ico" title="Backup"><i data-lucide="save"></i></span>
        <div class="wl-name">BACKUP</div>
        <div class="wl-hint">α I/O</div>
      </div>
      <div class="wl-btn" data-wl="sqlite">
        <span class="wl-ico" title="SQLite"><i data-lucide="database"></i></span>
        <div class="wl-name">SQLITE</div>
        <div class="wl-hint">α disque</div>
      </div>
      <div class="wl-btn" data-wl="oracle">
        <span class="wl-ico" title="SGBD lourd"><i data-lucide="server"></i></span>
        <div class="wl-name">ORACLE / SQL</div>
        <div class="wl-hint">α RAM+I/O</div>
      </div>
    </div>
  </div>
</div>
