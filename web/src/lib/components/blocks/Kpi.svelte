<script lang="ts">
	import type { KpiItem } from '$vertrag';
	import type { StandAnzeige } from '#lib/daten.ts';
	import StandHinweis from '$katalog/StandHinweis.svelte';
	import KpiValue from './KpiValue.svelte';
	import { kennzahlenText } from '#lib/textfassung.ts';
	import Textfassung from '../Textfassung.svelte';

	interface Props {
		/** Etappe B1: mit `stand` steht „Stand vom …“ unter der Kachel. */
		items: (KpiItem & { stand?: StandAnzeige | undefined })[];
		/** Textfassung gleich sichtbar (Katalog). */
		textOffen?: boolean;
	}
	let { items, textOffen = false }: Props = $props();
</script>

<div class="kpis">
	{#each items as k, i (i)}
		<div class="kpi">
			<div class="label muted small">{k.label}</div>
			<div class="value"><KpiValue value={k.value} /></div>
			{#if k.note}<div class="muted small">{k.note}</div>{/if}
			{#if k.stand}<div class="stand">
					<StandHinweis stand={k.stand.stand} alter={k.stand.alter} />
				</div>{/if}
		</div>
	{/each}
</div>
<!-- Etappe 3b: Textfassung mit den Endwerten (die Kacheln zählen sichtbar hoch). -->
<Textfassung fassung={kennzahlenText(items)} offen={textOffen} />

<style>
	.kpis {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
		gap: 10px;
		margin: 0.6em 0 1em;
	}
	.kpi {
		border: 1px solid var(--line);
		border-radius: 12px;
		padding: 10px 12px;
		background: color-mix(in srgb, var(--accent-soft) 35%, var(--card));
	}
	.stand {
		margin-top: 4px;
	}
	.value {
		font-size: var(--text-3xl);
		font-weight: 700;
		letter-spacing: -0.01em;
		font-variant-numeric: tabular-nums;
	}
</style>
