<script>
  import NoTauriBanner from './NoTauriBanner.svelte';
  import AppHeader from './AppHeader.svelte';
  import MetricsStrip from './MetricsStrip.svelte';
  import CompactHud from './CompactHud.svelte';
  import PanelLeftHardware from './PanelLeftHardware.svelte';
  import PanelCenterFormula from './PanelCenterFormula.svelte';
  import PanelWorkloads from './PanelWorkloads.svelte';
  import PanelInsights from './PanelInsights.svelte';
  import PanelRightColumn from './PanelRightColumn.svelte';
  import PanelBottomBar from './PanelBottomBar.svelte';
  import BenchmarkSection from './BenchmarkSection.svelte';
</script>

<NoTauriBanner />
<AppHeader />
<MetricsStrip />
<CompactHud />

<main class="shell-grid">
  <section class="shell-column shell-column-left" aria-label="Audit matériel">
    <PanelLeftHardware />
  </section>
  <section class="shell-column shell-column-main" aria-label="Décision et preuves">
    <PanelCenterFormula />
    <PanelWorkloads />
    <PanelInsights />
  </section>
  <section class="shell-column shell-column-right" aria-label="Pilotage et actions">
    <PanelRightColumn />
  </section>
  <section class="shell-footer">
    <PanelBottomBar />
  </section>
</main>

<BenchmarkSection />
