<script lang="ts">
	/**
	 * Die gemeinsame Zeitachse (Etappe 3a-1): Marken und Beschriftungen für
	 * einen Ausschnitt, ohne dass sich zwei Beschriftungen überdecken, dazu
	 * die Heute-Linie. Dichtestreifen und Gantt stellen sie über ihren Inhalt;
	 * die Breite misst der Baustein selbst und reicht sie hierher, damit Achse
	 * und Inhalt dieselbe Rechnung haben.
	 */
	import {
		beschriften,
		marken,
		passendeStufe,
		position,
		type Fenster,
		type Stufe
	} from '#lib/zeit/achse.ts';

	interface Props {
		fenster: Fenster;
		/** Breite in Pixeln (vom Baustein gemessen). */
		breite: number;
		/** Ohne Angabe: die Stufe, die zum Ausschnitt passt. */
		stufe?: Stufe | undefined;
		/** Plan-Monate statt Tagen. */
		plan?: boolean;
		/** Heute als Tageszahl; `null`: keine Linie. */
		heute?: number | null;
		/** Höhe der Linien unter der Beschriftung (für Gantt über alle Zeilen), in rem. */
		linienHoehe?: number;
	}
	let { fenster, breite, stufe, plan = false, heute = null, linienHoehe = 0.5 }: Props = $props();

	const gewaehlt = $derived(stufe ?? passendeStufe(fenster, breite));
	const liste = $derived(beschriften(marken(fenster, gewaehlt, plan), fenster, breite));
	const heuteX = $derived(
		heute === null || heute < fenster.von || heute > fenster.bis
			? null
			: position(fenster, breite, heute)
	);
</script>

<div
	class="achse"
	data-zeit="achse"
	data-stufe={gewaehlt}
	aria-hidden="true"
	style:--linien="{linienHoehe}rem"
>
	{#each liste as m (m.at)}
		<span class="strich" class:haupt={m.haupt} style:left="{m.x}px"></span>
		{#if m.sichtbar}
			<span class="text" class:haupt={m.haupt} style:left="{m.x + 3}px" data-beschriftung
				>{m.text}</span
			>
		{/if}
	{/each}
	{#if heuteX !== null}
		<span class="heute" style:left="{heuteX}px" data-heute-linie></span>
	{/if}
</div>

<style>
	.achse {
		position: relative;
		height: calc(1.25rem + var(--linien));
		overflow: hidden;
		font-size: var(--text-2xs);
		line-height: 1rem;
		color: var(--muted);
		user-select: none;
	}
	.strich {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 0.0625rem;
		background: var(--line);
	}
	.strich.haupt {
		background: color-mix(in srgb, var(--muted) 55%, var(--line));
	}
	.text {
		position: absolute;
		top: 0;
		white-space: nowrap;
	}
	.text.haupt {
		color: var(--text);
		font-weight: 600;
	}
	.heute {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 0.125rem;
		margin-left: -0.0625rem;
		background: var(--accent);
	}
</style>
