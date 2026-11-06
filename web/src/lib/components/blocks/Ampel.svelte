<script lang="ts">
	import type { AmpelRow } from '$vertrag';
	import type { StandAnzeige } from '#lib/daten.ts';
	import StandHinweis from '$katalog/StandHinweis.svelte';
	import StatusDot from '../StatusDot.svelte';

	interface Props {
		/** Etappe B1: mit `stand` steht „Stand vom …“ hinter der Zeile. */
		rows: (AmpelRow & { stand?: StandAnzeige | undefined })[];
	}
	let { rows }: Props = $props();
</script>

<ul class="plain ampel">
	{#each rows as r, i (i)}
		<li>
			<StatusDot bedeutung={r.bedeutung} /> <b>{r.label}</b>{#if r.note}<span class="muted">
					· {r.note}</span
				>{/if}{#if r.stand}
				<StandHinweis stand={r.stand.stand} alter={r.stand.alter} />{/if}
		</li>
	{/each}
</ul>

<style>
	.ampel {
		margin: 0.6em 0 1em;
		display: grid;
		gap: 4px;
	}
</style>
