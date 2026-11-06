<script lang="ts">
	let { plan, daten }: { plan: unknown; daten: unknown } = $props();
</script>

<div class="col-2">{String(plan)} {String(daten)}</div>
