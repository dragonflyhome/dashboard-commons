<script lang="ts">
	import { sparkText } from '#lib/textfassung.ts';
	import Textfassung from '../Textfassung.svelte';

	interface Props {
		/** Werte 0..100, älteste zuerst. */
		values: number[];
		label: string;
		/** Mindestens so viele Punkte breit zeichnen (ruhigere Kurve beim Füllen). */
		slots?: number;
		/** Textfassung gleich sichtbar (Katalog). */
		textOffen?: boolean;
	}
	let { values, label, slots = 180, textOffen = false }: Props = $props();

	const w = 300;
	const h = 60;
	const n = $derived(Math.max(slots, values.length));
	const points = $derived(
		values.map((v, i) => {
			const x = ((i + (n - values.length)) / Math.max(n - 1, 1)) * w;
			const y = h - (Math.max(0, Math.min(100, v)) / 100) * (h - 4) - 2;
			return [x, y] as const;
		})
	);
	const line = $derived(
		points.map(([x, y], i) => `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`).join(' ')
	);
	const area = $derived(
		points.length > 1
			? `${line} L ${points[points.length - 1]?.[0].toFixed(1) ?? w} ${h} L ${points[0]?.[0].toFixed(1) ?? 0} ${h} Z`
			: ''
	);
	const last = $derived(values[values.length - 1] ?? 0);
	const peak = $derived(values.length ? Math.max(...values) : 0);
</script>

<div class="spark-rahmen">
	<div
		class="spark"
		role="img"
		aria-label="{label}: zuletzt {Math.round(last)} Prozent, Spitze {Math.round(peak)} Prozent"
	>
		<div class="head">
			<span class="label">{label}</span>
			<span class="muted small">jetzt {Math.round(last)} % · Spitze {Math.round(peak)} %</span>
		</div>
		<svg viewBox="0 0 {w} {h}" preserveAspectRatio="none">
			<line class="mid" x1="0" x2={w} y1={h / 2} y2={h / 2} />
			{#if area}<path class="area" d={area} />{/if}
			{#if line}<path class="line" d={line} />{/if}
			{#if points.length}
				{@const p = points[points.length - 1]}
				{#if p}<circle class="now" cx={p[0]} cy={p[1]} r="3" />{/if}
			{/if}
		</svg>
	</div>
	<!-- Etappe 3b: Textfassung neben dem Bild (in role=img hörte sie niemand). -->
	<Textfassung fassung={sparkText(values, label)} offen={textOffen} />
</div>

<style>
	.spark-rahmen,
	.spark {
		min-width: 0;
	}
	.head {
		display: flex;
		justify-content: space-between;
		gap: 8px;
		font-size: var(--text-md);
	}
	.label {
		font-weight: 600;
	}
	svg {
		display: block;
		width: 100%;
		height: 60px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--line) 30%, transparent);
	}
	.mid {
		stroke: var(--line);
		stroke-dasharray: 3 4;
	}
	.area {
		fill: color-mix(in srgb, var(--accent) 22%, transparent);
	}
	.line {
		fill: none;
		stroke: var(--accent);
		stroke-width: 1.8;
		vector-effect: non-scaling-stroke;
	}
	.now {
		fill: var(--accent);
	}
</style>
