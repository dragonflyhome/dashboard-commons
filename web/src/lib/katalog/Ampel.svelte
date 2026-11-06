<script lang="ts">
	/** Ampel aus `daten/ampel.toml`: je Zeile Titel, Farbe (Bedeutung) und Hinweis. */
	import type { Abgeleitet, Ampel } from '$vertrag';
	import AmpelBlock from '#lib/components/blocks/Ampel.svelte';
	import { ampel } from '#lib/daten.ts';
	import Daten from './intern/Daten.svelte';
	import type { Quelle } from './abgeleitet.ts';

	interface Props {
		daten: Quelle<Ampel>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
	}
	let { daten, teil, abgeleitet }: Props = $props();
</script>

<Daten {daten} {teil} {abgeleitet}>
	{#snippet children(a: Ampel)}
		<AmpelBlock rows={ampel(a)} />
	{/snippet}
</Daten>
