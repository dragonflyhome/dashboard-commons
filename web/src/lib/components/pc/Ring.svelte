<script lang="ts">
	import { Tween } from 'svelte/motion';
	import { cubicOut } from 'svelte/easing';
	import type { Belegung } from '$vertrag';
	import { klasse } from '#lib/bedeutung.ts';
	import { belegungBedeutung } from '#lib/pc.ts';
	import { ringText } from '#lib/textfassung.ts';
	import Textfassung from '../Textfassung.svelte';

	interface Props {
		/** 0..100, nur für Bogen und Zahl. */
		percent: number;
		/**
		 * Stufe vom Server (Schwellen in `belegung_schwellen`); der Ring wählt
		 * nur die Farbe. `unbekannt` ist grau, nie grün.
		 */
		belegung: Belegung;
		label: string;
		/**
		 * Wert unter dem Ring, z. B. „12 GB / 32 GB“. In der Mitte steht nur die
		 * Prozentzahl: längerer Text lief dort über den Bogen.
		 */
		value: string;
		/** Weitere Zeile unter dem Ring. */
		detail?: string;
		/** Textfassung gleich sichtbar (Katalog). */
		textOffen?: boolean;
	}
	let { percent, belegung, label, value, detail, textOffen = false }: Props = $props();

	const r = 42;
	const c = 2 * Math.PI * r;
	const shown = new Tween(0, { duration: 600, easing: cubicOut });
	$effect(() => {
		void shown.set(Math.max(0, Math.min(100, percent)));
	});
	const cls = $derived(klasse(belegungBedeutung(belegung)));
</script>

<div class="ring-rahmen">
	<div
		class="ring"
		role="img"
		aria-label="{label}: {Math.round(percent)} Prozent, {value}"
		data-belegung={belegung}
	>
		<svg viewBox="0 0 100 100" width="104" height="104">
			<circle class="bg" cx="50" cy="50" {r} />
			<circle
				class="fg {cls}"
				cx="50"
				cy="50"
				{r}
				stroke-dasharray={c}
				stroke-dashoffset={c * (1 - shown.current / 100)}
				transform="rotate(-90 50 50)"
			/>
			<text class="pct" x="50" y="50" text-anchor="middle" dominant-baseline="central"
				>{Math.round(shown.current)}%</text
			>
		</svg>
		<div class="label">{label}</div>
		<div class="val muted small">{value}</div>
		{#if detail}<div class="muted small">{detail}</div>{/if}
	</div>
	<!-- Etappe 3b: Textfassung neben dem Bild (in role=img hörte sie niemand); Wert und Name
     stehen schon als Text da, darum ohne Knopf. -->
	<Textfassung
		fassung={ringText({ label, percent, value, belegung, detail })}
		umschalter={false}
		offen={textOffen}
	/>
</div>

<style>
	.ring-rahmen {
		min-width: 0;
	}
	.ring {
		display: grid;
		justify-items: center;
		text-align: center;
	}
	.bg {
		fill: none;
		stroke: color-mix(in srgb, var(--line) 80%, transparent);
		stroke-width: 9;
	}
	.fg {
		fill: none;
		stroke-width: 9;
		stroke-linecap: round;
		/* Ton aus der Klasse der Bedeutung (app.css, bedeutung.ts). */
		stroke: var(--ton, var(--ok));
		transition: stroke 300ms ease;
	}
	.pct {
		font-size: var(--text-grafik);
		font-weight: 700;
		fill: var(--text);
	}
	.val {
		overflow-wrap: anywhere;
	}
	.label {
		font-weight: 600;
		margin-top: 2px;
	}
</style>
