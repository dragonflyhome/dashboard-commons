<script lang="ts">
	/** Fortschrittsbalken aus `daten/fortschritt.toml` (`prozent` oder `erreicht` von `gesamt`). */
	import type { Abgeleitet, Fortschritt } from '$vertrag';
	import Progress from '#lib/components/blocks/Progress.svelte';
	import { fortschritt } from '#lib/daten.ts';
	import Daten from './intern/Daten.svelte';
	import type { Quelle } from './abgeleitet.ts';

	interface Props {
		daten: Quelle<Fortschritt>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
	}
	let { daten, teil, abgeleitet }: Props = $props();
</script>

<Daten {daten} {teil} {abgeleitet}>
	{#snippet children(f: Fortschritt)}
		<Progress rows={fortschritt(f)} />
	{/snippet}
</Daten>
