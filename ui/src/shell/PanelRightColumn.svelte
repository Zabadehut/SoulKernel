<!-- Colonne droite : Σ, paramètres, SoulRAM / HUD, actions dôme -->
<div class="panel panel-right">
  <div class="panel-title">
    <span class="pt-ico"><i data-lucide="sliders-horizontal"></i></span><span>PILOTAGE · ACTIONS</span>
  </div>
  <div class="advisor-panel operator-brief">
    <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
      <span class="pt-ico" style="color:var(--cpu)"><i data-lucide="crosshair"></i></span><span>Mode opérateur</span>
    </div>
    <div class="advisor-text">
      Décider vite: lire <strong>Σ</strong>, appliquer la recommandation, puis lancer ou rollback. Les détails restent
      disponibles plus bas sans polluer la boucle principale.
    </div>
  </div>
  <div class="pilotage-card pilotage-card--stress">
    <div class="pilotage-title" style="display:flex;align-items:center;gap:.35rem">
      <span class="pt-ico" style="color:var(--stress)"><i data-lucide="gauge"></i></span><span>Stress global Σ(t)</span>
    </div>
    <div class="sigma-gauge">
      <div class="sigma-fill" id="sigmaFill" style="height:0"></div>
      <div class="sigma-val" id="sigmaVal">—</div>
      <div class="sigma-label" id="sigmaSubLabel">PSI + STEAL + MEM PRESSURE</div>
    </div>
  </div>

  <div class="pilotage-card">
    <div class="pilotage-title" style="display:flex;align-items:center;gap:.35rem">
      <span class="pt-ico" style="color:var(--io)"><i data-lucide="sparkles"></i></span><span
        >Pilotage intelligent</span
      >
    </div>
    <div class="kappa-ctrl">
      <div class="kappa-row">
        <span class="kappa-name" style="color:var(--io)">Policy moteur</span>
        <span class="kappa-num" id="policyBadge" style="color:var(--io)">PRIVILEGED</span>
      </div>
      <select id="policyMode" class="target-select">
        <option value="privileged">Privileged (max perf)</option>
        <option value="safe">Safe (non-invasif)</option>
      </select>
      <div class="target-hint" id="policyStatusHint">admin: -- | reboot pending: --</div>
    </div>

    <div class="kappa-ctrl">
      <div class="kappa-row">
        <span class="kappa-name">κ · FREIN STABILITÉ</span>
        <span class="kappa-num" id="kappaNum">2.0</span>
      </div>
      <input type="range" id="kappaSlider" min="0.5" max="5" step="0.1" value="2.0" />
    </div>
    <div class="kappa-ctrl">
      <div class="kappa-row">
        <span class="kappa-name" style="color:var(--muted)">Σ<sub>max</sub></span>
        <span class="kappa-num" id="sigmaMaxNum" style="color:var(--stress)">0.75</span>
      </div>
      <input type="range" class="mem-range" id="sigmaMaxSlider" min="0.3" max="0.95" step="0.05" value="0.75" />
    </div>
    <div class="kappa-ctrl">
      <div class="kappa-row">
        <span class="kappa-name" style="color:var(--muted)">η · AJUSTEMENT</span>
        <span class="kappa-num" id="etaNum" style="color:var(--io)">0.15</span>
      </div>
      <input type="range" class="io-range" id="etaSlider" min="0.01" max="0.5" step="0.01" value="0.15" />
    </div>

    <div class="kappa-ctrl">
      <div class="kappa-row">
        <span class="kappa-name" style="color:var(--mem)">Processus cible</span>
        <button type="button" class="ctrl-btn btn-secondary" id="btnRefreshProcesses" style="font-size:.65rem;padding:.15rem .4rem"
          ><span class="btn-ico"><i data-lucide="refresh-cw"></i></span> Rafraîchir</button
        >
      </div>
      <div class="target-tools">
        <label class="target-auto">
          <input type="checkbox" id="autoProcessTarget" checked />
          Auto-cible (CPU max)
        </label>
        <span class="target-auto" id="processRefreshInfo">maj auto: --</span>
      </div>
      <select id="targetProcess" class="target-select">
        <option value="">Ce processus (SoulKernel)</option>
      </select>
      <div class="target-hint">
        Si un processus est choisi, le dôme lui donne un maximum d’amplitude et de performance (tous les cœurs,
        priorité haute, working set 2–4 Go).
      </div>
      <div class="process-impact-card">
        <div class="process-impact-head">
          <div class="process-impact-title">Impact processus</div>
          <div class="process-impact-sub" id="processImpactSummary">En attente de collecte...</div>
        </div>
        <div class="target-hint" id="overheadAuditSummary" style="margin-bottom:.35rem">
          Audit overhead SoulKernel/WebView en attente...
        </div>
        <div class="process-impact-scroll">
          <table class="process-impact-table">
            <thead>
              <tr>
                <th>Processus</th>
                <th>PID</th>
                <th>CPU % 🔬</th>
                <th>GPU % 🔬</th>
                <th>RAM 🔬</th>
                <th>I/O 🔬</th>
                <th>Puiss. est. 〜</th>
                <th>Impact est. % 〜</th>
                <th>Durée</th>
                <th>Statut</th>
                <th>Rôle</th>
              </tr>
            </thead>
            <tbody id="processImpactRows">
              <tr><td colspan="11" class="process-impact-empty">Aucune donnée processus.</td></tr>
            </tbody>
          </table>
        </div>
        <div class="target-hint" style="margin-top:.35rem">
          🔬 = observé par processus. 〜 = attribution estimée à partir de CPU/GPU/RAM/I/O et, si disponible, de la puissance machine mesurée.
        </div>
      </div>
    </div>

    <div class="advisor-panel">
      <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
        <span class="pt-ico"><i data-lucide="radio-tower"></i></span><span>Recommandation live</span>
      </div>
      <div class="advisor-text" id="tuningAdvice">En attente des metriques...</div>
      <div class="advisor-actions">
        <button type="button" class="ctrl-btn btn-secondary" id="btnApplyAdvice" style="font-size:.62rem;padding:.25rem .45rem"
          ><span class="btn-ico"><i data-lucide="check"></i></span> Appliquer</button
        >
      </div>
    </div>

    <details class="advanced-fold" id="advancedFold">
      <summary style="display:flex;align-items:center;gap:.35rem"
        ><span class="pt-ico" style="width:13px;height:13px"><i data-lucide="wrench"></i></span> Outils avances</summary
      >
      <div class="advanced-stack">
        <div class="kappa-ctrl" id="soulRamPanel">
          <div class="kappa-row">
            <span class="kappa-name" style="color:var(--io);display:inline-flex;align-items:center;gap:.3rem"
              ><span class="pt-ico" style="width:13px;height:13px;color:var(--io)"
                ><i data-lucide="memory-stick"></i></span
              > SoulRAM · backend memoire OS</span
            >
            <span class="kappa-num" id="soulRamStatus" style="color:var(--stress)">OFF</span>
          </div>
          <p class="target-hint" style="margin:.25rem 0 .35rem;line-height:1.45">
            <strong>But :</strong> détendre la mémoire hors dôme via un <strong>backend natif propre à l’OS</strong>.
            Le curseur fixe un <strong>objectif de politique</strong>, pas un gain garanti, et les effets se lisent dans la
            télémétrie réelle ci-dessous.
          </p>
          <div class="kappa-row" style="gap:.5rem;align-items:center">
            <input type="range" id="soulRamPct" min="5" max="60" step="1" value="20" style="flex:1" />
            <span class="kappa-num" id="soulRamPctNum" style="color:var(--io)">20%</span>
          </div>
          <div class="proof-panel" id="soulRamTelemetryBox" style="margin-top:.3rem;padding:.35rem;font-size:.62rem;line-height:1.4">
            <strong>Télémétrie réelle</strong>
            <div id="soulRamTelemetryLine" style="margin-top:.2rem;color:var(--muted)">
              Chargez les métriques (app native) pour voir le cumul SoulRAM par backend OS.
            </div>
            <div id="soulRamPolicyLine" style="margin-top:.15rem;color:var(--muted)">Backend / politique : —</div>
            <div id="soulRamGoalLine" style="margin-top:.15rem;color:var(--muted)">Equivalent fonctionnel : —</div>
            <div id="soulRamRoadmapLine" style="margin-top:.15rem;color:var(--muted)">Roadmap OS : —</div>
          </div>
          <div class="gains-actions" style="margin-top:.35rem">
            <button type="button" class="ctrl-btn btn-secondary" id="btnSoulRamOn" style="font-size:.62rem;padding:.25rem .45rem"
              ><span class="btn-ico"><i data-lucide="power"></i></span> Activer SoulRAM</button
            >
            <button type="button" class="ctrl-btn btn-danger" id="btnSoulRamOff" style="font-size:.62rem;padding:.25rem .45rem"
              ><span class="btn-ico"><i data-lucide="power-off"></i></span> Desactiver SoulRAM</button
            >
          </div>
        </div>

        <div class="advisor-panel" id="adaptivePanel" style="margin-top:.35rem">
          <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
            <span class="pt-ico"><i data-lucide="orbit"></i></span><span>Mode adaptatif (low overhead)</span>
          </div>
          <label class="target-auto"><input type="checkbox" id="adaptiveEnabled" /> Adaptive engine</label>
          <label class="target-auto" style="margin-top:.2rem"
            ><input type="checkbox" id="adaptiveAutoDome" checked /> Auto Dome</label
          >
          <div class="advisor-text" id="adaptiveStatus" style="margin-top:.3rem">OFF</div>
        </div>

        <div class="advisor-panel" id="hudPanel" style="margin-top:.35rem">
          <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
            <span class="pt-ico"><i data-lucide="monitor"></i></span><span>HUD Overlay (2ᵉ fenêtre)</span>
          </div>
          <p class="target-hint" style="margin:.2rem 0;line-height:1.45">
            Fenêtre always-on-top sur les mêmes métriques. Si elle décroche, repasse par <strong>HUD ON</strong> dans la
            barre d’outils avant de retenter.
          </p>
          <div class="gains-actions" style="margin-top:.2rem">
            <button type="button" class="ctrl-btn btn-secondary" id="btnHudEdit" style="font-size:.62rem;padding:.25rem .45rem"
              ><span class="btn-ico"><i data-lucide="mouse-pointer-2"></i></span> Edition OFF</button
            >
          </div>
          <div class="kappa-row" style="margin-top:.3rem;gap:.4rem"
            ><span class="kappa-name">Ecran HUD</span><select id="hudDisplay" class="target-select" style="width:130px;padding:.2rem .35rem"
              ><option value="">Auto</option></select
            ></div
          >
          <div class="kappa-row" style="margin-top:.3rem;gap:.4rem"
            ><span class="kappa-name">Preset</span><select id="hudPreset" class="target-select" style="width:130px;padding:.2rem .35rem"
              ><option value="mini">Mini</option><option value="compact" selected>Compact</option><option value="detailed"
                >Detaille</option
              ></select
            ></div
          >
          <div
            class="kappa-row"
            style="margin-top:.3rem;gap:.4rem;align-items:center;flex-wrap:wrap"
            ><span class="kappa-name">Taille</span><select
              id="hudSizeMode"
              class="target-select"
              style="width:150px;padding:.2rem .35rem"
              title="Ecran % = pourcentage de la resolution native de l’ecran HUD selectionne"
              ><option value="screen">% ecran actif</option><option value="content">Contenu (ajuste)</option><option value="manual"
                >Manuel (px)</option
              ></select
            ></div
          >
          <div id="hudSizeScreen">
            <div class="kappa-row" style="margin-top:.25rem;gap:.4rem;align-items:center"
              ><span class="kappa-name">Largeur</span><input id="hudScreenW" type="range" min="8" max="50" step="1" value="22" style="width:120px" /><span
                class="kappa-num"
                id="hudScreenWNum"
                style="color:var(--cpu);min-width:2.5rem">22%</span
              ></div
            >
            <div class="kappa-row" style="margin-top:.2rem;gap:.4rem;align-items:center"
              ><span class="kappa-name">Hauteur</span><input id="hudScreenH" type="range" min="8" max="50" step="1" value="28" style="width:120px" /><span
                class="kappa-num"
                id="hudScreenHNum"
                style="color:var(--cpu);min-width:2.5rem">28%</span
              ></div
            >
            <div class="target-hint" id="hudActiveResHint" style="margin-top:.2rem;font-size:.58rem;line-height:1.35">
              Ecran actif : —
            </div>
          </div>
          <div id="hudSizeManual" style="display:none">
            <div class="kappa-row" style="margin-top:.25rem;gap:.4rem"
              ><span class="kappa-name">L (px)</span><input
                id="hudManW"
                type="number"
                min="240"
                max="1600"
                value="420"
                class="target-select"
                style="width:92px;padding:.2rem .35rem"
              /></div
            >
            <div class="kappa-row" style="margin-top:.2rem;gap:.4rem"
              ><span class="kappa-name">H (px)</span><input
                id="hudManH"
                type="number"
                min="120"
                max="1200"
                value="260"
                class="target-select"
                style="width:92px;padding:.2rem .35rem"
              /></div
            >
          </div>
          <div class="kappa-row" style="margin-top:.35rem"><span class="kappa-name">Metriques</span></div>
          <div
            style="display:grid;grid-template-columns:1fr 1fr;gap:.15rem .45rem;font-size:.58rem;margin-top:.12rem;line-height:1.35"
          >
            <label class="target-auto"><input type="checkbox" class="hud-metric-cb" data-metric="dome" checked /> Dome</label>
            <label class="target-auto"><input type="checkbox" class="hud-metric-cb" data-metric="sigma" checked /> Sigma</label>
            <label class="target-auto"><input type="checkbox" class="hud-metric-cb" data-metric="pi" checked /> Pi</label>
            <label class="target-auto"><input type="checkbox" class="hud-metric-cb" data-metric="cpu" checked /> CPU</label>
            <label class="target-auto"><input type="checkbox" class="hud-metric-cb" data-metric="ram" checked /> RAM</label>
            <label class="target-auto"><input type="checkbox" class="hud-metric-cb" data-metric="target" checked /> Cible</label>
            <label class="target-auto"><input type="checkbox" class="hud-metric-cb" data-metric="power" checked /> Puissance</label>
            <label class="target-auto"><input type="checkbox" class="hud-metric-cb" data-metric="energy" checked /> Energie</label>
          </div>
          <div class="kappa-row" style="margin-top:.25rem;gap:.4rem"
            ><span class="kappa-name">Opacite</span><input
              id="hudOpacity"
              type="range"
              min="0.30"
              max="1"
              step="0.05"
              value="0.82"
              style="width:130px"
            /><span class="kappa-num" id="hudOpacityNum" style="color:var(--cpu)">82%</span></div
          >
          <div class="target-hint" style="margin-top:.2rem">
            Raccourcis : Alt+Shift+H (toggle), Alt+Shift+J (édition pointeur).
          </div>
        </div>

        <button class="ctrl-btn btn-secondary" id="btnInfo" type="button"
          ><span class="btn-ico"><i data-lucide="info"></i></span> Platform info</button
        >
      </div>
    </details>
  </div>

  <div class="controls controls-primary controls-primary--sticky">
    <button class="ctrl-btn btn-primary" id="btnDome"
      ><span class="btn-ico"><i data-lucide="rocket"></i></span> ACTIVER DOME</button
    >
    <button class="ctrl-btn btn-danger" id="btnReset"
      ><span class="btn-ico"><i data-lucide="rotate-ccw"></i></span> ROLLBACK</button
    >
  </div>
</div>
