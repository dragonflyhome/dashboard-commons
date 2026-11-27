<script lang="ts">
	/**
	 * Jahreskreis (Kalender, Stufe 6): das Festjahr als Kreis, oben die Wende im
	 * Winter (Yule). Innen die vier Jahreszeiten als Flächen, außen die Mondmonate
	 * als Bögen mit Nummer und Namen, der laufende hervorgehoben, Vollmonde als
	 * Punkte (die nächsten drei mit Ring), die Sonnentage als Marken und ein
	 * Zeiger für den Tag. Daneben dieselben Angaben als Liste; die Tabelle für
	 * Vorlese-Programme baut `jahreskreisText`. Ohne Bewegung, Farben nur aus Tokens.
	 */
	import type { KalenderJahr } from '$vertrag';
	import { fmtDate } from '#lib/format.ts';
	import {
		festjahrName,
		jahreskreisText,
		laufenderMonat,
		vollmondeImKreis
	} from '#lib/textfassung.ts';
	import { tagZahl } from '#lib/zeit/tage.ts';
	import Textfassung from '../Textfassung.svelte';

	interface Props {
		jahr: KalenderJahr;
		/** Der Tag des Zeigers (JJJJ-MM-TT). */
		tag: string;
		/** Textfassung gleich sichtbar (Katalog). */
		textOffen?: boolean;
	}
	let { jahr, tag, textOffen = false }: Props = $props();
	const id = $props.id();

	const M = 200;
	/** Halbmesser: Jahreszeiten innen, Marken der Sonnentage, Vollmonde, Monatsband, Namen. */
	const R = { mitte: 36, zeiten: 104, marke: 122, voll: 130, band: 150, name: 178 };
	/** Klasse je Wort der Jahreszeit (`jahreszeit` eines Monats ohne „vollmond“). */
	const KLASSE: Record<string, string> = {
		Winter: 'winter',
		Frühling: 'fruehling',
		Sommer: 'sommer',
		Herbst: 'herbst'
	};

	const nullTag = $derived(
		jahr.sonnentage.find((s) => s.kennung === 'wende-winter')?.datum ?? jahr.zeitraum.anfang
	);
	const tage = $derived(tagZahl(jahr.zeitraum.ende) - tagZahl(jahr.zeitraum.anfang));

	/** Winkel eines Tags in Grad, im Uhrzeigersinn ab oben (der Wende im Winter). */
	function winkel(t: string): number {
		return ((tagZahl(t) - tagZahl(nullTag)) / tage) * 360;
	}
	function lage(w: number, r: number): { x: string; y: string } {
		const a = ((w - 90) * Math.PI) / 180;
		return { x: (M + r * Math.cos(a)).toFixed(1), y: (M + r * Math.sin(a)).toFixed(1) };
	}
	function punkt(w: number, r: number): string {
		const p = lage(w, r);
		return `${p.x} ${p.y}`;
	}
	/** Mittelpunkt eines Kreises auf dem Kreis. */
	function kreis(w: number, r: number): { cx: string; cy: string } {
		const p = lage(w, r);
		return { cx: p.x, cy: p.y };
	}
	/** Bogen von `w1` nach `w2` im Uhrzeigersinn; `rueck`: gegen ihn (Schrift unten bleibt lesbar). */
	function bogen(w1: number, w2: number, r: number, rueck = false): string {
		const gross = w2 - w1 > 180 ? 1 : 0;
		return rueck
			? `M ${punkt(w2, r)} A ${String(r)} ${String(r)} 0 ${String(gross)} 0 ${punkt(w1, r)}`
			: `M ${punkt(w1, r)} A ${String(r)} ${String(r)} 0 ${String(gross)} 1 ${punkt(w2, r)}`;
	}
	/** Ringstück zwischen `r1` und `r2`. */
	function sektor(w1: number, w2: number, r1: number, r2: number): string {
		const gross = w2 - w1 > 180 ? 1 : 0;
		return `M ${punkt(w1, r2)} A ${String(r2)} ${String(r2)} 0 ${String(gross)} 1 ${punkt(w2, r2)} L ${punkt(w2, r1)} A ${String(r1)} ${String(r1)} 0 ${String(gross)} 0 ${punkt(w1, r1)} Z`;
	}
	/** Kurzer Name im Ring: der Teil nach „ · “, ohne Klammer und ohne „vollmond“. */
	function kurz(name: string): string {
		return (name.split(' · ').at(-1) ?? name).replace(/\s*\(.*\)$/, '').replace(/s?vollmond$/, '');
	}

	const laufend = $derived(laufenderMonat(jahr, tag));
	const naechste = $derived(jahr.naechste_vollmonde);
	const ringe = $derived(vollmondeImKreis(jahr));
	/**
	 * Die Jahreszeiten als Flächen nach den Monaten, die der Dienst ihnen zuordnet: Jede
	 * beginnt mit dem Neumond nach ihrem Festmonat, nicht am Sonnentag. Läuft dieselbe
	 * Jahreszeit über den Anfang des Festjahrs, wird sie eine Fläche.
	 */
	const zeiten = $derived.by(() => {
		const z: {
			wort: string;
			klasse: string;
			von: number;
			bis: number;
			ab: string;
			bisTag: string;
		}[] = [];
		for (const m of jahr.monate) {
			const wort = (m.jahreszeit ?? '').replace(/s?vollmond$/, '');
			const letzte = z.at(-1);
			if (letzte?.wort === wort) {
				letzte.bis = winkel(m.zeitraum.ende);
				letzte.bisTag = m.zeitraum.ende;
			} else
				z.push({
					wort,
					klasse: KLASSE[wort] ?? '',
					von: winkel(m.zeitraum.anfang),
					bis: winkel(m.zeitraum.ende),
					ab: m.zeitraum.anfang,
					bisTag: m.zeitraum.ende
				});
		}
		const [erste, letzte] = [z[0], z.at(-1)];
		if (z.length > 1 && erste && erste.wort === letzte?.wort) {
			erste.von = letzte.von - 360;
			erste.ab = letzte.ab;
			z.pop();
		}
		return z.filter((x) => x.wort !== '');
	});
	const beschreibung = $derived(
		`Jahreskreis ${festjahrName(jahr)}, ${
			jahr.monate.length > 0 ? `${String(jahr.monate.length)} Mondmonate` : 'ohne Mondmonate'
		}, am ${fmtDate(tag)}${laufend ? ` im ${String(laufend.nummer)}. Monat (${laufend.name})` : ''}`
	);
</script>

<div class="jahreskreis">
	<div class="bild">
		<svg viewBox="0 0 400 400" role="img" aria-label={beschreibung}>
			{#each zeiten as z (z.klasse + String(z.von))}
				<path
					class="zeit {z.klasse}"
					data-von={z.ab}
					data-bis={z.bisTag}
					d={sektor(z.von, z.bis, R.mitte, R.zeiten)}
				/>
				<text class="wort" {...lage((z.von + z.bis) / 2, (R.mitte + R.zeiten) / 2)}>{z.wort}</text>
			{/each}
			{#each jahr.monate as m (m.nummer)}
				{@const w1 = winkel(m.zeitraum.anfang) + 1.2}
				{@const w2 = winkel(m.zeitraum.ende) - 1.2}
				{@const mitte = (w1 + w2) / 2}
				{@const unten = ((mitte % 360) + 360) % 360 > 90 && ((mitte % 360) + 360) % 360 < 270}
				<path class="monat" class:laufend={m === laufend} d={bogen(w1, w2, R.band)} />
				<text class="nummer" class:laufend={m === laufend} {...lage(mitte, R.band)}>{m.nummer}</text
				>
				<path id="{id}-{m.nummer}" class="leer" d={bogen(w1, w2, R.name, unten)} />
				<text class="name"
					><textPath href="#{id}-{m.nummer}" startOffset="50%">{kurz(m.name)}</textPath></text
				>
				<circle class="voll" {...kreis(winkel(m.zeitraum.vollmond), R.voll)} r="3.5" />
			{/each}
			{#each ringe as v (v.datum)}
				<circle class="naechster" {...kreis(winkel(v.datum), R.voll)} r="7" />
			{/each}
			{#each jahr.sonnentage as s (s.kennung)}
				<path
					class="marke"
					d="M {punkt(winkel(s.datum), R.zeiten)} L {punkt(winkel(s.datum), R.marke)}"
				/>
			{/each}
			<path
				class="zeiger"
				d="M {punkt(winkel(tag), R.mitte)} L {punkt(winkel(tag), R.name - 12)}"
			/>
			<circle class="mitte" cx={M} cy={M} r={R.mitte} />
			<text class="jahr" x={M} y={M}>{festjahrName(jahr)}</text>
		</svg>
		<!-- Beide Ringe folgen den Mondmonaten: innen die Jahreszeiten, außen die Monate; ein Festmonat wie „Imbolc“ gehört noch zur Jahreszeit davor. -->
		<p class="muted small" data-legende>Innen die Jahreszeiten, außen die Mondmonate.</p>
	</div>
	<div class="liste">
		{#if jahr.hinweis}<p class="muted small">{jahr.hinweis}</p>{/if}
		{#if jahr.monate.length > 0}
			<h3>Mondmonate</h3>
			<ol class="plain">
				{#each jahr.monate as m (m.nummer)}
					<li class:laufend={m === laufend} aria-current={m === laufend ? 'date' : undefined}>
						<b>{m.nummer}.</b>
						{m.name}
						<span class="muted small"
							>{fmtDate(m.zeitraum.anfang)} bis {fmtDate(m.zeitraum.ende)}, Vollmond {fmtDate(
								m.zeitraum.vollmond
							)}</span
						>
					</li>
				{/each}
			</ol>
		{/if}
		{#if jahr.sonnentage.length > 0}
			<h3>Sonnentage</h3>
			<ol class="plain">
				{#each jahr.sonnentage as s (s.kennung)}
					<li>{s.name} <span class="muted small">{fmtDate(s.datum)}, {s.zeit} Uhr</span></li>
				{/each}
			</ol>
		{/if}
		{#if naechste.length > 0}
			<h3>Nächste Vollmonde</h3>
			<ol class="plain">
				{#each naechste as v (v.datum)}
					<li>{fmtDate(v.datum)} <span class="muted small">{v.zeit} Uhr</span></li>
				{/each}
			</ol>
		{/if}
	</div>
	<Textfassung fassung={jahreskreisText(jahr, tag)} umschalter={false} offen={textOffen} />
</div>

<style>
	.jahreskreis {
		display: grid;
		gap: 0.75rem 1.5rem;
		align-items: start;
	}
	.bild svg {
		display: block;
		width: 100%;
		max-width: 26rem;
		margin: 0 auto;
	}
	@media (min-width: 48rem) {
		.jahreskreis {
			grid-template-columns: minmax(0, 26rem) minmax(0, 1fr);
		}
	}
	text {
		font-size: var(--text-grafik-klein);
		fill: var(--text);
		text-anchor: middle;
		dominant-baseline: central;
	}
	.zeit.winter {
		fill: var(--info-soft);
	}
	.zeit.fruehling {
		fill: var(--ok-soft);
	}
	.zeit.sommer {
		fill: var(--warn-soft);
	}
	.zeit.herbst {
		fill: var(--plan-soft);
	}
	.monat {
		fill: none;
		stroke: var(--line);
		stroke-width: 20;
	}
	.monat.laufend {
		stroke: var(--accent);
	}
	.nummer.laufend {
		fill: var(--on-accent);
		font-weight: 700;
	}
	.leer {
		fill: none;
	}
	.voll {
		fill: var(--text);
	}
	.naechster {
		fill: none;
		stroke: var(--accent);
		stroke-width: 2;
	}
	.marke {
		stroke: var(--muted);
		stroke-width: 3;
	}
	.zeiger {
		stroke: var(--accent-text);
		stroke-width: 2.5;
		stroke-linecap: round;
	}
	.mitte {
		fill: var(--card);
		stroke: var(--line);
	}
	.jahr {
		font-weight: 600;
	}
	.liste h3 {
		margin: 0.6em 0 0.2em;
		font-size: var(--text-md);
	}
	.liste ol {
		display: grid;
		gap: 0.2rem;
	}
	.liste li {
		overflow-wrap: anywhere;
	}
	.liste li.laufend {
		font-weight: 600;
	}
	/* Schmal: im Ring nur die Nummern, die Namen stehen in der Liste. */
	@media (max-width: 30rem) {
		.name {
			display: none;
		}
	}
</style>
