<script lang="ts">
  import { onMount } from 'svelte';
  import SoulKernelShell from './shell/SoulKernelShell.svelte';

  onMount(() => {
    void import('./legacy/bootstrap').then((m) => m.bootSoulKernelApp());
  });
</script>

<div class="sk-legacy-root">
  <SoulKernelShell />
</div>
