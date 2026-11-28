<script lang="ts">
	/**
	 * Kalender, Stufe 7b (/heute): der Lauf der Sonne heute am Blickort als Bild.
	 * Waagrecht die Uhrzeit (0 bis 24 Uhr), senkrecht die Höhe über dem sichtbaren
	 * Horizont (Linie bei 0°). Der Bogen geht von Auf- bis Untergang und erreicht am
	 * wahren Mittag (Marke) `hoehe_mittag`; die Sonne steht bei der Uhrzeit `uhrzeit`
	 * auf `hoehe`, nachts unter der Linie. Darunter dieselben Werte als Sätze und die
	 * Textfassung. Polartag und Polarnacht zeigen den Satz des Servers.
	 */
	import { sonnenbogenText } from '#lib/textfassung.ts';
	import Textfassung from '../Textfassung.svelte';
	import type { KalenderKopf } from '$vertrag';

	interface Props {
		kopf: KalenderKopf;
		/** „HH:MM“, die Uhrzeit, zu der die Sonne gezeigt wird (die der Uhr auf /heute). */
		uhrzeit: string;
	}
	let { kopf, uhrzeit }: Props = $props();

	const BREITE = 360;
	const HORIZONT = 110;
	const RAND = 10;

	/**
	 * Minuten seit Mitternacht aus „HH:MM“, an Umstellungstagen mit Kürzel oder Versatz
	 * dahinter („07:01 CEST“, „07:01 +03:00“); ein anderer Tag („00:17 (am 27.7.)“) gibt `null`.
	 */
	function minuten(t: string | null): number | null {
		const m =
			t === null ? null : /^(\d{2}):(\d{2})(?: [A-Za-z]{2,6}| [+-]\d{2}(?::\d{2})?)?$/.exec(t);
		return m === null ? null : Number(m[1]) * 60 + Number(m[2]);
	}

	const x = (min: number): number => RAND + (min / 1440) * (BREITE - 2 * RAND);
	// Der höchste Wert füllt den Raum über der Linie; flache Tage werden nicht zur Treppe.
	const massstab = $derived(
		(HORIZONT - RAND) / Math.max(kopf.hoehe_mittag ?? 0, kopf.hoehe ?? 0, 10)
	);
	const y = (h: number): number => Math.min(144, Math.max(6, HORIZONT - h * massstab));

	const auf = $derived(minuten(kopf.aufgang));
	const unter = $derived(minuten(kopf.untergang));
	const mittag = $derived(minuten(kopf.mittag));
	const jetzt = $derived(minuten(uhrzeit));

	/** Der Bogen als Linienzug: links und rechts vom Mittag je ein Viertel einer Sinuswelle. */
	const bogen = $derived.by(() => {
		const hm = kopf.hoehe_mittag;
		if (auf === null || unter === null || mittag === null || hm === null) return null;
		if (!(auf < mittag && mittag < unter)) return null;
		const punkte: string[] = [];
		const schritte = 24;
		for (let i = 0; i <= schritte; i++) {
			const t = auf + ((mittag - auf) * i) / schritte;
			punkte.push(
				`${x(t).toFixed(1)},${y(hm * Math.sin((Math.PI / 2) * (i / schritte))).toFixed(1)}`
			);
		}
		for (let i = 1; i <= schritte; i++) {
			const t = mittag + ((unter - mittag) * i) / schritte;
			punkte.push(
				`${x(t).toFixed(1)},${y(hm * Math.sin((Math.PI / 2) * (1 - i / schritte))).toFixed(1)}`
			);
		}
		return punkte.join(' ');
	});

	/**
	 * Unter dem Horizont: der nächste Aufgang, nach dem Mittag der von morgen.
	 * shortcut: Ohne IANA-Zone zeigt die Uhr Rechnerzeit; bei einem Ort mit festem Versatz
	 * kann „heute“/„morgen“ dann um den Unterschied danebenliegen, mit 7c (Zeit des Orts) beheben.
	 */
	const nacht = $derived.by(() => {
		if (kopf.hoehe === null || kopf.hoehe >= 0 || jetzt === null) return null;
		const morgen = jetzt > (mittag ?? 720);
		const zeit = morgen ? kopf.aufgang_morgen : kopf.aufgang;
		if (zeit === null) return null;
		return `Die Sonne steht unter dem Horizont. Sie geht ${morgen ? 'morgen' : 'heute'} um ${zeit} auf.`;
	});

	const name = $derived(
		[
			'Sonnenbogen:',
			kopf.aufgang !== null ? `Aufgang ${kopf.aufgang},` : '',
			kopf.mittag !== null && kopf.hoehe_mittag !== null
				? `Mittag ${kopf.mittag} mit ${String(kopf.hoehe_mittag)}°,`
				: '',
			kopf.untergang !== null ? `Untergang ${kopf.untergang}.` : '',
			kopf.text ?? '',
			kopf.hoehe !== null ? `Um ${uhrzeit} steht die Sonne ${String(kopf.hoehe)}° hoch.` : ''
		]
			.join(' ')
			.replace(/\s+/g, ' ')
			.replace(/,\s*$/, '.')
			.trim()
	);
</script>

<figure class="sonnenbogen" data-sonnenbogen>
	<svg viewBox="0 0 {BREITE} 150" role="img" aria-label={name}>
		<line class="horizont" x1={RAND} x2={BREITE - RAND} y1={HORIZONT} y2={HORIZONT} />
		{#if bogen !== null}
			<polyline class="bogen" points={bogen} />
		{/if}
		{#if mittag !== null && kopf.hoehe_mittag !== null}
			<line
				class="marke"
				x1={x(mittag)}
				x2={x(mittag)}
				y1={HORIZONT}
				y2={y(kopf.hoehe_mittag)}
				data-mittag
			/>
		{/if}
		{#if jetzt !== null && kopf.hoehe !== null}
			<circle class="sonne" cx={x(jetzt)} cy={y(kopf.hoehe)} r="7" data-sonne />
		{/if}
	</svg>
	<figcaption>
		<p class="zeiten">
			{#if kopf.aufgang !== null}<span>Aufgang {kopf.aufgang}</span>{/if}
			{#if kopf.mittag !== null}<span
					>Mittag {kopf.mittag}{#if kopf.hoehe_mittag !== null}
						({kopf.hoehe_mittag}°){/if}</span
				>{/if}
			{#if kopf.untergang !== null}<span>Untergang {kopf.untergang}</span>{/if}
		</p>
		{#if kopf.text !== null}<p data-polar>{kopf.text}</p>{/if}
		{#if nacht !== null}<p data-nacht>{nacht}</p>{/if}
	</figcaption>
</figure>
<Textfassung fassung={sonnenbogenText(kopf, uhrzeit)} />

<style>
	.sonnenbogen {
		margin: 0;
	}
	svg {
		display: block;
		width: 100%;
		max-width: 36rem;
		height: auto;
	}
	.horizont {
		stroke: var(--vordergrund-leise);
		stroke-width: 1.5;
	}
	.bogen {
		fill: none;
		stroke: var(--reihe-3);
		stroke-width: 2;
	}
	.marke {
		stroke: var(--vordergrund-leise);
		stroke-width: 1;
		stroke-dasharray: 3 3;
	}
	.sonne {
		fill: var(--reihe-3);
		stroke: var(--vordergrund);
		stroke-width: 1;
	}
	.zeiten {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 1rem;
		margin: 0.4rem 0 0;
		font-variant-numeric: tabular-nums;
	}
	figcaption p {
		margin: 0.4rem 0 0;
	}
</style>
