<script lang="ts">
	/**
	 * Kleine Kurve des PC-Verlaufs: Prozessor und Arbeitsspeicher in Prozent,
	 * x nach Zeit. Unter zwei Werten gibt es keine Kurve, nur einen Satz.
	 */
	import type { PcMesswert } from '$vertrag';
	import { pcKurveText } from '#lib/textfassung.ts';
	import Textfassung from '../Textfassung.svelte';

	interface Props {
		/** Messwerte in Zeitreihenfolge, cpu und mem in Prozent 0 bis 100. */
		werte: PcMesswert[];
		/** Textfassung gleich sichtbar (Katalog). */
		textOffen?: boolean;
	}
	let { werte, textOffen = false }: Props = $props();

	const w = 600;
	const h = 140;
	const pad = 4;

	const zeiten = $derived(werte.map((m) => new Date(m.zeit).getTime()));
	const t0 = $derived(zeiten[0] ?? 0);
	const t1 = $derived(zeiten[zeiten.length - 1] ?? 0);

	function x(t: number): number {
		const span = t1 - t0;
		return span > 0 ? ((t - t0) / span) * w : 0;
	}
	function y(p: number): number {
		return h - pad - (Math.max(0, Math.min(100, p)) / 100) * (h - 2 * pad);
	}
	/** Ein Pfad; ein Wert `null` (Prozessor unbekannt, Etappe 3a+) unterbricht ihn: Lücke statt 0. */
	function pfad(feld: 'cpu' | 'mem'): string {
		const teile: string[] = [];
		let neu = true;
		werte.forEach((m, i) => {
			const wert = m[feld];
			if (wert === null) {
				neu = true;
				return;
			}
			const px = x(zeiten[i] ?? t0).toFixed(1);
			const py = y(wert).toFixed(1);
			teile.push(`${neu ? 'M' : 'L'} ${px} ${py}`);
			neu = false;
		});
		return teile.join(' ');
	}

	const cpuPfad = $derived(pfad('cpu'));
	const memPfad = $derived(pfad('mem'));

	function uhr(iso: string | undefined): string {
		if (iso === undefined) return '';
		return new Date(iso).toLocaleString('de-DE', { hour: '2-digit', minute: '2-digit' });
	}
	function mittel(feld: 'cpu' | 'mem'): number {
		const da = werte.flatMap((m) => (m[feld] === null ? [] : [m[feld]]));
		if (da.length === 0) return 0;
		return Math.round(da.reduce((n, w) => n + w, 0) / da.length);
	}
	const erste = $derived(uhr(werte[0]?.zeit));
	const letzte = $derived(uhr(werte[werte.length - 1]?.zeit));
	const zusammenfassung = $derived(
		`PC-Verlauf von ${erste} bis ${letzte} Uhr: Prozessor im Mittel ${mittel('cpu')} Prozent, ` +
			`Arbeitsspeicher im Mittel ${mittel('mem')} Prozent.`
	);
</script>

{#if werte.length === 0}
	<p class="muted">Noch keine Messwerte, der erste kommt kurz nach dem Start.</p>
{:else if werte.length < 2}
	<p class="muted">Erst ein Messwert da, die Kurve erscheint ab dem zweiten.</p>
{:else}
	<figure class="verlauf">
		<svg viewBox="0 0 {w} {h}" preserveAspectRatio="none" role="img" aria-label={zusammenfassung}>
			<title>{zusammenfassung}</title>
			<line class="mid" x1="0" x2={w} y1={y(50)} y2={y(50)} />
			<path class="cpu" d={cpuPfad} />
			<path class="mem" d={memPfad} />
		</svg>
		<div class="achse muted small" aria-hidden="true">
			<span>{erste}</span>
			<span>{letzte}</span>
		</div>
		<figcaption class="legende small">
			<span><i class="strich cpu-farbe"></i> Prozessor</span>
			<span><i class="strich mem-farbe"></i> Arbeitsspeicher</span>
			<span class="muted">0 bis 100 %</span>
		</figcaption>
	</figure>
	<!-- Etappe 3b: Textfassung mit Mittel, Spitze und Tiefst je Reihe. -->
	<Textfassung fassung={pcKurveText(werte)} offen={textOffen} />
{/if}

<style>
	.verlauf {
		margin: 0;
		min-width: 0;
	}
	svg {
		display: block;
		width: 100%;
		height: 140px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--line) 30%, transparent);
	}
	.mid {
		stroke: var(--line);
		stroke-dasharray: 3 4;
	}
	path {
		fill: none;
		stroke-width: 1.8;
		vector-effect: non-scaling-stroke;
	}
	.cpu {
		stroke: var(--accent);
	}
	.mem {
		stroke: var(--info);
	}
	.achse {
		display: flex;
		justify-content: space-between;
		margin-top: 2px;
	}
	.legende {
		display: flex;
		flex-wrap: wrap;
		gap: 14px;
		margin-top: 4px;
	}
	.strich {
		display: inline-block;
		width: 14px;
		height: 3px;
		border-radius: 2px;
		vertical-align: middle;
		margin-right: 4px;
	}
	.cpu-farbe {
		background: var(--accent);
	}
	.mem-farbe {
		background: var(--info);
	}
</style>
