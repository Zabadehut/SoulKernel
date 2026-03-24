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

<main>
  <PanelLeftHardware />
  <PanelCenterFormula />
  <PanelWorkloads />
  <PanelInsights />
  <PanelRightColumn />
  <PanelBottomBar />
</main>

<BenchmarkSection />
