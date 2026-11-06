/**
 * Kontrast der Token (Etappe 3a-2): liest src/app.css, rechnet die Farben
 * beider Themen aus (Hex, var(), color-mix in srgb, transparent) und prüft
 * die wichtigen Paare nach WCAG 2.2: Text ab 4,5:1, Teile der Oberfläche
 * (Punkte, Balken, Ränder mit Bedeutung, Fokusrahmen) ab 3:1.
 *
 * Bewusst nicht geprüft: --line (Trennlinien, Schmuck).
 *
 * Seit dem Prüfbericht dazu die Bedeutungen so, wie die Oberfläche
 * sie benutzt (unten, „Bedeutungen in Gebrauch“): je Klasse .b-… aus
 * bedeutung.ts der Ton auf Karte, Seite und eigener Fläche (ab 3:1, auch
 * --plan), Text auf der Fläche und die Textfarbe der Bedeutung (ab 4,5:1).
 *
 * KONTRAST_ZEIGEN=1 npx vitest run src/kontrast.spec.ts gibt die Tabelle aus.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { Bedeutung } from '$vertrag';
import { BEDEUTUNGEN, klasse } from '#lib/bedeutung.ts';

type Rgba = [number, number, number, number];
type Token = Record<string, string>;

const CSS = readFileSync(join(import.meta.dirname, 'app.css'), 'utf8').replace(
	/\/\*[\s\S]*?\*\//g,
	''
);

/** Inhalt des Blocks ab `kopf` bis zur passenden schließenden Klammer. */
function block(text: string, kopf: RegExp): string {
	const m = kopf.exec(text);
	if (m === null) throw new Error(`Block ${String(kopf)} fehlt in app.css`);
	let tiefe = 1;
	const start = m.index + m[0].length;
	for (let i = start; i < text.length; i++) {
		if (text[i] === '{') tiefe++;
		if (text[i] === '}' && --tiefe === 0) return text.slice(start, i);
	}
	throw new Error('Block ohne Ende');
}

function token(inhalt: string): Token {
	const t: Token = {};
	for (const m of inhalt.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) {
		const [, name = '', wert = ''] = m;
		t[name] = wert.trim().replace(/\s+/g, ' ');
	}
	return t;
}

const HELL = token(block(CSS, /^:root,\s*\[data-thema-vorschau\]\s*\{/m));
/** Dunkles System ohne Wahl (und nicht „Hell“ gewählt). */
const DUNKEL_NUR_BLOCK = token(
	block(
		block(CSS, /@media \(prefers-color-scheme: dark\)\s*\{/),
		/:root:not\(\[data-theme='hell'\]\)\s*\{/
	)
);
const DUNKEL: Token = { ...HELL, ...DUNKEL_NUR_BLOCK };
/**
 * Etappe 3b, Schalter System/Hell/Dunkel: „Hell“ gewählt (bei jedem System)
 * nimmt :root und den Block `[data-theme='hell']`, „Dunkel“ gewählt den Block
 * `[data-theme='dunkel']` über :root. Gleiche Werte, also gleiche Kontraste.
 */
const HELL_BLOCK = token(
	block(CSS, /^:root\[data-theme='hell'\],\s*\[data-thema-vorschau='hell'\]\s*\{/m)
);
const DUNKEL_GEWAEHLT_BLOCK = token(
	block(CSS, /^:root\[data-theme='dunkel'\],\s*\[data-thema-vorschau='dunkel'\]\s*\{/m)
);
const HELL_GEWAEHLT: Token = { ...HELL, ...HELL_BLOCK };
const DUNKEL_GEWAEHLT: Token = { ...HELL, ...DUNKEL_GEWAEHLT_BLOCK };

function hex(h: string): Rgba {
	let s = h.slice(1);
	if (s.length <= 4) s = s.replace(/./g, (c) => c + c);
	const n = (i: number): number => parseInt(s.slice(i, i + 2), 16) / 255;
	return [n(0), n(2), n(4), s.length === 8 ? n(6) : 1];
}

/** Zerlegt „a, b, c“ auf oberster Ebene (Kommas in Klammern zählen nicht). */
function teile(s: string): string[] {
	const out: string[] = [];
	let tiefe = 0;
	let akt = '';
	for (const c of s) {
		if (c === '(') tiefe++;
		if (c === ')') tiefe--;
		if (c === ',' && tiefe === 0) {
			out.push(akt.trim());
			akt = '';
		} else akt += c;
	}
	out.push(akt.trim());
	return out;
}

/** Farbe eines Werts in einem Thema. */
function farbe(wert: string, t: Token): Rgba {
	const w = wert.trim();
	if (w === 'transparent') return [0, 0, 0, 0];
	if (w.startsWith('#')) return hex(w);
	const v = /^var\((--[a-z0-9-]+)\)$/.exec(w);
	if (v !== null) {
		const name = v[1] ?? '';
		const ziel = t[name];
		if (ziel === undefined) throw new Error(`Token ${name} fehlt`);
		return farbe(ziel, t);
	}
	const mix = /^color-mix\(in srgb,(.*)\)$/.exec(w);
	if (mix !== null) {
		const [a = '', b = ''] = teile(mix[1] ?? '');
		const anteil = (x: string): [string, number | null] => {
			const p = /^(.*?)\s+([\d.]+)%$/.exec(x);
			return p === null ? [x, null] : [p[1] ?? '', Number(p[2]) / 100];
		};
		const [fa, pa] = anteil(a);
		const [fb, pb] = anteil(b);
		const p = pa ?? (pb === null ? 0.5 : 1 - pb);
		const ca = farbe(fa, t);
		const cb = farbe(fb, t);
		// Mischung mit vormultipliziertem Alpha, wie color-mix im Browser.
		const alpha = ca[3] * p + cb[3] * (1 - p);
		if (alpha === 0) return [0, 0, 0, 0];
		const k = (x: number, y: number): number => (x * ca[3] * p + y * cb[3] * (1 - p)) / alpha;
		return [k(ca[0], cb[0]), k(ca[1], cb[1]), k(ca[2], cb[2]), alpha];
	}
	throw new Error(`Farbe nicht lesbar: ${w}`);
}

/** Vordergrund über Hintergrund gelegt (Hintergrund wird deckend gemacht). */
function ueber(vorne: Rgba, hinten: Rgba): Rgba {
	const a = vorne[3];
	const k = (x: number, y: number): number => x * a + y * (1 - a);
	return [k(vorne[0], hinten[0]), k(vorne[1], hinten[1]), k(vorne[2], hinten[2]), 1];
}

function leuchtdichte(c: Rgba): number {
	const lin = (x: number): number => (x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4);
	return 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
}

/** Ein Token (`--ok`) oder ein ganzer Wert (`color-mix(…)`). */
const wert = (x: string): string => (x.startsWith('--') ? `var(${x})` : x);

/**
 * Kontrast von `vorne` auf Fläche `flaeche` (Token oder Wert); durchscheinende
 * Flächen liegen auf `grund`.
 */
function kontrast(t: Token, vorne: string, flaeche: string, grund = '--card'): number {
	const unten = ueber(farbe(wert(flaeche), t), farbe(wert(grund), t));
	const oben = ueber(farbe(wert(vorne), t), unten);
	const lo = leuchtdichte(oben);
	const lu = leuchtdichte(unten);
	return (Math.max(lo, lu) + 0.05) / (Math.min(lo, lu) + 0.05);
}

/** [vorne, Fläche, Mindestwert, wofür] */
const PAARE: [string, string, number, string][] = [
	['--text', '--bg', 4.5, 'Text auf der Seite'],
	['--text', '--card', 4.5, 'Text auf Karten'],
	['--muted', '--bg', 4.5, 'Nebentext auf der Seite'],
	['--muted', '--card', 4.5, 'Nebentext auf Karten'],
	['--text', '--code-bg', 4.5, 'Code auf Karten'],
	['--text', '--accent-soft', 4.5, 'Text auf gewähltem Knopf'],
	['--accent', '--card', 4.5, 'Links auf Karten'],
	['--accent', '--bg', 4.5, 'Links auf der Seite'],
	[
		'--accent-text',
		'--accent-soft',
		4.5,
		'Aktiver Seitenlink eines Projekts (Akzent auf heller Akzentfläche)'
	],
	['--on-accent', '--accent', 4.5, 'Knopf in Akzentfarbe'],
	['--on-warn', '--warn', 4.5, 'Zähler auf Gelb'],
	['--ok-text', '--card', 4.5, 'Text „in Ordnung“'],
	['--ok-text', '--ok-soft', 4.5, 'Text „in Ordnung“ auf eigener Fläche'],
	['--warn-text', '--card', 4.5, 'Text „wartet“'],
	['--warn-text', '--warn-soft', 4.5, 'Text „wartet“ auf eigener Fläche'],
	['--bad-text', '--card', 4.5, 'Text „Problem“'],
	['--bad-text', '--bad-soft', 4.5, 'Text „Problem“ auf eigener Fläche'],
	['--info-text', '--card', 4.5, 'Text „Hinweis“'],
	['--info-text', '--info-soft', 4.5, 'Text „Hinweis“ auf eigener Fläche'],
	['--text', '--ok-soft', 4.5, 'Text auf grüner Fläche'],
	['--text', '--warn-soft', 4.5, 'Text auf gelber Fläche'],
	['--text', '--bad-soft', 4.5, 'Text auf roter Fläche'],
	['--text', '--info-soft', 4.5, 'Text auf türkiser Fläche (läuft)'],
	['--text', '--done-soft', 4.5, 'Text auf blauer Fläche (Hinweis, Hinweis.svelte)'],
	['--text', '--plan-soft', 4.5, 'Text auf grauer Fläche (neutral)'],
	['--ok', '--card', 3, 'Punkt/Balken „in Ordnung“'],
	['--warn', '--card', 3, 'Punkt/Balken „wartet“'],
	['--bad', '--card', 3, 'Punkt/Balken „Problem“'],
	['--info', '--card', 3, 'Punkt/Balken „läuft“'],
	['--done', '--card', 3, 'Punkt/Balken „Hinweis“'],
	['--fokus', '--card', 3, 'Fokusrahmen auf Karten'],
	['--fokus', '--bg', 3, 'Fokusrahmen auf der Seite']
];

/** [Name, Token, Grundthema für die festgehaltenen Messwerte] */
const THEMEN: [string, Token, 'hell' | 'dunkel'][] = [
	['hell', HELL, 'hell'],
	['dunkel', DUNKEL, 'dunkel'],
	['hell gewählt', HELL_GEWAEHLT, 'hell'],
	['dunkel gewählt', DUNKEL_GEWAEHLT, 'dunkel']
];

describe('Schalter Hell/Dunkel (Etappe 3b): gewählte Themen haben dieselben Werte', () => {
	it('„Dunkel“ gewählt setzt genau die Token des dunklen Systems', () => {
		expect(DUNKEL_GEWAEHLT_BLOCK).toEqual(DUNKEL_NUR_BLOCK);
		expect(Object.keys(DUNKEL_NUR_BLOCK).length).toBeGreaterThan(15);
	});

	it('„Hell“ gewählt setzt keine eigenen Token (nur die Werte aus :root)', () => {
		expect(HELL_BLOCK).toEqual({});
		expect(HELL_GEWAEHLT).toEqual(HELL);
	});

	it('die Blöcke heißen so, wie thema.ts sie setzt, und die Bedienelemente folgen', () => {
		expect(CSS).toMatch(
			/:root\[data-theme='hell'\],\s*\[data-thema-vorschau='hell'\]\s*\{\s*color-scheme: light;/
		);
		expect(CSS).toMatch(
			/:root\[data-theme='dunkel'\],\s*\[data-thema-vorschau='dunkel'\]\s*\{\s*color-scheme: dark;/
		);
	});
});

describe('Kontrast der Token', () => {
	it('liest app.css: Hex, var() und color-mix', () => {
		expect(farbe('var(--card)', HELL)).toEqual([1, 1, 1, 1]);
		expect(farbe('color-mix(in srgb, #000000 50%, transparent)', HELL)).toEqual([0, 0, 0, 0.5]);
		expect(kontrast({ '--a': '#000000', '--b': '#ffffff' }, '--a', '--b', '--b')).toBeCloseTo(21);
		expect(DUNKEL['--card']).not.toBe(HELL['--card']);
	});

	it.each(THEMEN)('%s: alle Paare erreichen ihren Mindestwert', (thema, t) => {
		const zeilen = PAARE.map(([vorne, flaeche, mindest, wofuer]) => {
			const k = kontrast(t, vorne, flaeche);
			return { paar: `${vorne} auf ${flaeche}`, kontrast: Number(k.toFixed(2)), mindest, wofuer };
		});
		if (process.env.KONTRAST_ZEIGEN) {
			console.log(thema);
			console.table(zeilen);
		}
		expect(zeilen.filter((z) => z.kontrast < z.mindest)).toEqual([]);
	});

	it('gewählte Themen: dieselben Verhältnisse wie das System-Thema', () => {
		const messen = (t: Token): number[] => PAARE.map(([v, f]) => kontrast(t, v, f));
		expect(messen(HELL_GEWAEHLT)).toEqual(messen(HELL));
		expect(messen(DUNKEL_GEWAEHLT)).toEqual(messen(DUNKEL));
	});
});

/**
 * Reihen und „wartet“ (Auflage aus 3a-2, Plan 3a+): Die Werte stehen hier
 * auf zwei Stellen fest, dazu der verlangte Mindestwert (Text 4,5:1, Grafik
 * 3:1), in beiden Themen auf --bg und --card. Fünf Token hat Architektur in
 * 3a+ geändert (plan-3a.toml [plan_3a_plus.kontrast_token]): --warn und
 * --warn-text hell, --reihe-1, --reihe-3, --reihe-7. Ändert sich ein Token,
 * schlägt das Festhalten an; dann neu messen und vorher entscheiden lassen.
 * Prüfbericht (von Architektur freigegeben): --warn hell #c46b06,
 * darum --warn-text auf --warn-soft 5,03 statt 5,07 und --warn auf --bg 3,56
 * statt 3,13.
 *
 * Die Reihen (--reihe-1 … --reihe-10) sind in beiden Themen gleich; sie
 * tragen Balken und Konfetti, also Grafik ab 3:1, auf --bg und auf --card.
 */
const OFFEN: [string, string, number, string, { hell: number; dunkel: number }][] = [
	[
		'--warn-text',
		'--warn-soft',
		4.5,
		'Text „wartet“ auf eigener Fläche',
		{ hell: 5.03, dunkel: 8.14 }
	],
	['--warn', '--bg', 3, 'Punkt/Balken „wartet“ auf der Seite', { hell: 3.56, dunkel: 11.02 }],
	...(
		[
			[4.96, 5.36, 3.43, 3.1],
			[4.08, 4.41, 4.17, 3.77],
			[3.1, 3.35, 5.48, 4.96],
			[4.08, 4.42, 4.17, 3.77],
			[4.7, 5.08, 3.62, 3.27],
			[3.15, 3.41, 5.4, 4.88],
			[3.14, 3.4, 5.42, 4.9],
			[3.77, 4.07, 4.52, 4.09],
			[4.8, 5.19, 3.55, 3.21],
			[4.52, 4.89, 3.77, 3.41]
		] as const
	).flatMap(([hellBg, hellCard, dunkelBg, dunkelCard], i): (typeof OFFEN)[number][] => [
		[
			`--reihe-${String(i + 1)}`,
			'--bg',
			3,
			'Reihe (Balken, Konfetti) auf der Seite',
			{ hell: hellBg, dunkel: dunkelBg }
		],
		[
			`--reihe-${String(i + 1)}`,
			'--card',
			3,
			'Reihe (Balken, Konfetti) auf Karten',
			{ hell: hellCard, dunkel: dunkelCard }
		]
	])
];

describe('Kontrast: Reihen und wartet (Token 3a+, plan-3a.toml [plan_3a_plus.kontrast_token])', () => {
	it.each(THEMEN)('%s: Messwerte von heute festgehalten', (_thema, t, grund) => {
		const ist = OFFEN.map(([vorne, flaeche]) => Number(kontrast(t, vorne, flaeche).toFixed(2)));
		const soll = OFFEN.map(([, , , , w]) => (grund === 'hell' ? w.hell : w.dunkel));
		expect(ist).toEqual(soll);
	});

	for (const [thema, t] of THEMEN) {
		for (const [vorne, flaeche, mindest, wofuer] of OFFEN) {
			const k = kontrast(t, vorne, flaeche);
			const name = `${thema}: ${vorne} auf ${flaeche} ab ${String(mindest)}:1, heute ${k.toFixed(2)}:1 (${wofuer})`;
			it(name, () => {
				expect(k).toBeGreaterThanOrEqual(mindest);
			});
		}
	}
});

/**
 * Bedeutungen in Gebrauch: Die Paare oben nennen die
 * Token; hier stehen die Paare, die die Oberfläche wirklich malt. Die Klassen
 * kommen aus bedeutung.ts (klasse()) und ihre Werte aus app.css (.b-…), die
 * Bausteine unten (Marke, StandHinweis, Ampel mit StatusDot, Widget Stand,
 * Hinweis, ProjektSpalten, Seitenleiste) malen damit:
 *
 * - Ton (--ton) als Punkt, Lampe, Balken, Ring oder Rand: ab 3:1 auf Karte,
 *   Seite, der eigenen Fläche (Punkt in Marke und StandHinweis), der Spalte
 *   in ProjektSpalten und dem gewählten Eintrag der Seitenleiste (Bau-Marke).
 * - Text auf der Fläche (--text, --muted auf --ton-soft): ab 4,5:1.
 * - Textfarbe der Bedeutung (--ok-text …, falls es sie gibt): ab 4,5:1 auf
 *   Karte, Seite und eigener Fläche.
 *
 * Töne tragen nie Text (eigener Test unten); wer das will, nimmt --…-text.
 */
const SPALTE = 'color-mix(in srgb, var(--ton-soft) 60%, var(--bg))';

/** [vorne, Fläche, Mindestwert, wo] je Bedeutung; --ton und --ton-soft aus .b-… */
const TON_PAARE: [string, string, number, string][] = [
	['--ton', '--card', 3, 'Punkt, Lampe, Balken, Ring, Widget-Rand auf Karten (StatusDot, Ampel)'],
	['--ton', '--bg', 3, 'Rand und Marke auf der Seite (ProjektSpalten, Seitenleiste)'],
	['--ton', '--ton-soft', 3, 'Punkt in Marke und StandHinweis auf eigener Fläche'],
	['--ton', SPALTE, 3, 'Rand oben an der Spalte (ProjektSpalten)'],
	['--ton', '--accent-soft', 3, 'Bau-Marke auf gewähltem Eintrag (Seitenleiste)'],
	['--text', '--ton-soft', 4.5, 'Wort in Marke, StandHinweis alt, Zahlen im Widget Stand, Hinweis'],
	['--muted', '--ton-soft', 4.5, 'Nebentext auf eigener Fläche (BackupCard, Hinweis)'],
	['--text', SPALTE, 4.5, 'Text in der Spalte (ProjektSpalten)'],
	['--muted', SPALTE, 4.5, 'Anzahl in der Spalte (ProjektSpalten)']
];

/** Einzelne Stellen mit eigener Mischung: [vorne, Fläche, Mindestwert, wo]. */
const STELLEN: [string, string, number, string][] = [
	[
		'--text',
		'color-mix(in srgb, var(--warn) 30%, transparent)',
		4.5,
		'Frist „achtung“ (EintragZeile, AufgabenKasten)'
	],
	[
		'--text',
		'color-mix(in srgb, var(--warn) 10%, transparent)',
		4.5,
		'Zeile „wartet“ (AufgabenKasten, SystemApps)'
	],
	[
		'--text',
		'color-mix(in srgb, var(--ok) 12%, transparent)',
		4.5,
		'Antwort angekommen (AntwortStatus)'
	],
	[
		'--bad-text',
		'color-mix(in srgb, var(--bad) 6%, transparent)',
		4.5,
		'Fehler im Vorgang (VorgangZeile)'
	],
	[
		'color-mix(in srgb, var(--bad-text) 80%, transparent)',
		'--card',
		4.5,
		'fehlende Kopie, Deckkraft 0,8 (BackupCard)'
	],
	['--muted', '--card', 4.5, '„Wie vorgeschlagen“, auch gesperrt (EintragZeile, ohne Deckkraft)'],
	['--on-warn', '--warn', 4.5, 'Zähler auf Gelb (Glocke, Menü, Seitenleiste)'],
	['--accent-text', '--accent-soft', 4.5, 'Zähler „wartet“ in der Seitenleiste']
];

/** Werte von --ton und --ton-soft einer Klasse .b-… aus app.css. */
function bedeutungsToken(b: Bedeutung): Token {
	const t = token(block(CSS, new RegExp(`^\\.${klasse(b)}\\s*\\{`, 'm')));
	if (t['--ton'] === undefined || t['--ton-soft'] === undefined) {
		throw new Error(`.${klasse(b)} setzt --ton und --ton-soft nicht`);
	}
	return t;
}

/** Textfarbe einer Bedeutung: zu --ton = var(--ok) gehört --ok-text, falls es sie gibt. */
function textToken(b: Bedeutung, t: Token): string | null {
	const ton = /^var\((--[a-z]+)\)$/.exec(bedeutungsToken(b)['--ton'] ?? '')?.[1];
	const name = `${ton ?? ''}-text`;
	return t[name] === undefined ? null : name;
}

interface Zeile {
	paar: string;
	kontrast: number;
	mindest: number;
	wo: string;
}

function inGebrauch(t: Token): Zeile[] {
	const zeile = (
		tt: Token,
		vorne: string,
		flaeche: string,
		mindest: number,
		wo: string,
		vor = ''
	) => ({
		paar: `${vor}${vorne} auf ${flaeche}`,
		kontrast: Number(kontrast(tt, vorne, flaeche).toFixed(2)),
		mindest,
		wo
	});
	const jeBedeutung = BEDEUTUNGEN.flatMap((b) => {
		const tb: Token = { ...t, ...bedeutungsToken(b) };
		const text = textToken(b, t);
		const textPaare: [string, string, number, string][] =
			text === null
				? []
				: [
						[text, '--card', 4.5, 'Text der Bedeutung auf Karten'],
						[text, '--bg', 4.5, 'Text der Bedeutung auf der Seite'],
						[text, '--ton-soft', 4.5, 'Text der Bedeutung auf eigener Fläche']
					];
		return [...TON_PAARE, ...textPaare].map(([vorne, flaeche, mindest, wo]) =>
			zeile(tb, vorne, flaeche, mindest, wo, `${klasse(b)}: `)
		);
	});
	const stellen = STELLEN.map(([vorne, flaeche, mindest, wo]) =>
		zeile(t, vorne, flaeche, mindest, wo)
	);
	return [...jeBedeutung, ...stellen];
}

/** Quelltext unter src, Leerraum zu einem Leerzeichen. */
const quelle = (...teile: string[]): string =>
	readFileSync(join(import.meta.dirname, ...teile), 'utf8').replace(/\s+/g, ' ');

/** Was die Bausteine malen; ändert sich eine Zeile, gehört das neue Paar oben dazu. */
const BAUSTEINE: [string, string[]][] = [
	[
		'lib/katalog/Marke.svelte',
		[
			'background: var(--ton-soft, var(--plan-soft));',
			'color: var(--text);',
			'background: var(--ton, var(--plan));'
		]
	],
	[
		'lib/katalog/StandHinweis.svelte',
		[
			'color: var(--muted);',
			'color: var(--text); background: var(--ton-soft);',
			'background: var(--ton);'
		]
	],
	['lib/katalog/widgets/Stand.svelte', ['background: var(--ton-soft, var(--plan-soft));']],
	[
		'lib/katalog/Hinweis.svelte',
		[
			'border-left: 4px solid var(--ton, var(--done));',
			'background: var(--ton-soft, var(--done-soft));'
		]
	],
	[
		'lib/components/blocks/Ampel.svelte',
		['<StatusDot bedeutung={r.bedeutung} />', '<StandHinweis']
	],
	[
		'lib/components/ProjektSpalten.svelte',
		[
			'border-top: 0.25rem solid var(--ton, var(--plan));',
			'background: color-mix(in srgb, var(--ton-soft, var(--card)) 60%, var(--bg));'
		]
	],
	['app.css', ['.dot {', 'background: var(--ton, var(--plan));']],
	[
		'lib/components/anstehend/EintragZeile.svelte',
		['.vorschlag { color: var(--muted); }', '.vorschlag:disabled { opacity: 1;']
	]
];

/** Alle Stile der Oberfläche (*.svelte und *.css unter src). */
function stilDateien(ordner: string): string[] {
	return readdirSync(ordner).flatMap((name) => {
		const pfad = join(ordner, name);
		if (statSync(pfad).isDirectory()) return stilDateien(pfad);
		return /\.(svelte|css)$/.test(name) ? [pfad] : [];
	});
}

describe('Kontrast: Bedeutungen in Gebrauch', () => {
	it('jede Bedeutung aus bedeutung.ts hat ihre Klasse mit Ton und Fläche', () => {
		expect(BEDEUTUNGEN.map((b) => Object.keys(bedeutungsToken(b)).sort())).toEqual(
			BEDEUTUNGEN.map(() => ['--ton', '--ton-soft'])
		);
		// Textfarben gibt es für ok, läuft, wartet und Problem.
		expect(BEDEUTUNGEN.filter((b) => textToken(b, HELL) !== null)).toEqual([
			'ok',
			'laeuft',
			'wartet',
			'problem'
		]);
	});

	it.each(THEMEN)('%s: alle benutzten Paare erreichen ihren Mindestwert', (thema, t) => {
		const zeilen = inGebrauch(t);
		if (process.env.KONTRAST_ZEIGEN) {
			console.log(`${thema} (in Gebrauch)`);
			console.table(zeilen);
		}
		expect(zeilen.length).toBeGreaterThan(60);
		expect(zeilen.filter((z) => z.kontrast < z.mindest)).toEqual([]);
	});

	it.each(BAUSTEINE)('%s malt mit den geprüften Paaren', (pfad, stellen) => {
		const text = quelle(...pfad.split('/'));
		for (const s of stellen) expect(text).toContain(s);
	});

	it('Töne tragen keinen Text: nie color: var(--ok) und Co., dafür gibt es --…-text', () => {
		const ton =
			/(?<![\w-])color\s*[:=]\s*["'{]*\s*var\(--(?:ton|ok|warn|bad|info|done|plan)\s*[,)]/;
		const funde = stilDateien(import.meta.dirname)
			.filter((d) => ton.test(readFileSync(d, 'utf8')))
			.map((d) => relative(import.meta.dirname, d));
		expect(funde).toEqual([]);
	});
});
