<!-- Centre : formule π, 𝒟, détail modèle, Green IT -->
<div class="panel panel-center">
  <div class="panel-title">
    <span class="pt-ico"><i data-lucide="function-square"></i></span><span>FORMULE UNIFIÉE π(t)</span>
  </div>
  <div class="target-hint panel-lead compactable-copy" style="margin:-.35rem 0 .65rem;max-width:36rem">
    Lecture opérateur: <strong>π</strong> mesure le rendement instantané, <strong>𝒟</strong> la valeur cumulée.
    Les gains affichés restent fondés sur <strong>r(t) réel</strong> et se défendent via <strong>Benchmark A/B</strong>.
  </div>

  <div class="formula-hero">
    <div class="formula-zone formula-zone--hero" id="fZone">
      <div class="formula-kicker">Rendement instantané</div>
      <div class="formula-main">
        <span class="f-sym" style="font-size:1.1em">π</span><span class="f-op">(t) = (</span>
        <span class="f-sym">𝒲</span><span class="f-op">·</span><span class="f-sym">r</span><span class="f-op"
          >(t)) ·</span
        >
        <span class="f-op">∏</span><sub style="font-size:.6em">k</sub>
        <span class="f-op">(1−</span><span class="f-sym f-green">ε</span><sub style="font-size:.6em">k</sub
          ><span class="f-op">)</span
        ><sup style="font-size:.6em;color:var(--io)">α<sub>k</sub></sup>
        <span class="f-op"> · </span>
        <span class="f-red">e</span><sup style="font-size:.6em;color:var(--stress)">−κΣ(t)</sup>
      </div>
      <div class="eq-result" id="piVal">π = —</div>
      <div class="formula-label">RENDEMENT INSTANTANÉ · HARDWARE LIVE</div>
    </div>

    <div class="formula-zone formula-zone--side" id="dZone" style="margin-bottom:.6rem">
      <div class="formula-kicker">Résultat de session</div>
      <div class="formula-main">
        <span class="f-sym" style="font-size:1em">𝒟</span>
        <span class="f-op">[τ₀,τ₁] = ∫π(t)dt −</span>
        <span class="f-purple">𝒞</span><sub style="font-size:.6em;color:var(--mem)">setup</sub>
        <span class="f-op">−</span>
        <span class="f-purple">𝒞</span><sub style="font-size:.6em;color:var(--mem)">rb</sub>
      </div>
      <div class="eq-result" id="domeVal" style="font-size:1.1rem">𝒟 = —</div>
      <div class="formula-label">∫π·dt (session) — coûts d’affichage UI retirés du nombre montré</div>
    </div>
  </div>

  <details class="advanced-fold">
    <summary style="display:flex;align-items:center;gap:.35rem"
      ><span class="pt-ico" style="width:13px;height:13px"><i data-lucide="bar-chart-3"></i></span> Détail du
      modèle (α, barres, friction)</summary
    >
    <div class="advanced-stack">
      <div class="dome-vis">
        <div class="dome-title">ALLOCATION PAR DIMENSION · α · r(t)</div>
        <div class="dome-bars">
          <div class="dome-bar-wrap">
            <div class="dome-bar-col" id="dc0" style="height:0;background:var(--cpu)"></div>
            <div class="dome-bar-label">CPU</div>
          </div>
          <div class="dome-bar-wrap">
            <div class="dome-bar-col" id="dc1" style="height:0;background:var(--mem)"></div>
            <div class="dome-bar-label">MEM</div>
          </div>
          <div class="dome-bar-wrap">
            <div class="dome-bar-col" id="dc2" style="height:0;background:var(--io)"></div>
            <div class="dome-bar-label">COMP</div>
          </div>
          <div class="dome-bar-wrap">
            <div class="dome-bar-col" id="dc3" style="height:0;background:var(--gpu)"></div>
            <div class="dome-bar-label">I/O</div>
          </div>
          <div class="dome-bar-wrap">
            <div class="dome-bar-col" id="dc4" style="height:0;background:#ff6b6b"></div>
            <div class="dome-bar-label">GPU</div>
          </div>
        </div>
      </div>

      <div class="metrics-row">
        <div class="metric-cell"><div class="metric-val" id="mBrut">—</div><div class="metric-name">GAIN BRUT</div></div>
        <div class="metric-cell"><div class="metric-val" id="mFric">—</div><div class="metric-name">FRICTION</div></div>
        <div class="metric-cell"><div class="metric-val" id="mFrein">—</div><div class="metric-name">FREIN σ</div></div>
        <div class="metric-cell"><div class="metric-val" id="mRenta">—</div><div class="metric-name">RENTABLE</div></div>
      </div>
    </div>
  </details>

  <div class="green-it-panel" id="greenItPanel">
    <div class="green-it-title">
      <span class="pt-ico" style="color:var(--io)"><i data-lucide="leaf"></i></span><span>GREEN IT · IMPACT</span
      ><span class="green-it-source" id="greenItSource">—</span>
    </div>
    <div class="green-it-grid">
      <div class="green-it-stat">
        <div class="green-it-val" id="greenCpuH">0</div>
        <div class="green-it-label">CPU·H SAVED</div>
      </div>
      <div class="green-it-stat">
        <div class="green-it-val" id="greenMemGh">0</div>
        <div class="green-it-label">RAM·GB·H ÉPARGNÉS</div>
      </div>
      <div class="green-it-stat">
        <div class="green-it-val" id="greenCo2">0</div>
        <div class="green-it-label">KG CO₂ (kWh MES.)</div>
      </div>
    </div>
    <div class="green-it-grid" style="margin-top:.35rem">
      <div class="green-it-stat">
        <div class="green-it-val" id="greenKwh">0</div>
        <div class="green-it-label">KWH MESURÉS</div>
      </div>
      <div class="green-it-stat">
        <div class="green-it-val" id="greenDomeN">0</div>
        <div class="green-it-label">ACTIVATIONS</div>
      </div>
      <div class="green-it-stat">
        <div class="green-it-val" id="greenDomeH">0</div>
        <div class="green-it-label">DOME HOURS</div>
      </div>
    </div>
    <div class="green-it-grid" style="margin-top:.35rem">
      <div class="green-it-stat" style="grid-column:1 / -1">
        <div class="green-it-val" id="greenDomeD">0</div>
        <div class="green-it-label">∫𝒟 RÉELLE</div>
      </div>
    </div>
    <div class="green-it-lifetime" id="greenItLifetime">Chargement des donnees vie entiere...</div>
    <div
      class="green-it-hint compactable-copy"
      style="margin-top:.5rem;font-size:.74rem;line-height:1.35;color:var(--muted);max-width:42rem"
    >
      Compteurs <strong>ACTIF</strong> seulement (idle/média exclus). <strong>kWh / CO₂ / €</strong> = intégrale
      de la <strong>puissance mesurée</strong> × tarifs (pas un « gain dôme » sans double mesure énergétique).
    </div>
    <details class="advanced-fold" style="margin-top:.4rem">
      <summary
        style="font-size:.65rem;color:var(--mem);display:flex;align-items:center;gap:.35rem"
        ><span class="pt-ico" style="width:12px;height:12px"><i data-lucide="book-open"></i></span> Méthode
        complète & limites (pour audit / revue)</summary
      >
      <div
        class="green-it-hint compactable-copy"
        style="margin-top:.35rem;font-size:.72rem;line-height:1.4;color:var(--muted);border:none;padding:0;max-width:42rem"
      >
        <strong>CPU·h</strong> et <strong>RAM·GB·h</strong> comparent baseline OFF et mesures ON sur fenêtre glissante.
        <br /><br />
        <strong>∫𝒟</strong> dépend de <strong>π(t)</strong>, de la friction et des paramètres visibles. SoulRAM traite
        la pression mémoire, pas un gain marketing figé.
      </div>
    </details>
  </div>
</div>
