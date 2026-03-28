<section class="external-view" id="externalPowerView">
  <div class="external-shell">
    <div class="external-column">
      <div class="advisor-panel external-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico" style="color:var(--io)"><i data-lucide="plug-zap"></i></span><span>Prise externe</span>
        </div>
        <div class="advisor-text">
          Active ici la mesure murale Meross pour boucler la preuve énergétique. SoulKernel garde le pilotage OS, la
          prise sert de référence secteur.
        </div>

        <label class="target-auto" style="margin-top:.55rem">
          <input type="checkbox" id="merossEnabled" />
          Activer la source puissance externe
        </label>

        <label class="kappa-name" for="merossPowerFile" style="display:block;margin-top:.55rem">Fichier watts JSON</label>
        <input
          id="merossPowerFile"
          class="target-select"
          type="text"
          placeholder="~/.config/soulkernel/meross_power.json"
          title="Chemin du JSON écrit par le bridge Meross"
        />

        <label class="kappa-name" for="merossMaxAgeMs" style="display:block;margin-top:.45rem">Fraîcheur max (ms)</label>
        <input
          id="merossMaxAgeMs"
          class="target-select"
          type="number"
          min="1000"
          step="1000"
          value="15000"
          title="Au-delà, la lecture Meross est ignorée"
          style="width:180px"
        />

        <label class="kappa-name" for="merossEmail" style="display:block;margin-top:.55rem">Compte Meross</label>
        <input id="merossEmail" class="target-select" type="email" placeholder="vous@example.com" />

        <label class="kappa-name" for="merossPassword" style="display:block;margin-top:.45rem">Mot de passe</label>
        <input id="merossPassword" class="target-select" type="password" placeholder="Mot de passe Meross" />

        <div class="external-inline-grid" style="margin-top:.45rem">
          <div>
            <label class="kappa-name" for="merossRegion">Région</label>
            <select id="merossRegion" class="target-select">
              <option value="eu">EU</option>
              <option value="us">US</option>
              <option value="ap">AP</option>
            </select>
          </div>
          <div>
            <label class="kappa-name" for="merossDeviceType">Type appareil</label>
            <input id="merossDeviceType" class="target-select" type="text" placeholder="mss315" />
          </div>
        </div>

        <div class="external-inline-grid" style="margin-top:.45rem">
          <div>
            <label class="kappa-name" for="merossPythonBin">Python</label>
            <input id="merossPythonBin" class="target-select" type="text" placeholder="auto: python3 / python / py" />
          </div>
          <div>
            <label class="kappa-name" for="merossBridgeInterval">Intervalle bridge (s)</label>
            <input id="merossBridgeInterval" class="target-select" type="number" min="2" max="300" step="1" value="8" />
          </div>
        </div>

        <label class="target-auto" style="margin-top:.55rem">
          <input type="checkbox" id="merossAutostartBridge" />
          Démarrer le bridge automatiquement au lancement
        </label>

        <div class="gains-actions" style="margin-top:.55rem">
          <button type="button" class="ctrl-btn btn-secondary" id="btnApplyMerossConfig" style="font-size:.62rem;padding:.25rem .45rem">
            <span class="btn-ico"><i data-lucide="save"></i></span> Sauvegarder
          </button>
          <button type="button" class="ctrl-btn btn-secondary" id="btnRefreshMerossStatus" style="font-size:.62rem;padding:.25rem .45rem">
            <span class="btn-ico"><i data-lucide="refresh-cw"></i></span> Rafraîchir état
          </button>
          <button type="button" class="ctrl-btn btn-secondary" id="btnStartMerossBridge" style="font-size:.62rem;padding:.25rem .45rem">
            <span class="btn-ico"><i data-lucide="play"></i></span> Démarrer bridge
          </button>
          <button type="button" class="ctrl-btn btn-danger" id="btnStopMerossBridge" style="font-size:.62rem;padding:.25rem .45rem">
            <span class="btn-ico"><i data-lucide="square"></i></span> Stop bridge
          </button>
        </div>

        <div class="advisor-text" id="merossConfigStatus" style="margin-top:.45rem">Chargement configuration...</div>
      </div>

      <div class="advisor-panel external-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico"><i data-lucide="terminal-square"></i></span><span>Bridge Meross</span>
        </div>
        <div class="target-hint" style="margin-top:.15rem">
          Utilise l’app mobile Meross pour appairer la prise, puis lance ce bridge avec le même compte cloud.
        </div>
        <div class="advisor-text" style="margin-top:.35rem">
          Si le bridge démarre mais qu’aucune mesure n’arrive, ouvre <strong>Diagnostic technique</strong> pour voir la
          commande exacte, le log et la dernière erreur remontée.
        </div>
        <div class="target-hint" style="margin-top:.3rem">
          SoulKernel injecte <code>MEROSS_EMAIL</code>, <code>MEROSS_PASSWORD</code> et <code>MEROSS_REGION</code>
          automatiquement au lancement du bridge.
        </div>
      </div>
    </div>

    <div class="external-column">
      <div class="advisor-panel external-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico" style="color:var(--warning)"><i data-lucide="activity"></i></span><span>État live</span>
        </div>
        <div class="external-hero">
          <div class="external-hero-line">
            <div class="external-hero-status" id="merossOverallStatus">En attente</div>
            <div class="external-runtime-chip" id="merossPythonRuntime">Runtime inconnu</div>
          </div>
          <div class="external-hero-sub" id="merossOverallSummary">
            SoulKernel attend une première mesure murale pour basculer la preuve énergétique sur la prise externe.
          </div>
        </div>
        <div class="external-status-grid">
          <div class="raw-cell">
            <div class="raw-label">Source</div>
            <div class="raw-val" id="merossSourceTag">meross_wall</div>
          </div>
          <div class="raw-cell">
            <div class="raw-label">Dernière mesure</div>
            <div class="raw-val" id="merossLastWatts">N/A</div>
          </div>
          <div class="raw-cell">
            <div class="raw-label">Fraîcheur</div>
            <div class="raw-val" id="merossFreshness">N/A</div>
          </div>
          <div class="raw-cell">
            <div class="raw-label">Fichier JSON</div>
            <div class="raw-val" id="merossFilePresence">N/A</div>
          </div>
          <div class="raw-cell">
            <div class="raw-label">Bridge</div>
            <div class="raw-val" id="merossBridgeRunning">N/A</div>
          </div>
          <div class="raw-cell">
            <div class="raw-label">Crédentials</div>
            <div class="raw-val" id="merossCredentialsState">N/A</div>
          </div>
        </div>
        <div class="target-hint" id="merossActionHint" style="margin-top:.55rem">
          Sauvegarde la configuration, démarre le bridge, puis attends l'écriture du JSON de puissance.
        </div>
        <details class="advanced-fold" style="margin-top:.55rem">
          <summary>Diagnostic technique</summary>
          <div class="advanced-stack">
            <div class="proof-panel">
              <div><strong>Config</strong> : <span id="merossConfigPath">—</span></div>
              <div style="margin-top:.25rem"><strong>Fichier puissance</strong> : <span id="merossResolvedPowerFile">—</span></div>
              <div style="margin-top:.25rem"><strong>Timestamp</strong> : <span id="merossLastTs">—</span></div>
              <div style="margin-top:.25rem"><strong>Dernière erreur</strong> : <span id="merossBridgeError">—</span></div>
              <div style="margin-top:.25rem"><strong>Bridge log</strong> : <span id="merossBridgeLogPath">—</span></div>
              <div style="margin-top:.25rem"><strong>Bridge script</strong> : <span id="merossBridgeScriptPath">—</span></div>
            </div>
            <div class="advisor-panel" style="padding:.5rem .6rem">
              <div class="raw-label" style="margin-bottom:.25rem">Commande debug</div>
              <pre class="external-pre" id="merossBridgeCommand">python3 scripts/meross_mss315_bridge.py --out ~/.config/soulkernel/meross_power.json</pre>
            </div>
          </div>
        </details>
      </div>

      <div class="advisor-panel external-panel">
        <div class="advisor-title" style="display:flex;align-items:center;gap:.35rem">
          <span class="pt-ico"><i data-lucide="circle-help"></i></span><span>But ultime</span>
        </div>
        <div class="advisor-text">
          Si l’état devient <strong>frais</strong> et que la dernière mesure remonte correctement, les watts affichés et
          intégrés dans SoulKernel basculent sur la prise externe au lieu du capteur interne.
        </div>
      </div>
    </div>
  </div>
</section>
