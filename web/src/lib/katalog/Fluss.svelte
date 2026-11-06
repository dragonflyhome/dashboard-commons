<script lang="ts">
	/** Fluss aus `daten/fluss.toml`: Kästen und Pfeile, Spalten nach längstem Weg. */
	import type { Abgeleitet, Fluss } from '$vertrag';
	import Flow from '#lib/components/blocks/Flow.svelte';
	import { fluss } from '#lib/daten.ts';
	import Daten from './intern/Daten.svelte';
	import type { Quelle } from './abgeleitet.ts';

	interface Props {
		daten: Quelle<Fluss>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
	}
	let { daten, teil, abgeleitet }: Props = $props();
</script>

<Daten {daten} {teil} {abgeleitet}>
	{#snippet children(f: Fluss)}
		<Flow data={fluss(f)} />
	{/snippet}
</Daten>
