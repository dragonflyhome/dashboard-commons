/**
 * Etappe 3b, Teil C (Entscheidung plan-3b, Ergänzung C): Jede
 * Abschaltung einer Prüfung in web/src steht in der Liste der Anleitung
 * (docs/anleitung/contributing-05-5-committen.toml, Teil „erlaubte
 * Abschaltungen“, die Tabelle mit den Spalten Datei, Regel, Anzahl, Grund)
 * und hat ihren Grund in derselben Zeile. Gezählt wird je Datei und Regel;
 * eine Stelle ohne Eintrag, eine zusätzliche Stelle und ein Eintrag ohne
 * Stelle machen den Test rot.
 *
 * Gesucht wird nur in Kommentaren: nach // , /* oder <!-- folgt das
 * Stichwort. Steht davor ein Anführungszeichen, ist es ein Beispiel in einer
 * Zeichenkette (etwa in stil.spec.ts) und zählt nicht.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC = import.meta.dirname;
const LISTE = join(SRC, '..', '..', 'docs', 'anleitung', 'contributing-05-5-committen.toml');

/** Ein Fund: Datei (relativ zu web/src), Zeile, Regel und ob der Grund fehlt. */
interface Stelle {
	datei: string;
	zeile: number;
	regel: string;
	mangel?: string;
}

/** Beginn eines Kommentars, nicht direkt hinter einem Anführungszeichen. */
const KOMMENTAR = /(^|[^'"`\\])(\/\/|\/\*|<!--)\s*/g;

/** Die Abschaltungen in einer Zeile (leer, wenn keine). */
export function stellenInZeile(datei: string, zeile: number, text: string): Stelle[] {
	for (const m of text.matchAll(KOMMENTAR)) {
		const funde = stellenAb(datei, zeile, text.slice(m.index + m[0].length));
		if (funde.length > 0) return funde;
	}
	return [];
}

/** Die Abschaltung am Anfang von `kommentar` (Text hinter dem Kommentarzeichen). */
function stellenAb(datei: string, zeile: number, kommentar: string): Stelle[] {
	// Ende des Kommentars abschneiden.
	const rest = kommentar.replace(/\s*(\*\/|-->)\s*$/, '');
	const mit = (regeln: string[], mangel?: string): Stelle[] =>
		regeln.map((regel) => ({ datei, zeile, regel, ...(mangel ? { mangel } : {}) }));

	// ESLint und Stylelint: Regeln, dann „-- Grund“.
	const lint = /^(eslint|stylelint)-disable(-next-line|-line)?\b(.*)$/.exec(rest);
	if (lint) {
		const [vorne = '', ...grund] = (lint[3] ?? '').split(/\s--\s|\s--$/);
		const regeln = vorne
			.split(',')
			.map((r) => r.trim())
			.filter((r) => r !== '');
		if (regeln.length === 0) return mit([`${lint[1]}-disable`], 'keine Regel genannt');
		return mit(regeln, grund.join(' -- ').trim() === '' ? 'kein Grund nach „--“' : undefined);
	}
	// ESLint-Einstellung im Kommentar (eslint regel: off) schaltet auch ab.
	const einstellung = /^eslint\s+([\w@/-]+)\s*:/.exec(rest);
	if (einstellung) return mit([einstellung[1] ?? ''], 'Regel-Einstellung im Kommentar');
	// Svelte: Codes, dann der Grund in Klammern.
	const svelte = /^svelte-ignore\s+(.*)$/.exec(rest);
	if (svelte) {
		const codes = svelte[1] ?? '';
		const klammer = codes.indexOf('(');
		const vorne = klammer === -1 ? codes : codes.slice(0, klammer);
		const grund = klammer === -1 ? '' : codes.slice(klammer + 1).replace(/\)\s*$/, '');
		const regeln = vorne
			.split(/[\s,]+/)
			.map((r) => r.trim())
			.filter((r) => r !== '');
		if (regeln.length === 0) return mit(['svelte-ignore'], 'kein Code genannt');
		return mit(regeln, grund.trim() === '' ? 'kein Grund in Klammern' : undefined);
	}
	// TypeScript.
	const ts = /^@ts-(expect-error|ignore|nocheck)\b(.*)$/.exec(rest);
	if (ts) {
		const regel = `@ts-${ts[1]}`;
		if (ts[1] !== 'expect-error') return mit([regel], `${regel} ist nicht erlaubt`);
		const grund = (ts[2] ?? '').replace(/^\s*(--|:)?\s*/, '');
		return mit([regel], grund === '' ? 'kein Grund nach „--“' : undefined);
	}
	return [];
}

function dateien(pfad: string): string[] {
	if (statSync(pfad).isFile()) return [pfad];
	return readdirSync(pfad).flatMap((n) => dateien(join(pfad, n)));
}

function alleStellen(): Stelle[] {
	return (
		dateien(SRC)
			.filter((p) => /\.(svelte|ts|js|css|html)$/.test(p))
			// Diese Datei selbst enthält die Formen nur als Beispiele.
			.filter((p) => !p.endsWith('abschaltungen.spec.ts'))
			.flatMap((p) => {
				const datei = relative(SRC, p).split('\\').join('/');
				return readFileSync(p, 'utf8')
					.split(/\r?\n/)
					.flatMap((text, i) => stellenInZeile(datei, i + 1, text));
			})
	);
}

/** Die Liste aus der Anleitung: Schlüssel „Datei Regel“, Wert die Anzahl. */
function liste(): Map<string, number> {
	const text = readFileSync(LISTE, 'utf8');
	const anfang = text.indexOf("spalten = ['Datei', 'Regel', 'Anzahl', 'Grund']");
	expect(anfang, 'Tabelle der Abschaltungen in docs/anleitung').toBeGreaterThan(-1);
	// Die Tabelle endet mit der ersten Zeile, die nur „]“ ist.
	const ende = text.slice(anfang).search(/\r?\n\]/);
	expect(ende).toBeGreaterThan(-1);
	const eintraege = new Map<string, number>();
	for (const zeile of text.slice(anfang, anfang + ende).split(/\r?\n/)) {
		// Nur die Zeilen der Tabelle; jede muss sich lesen lassen.
		if (!zeile.startsWith('  [')) continue;
		const m = /^\s*\['([^']+)',\s*'([^']+)',\s*'(\d+)',\s*'([^']*)'\],?\s*$/.exec(zeile);
		expect(m, `Zeile der Liste nicht lesbar: ${zeile}`).not.toBeNull();
		if (!m) continue;
		expect(m[4], `Grund in der Liste für ${m[1]} ${m[2]}`).not.toBe('');
		const schluessel = `${m[1]} ${m[2]}`;
		expect(eintraege.has(schluessel), `doppelt in der Liste: ${schluessel}`).toBe(false);
		eintraege.set(schluessel, Number(m[3]));
	}
	return eintraege;
}

describe('Abschaltungen in web/src (Etappe 3b)', () => {
	it('jede Abschaltung hat ihren Grund in derselben Zeile', () => {
		const ohne = alleStellen()
			.filter((s) => s.mangel)
			.map((s) => `${s.datei}:${s.zeile} ${s.regel}: ${s.mangel}`);
		expect(ohne).toEqual([]);
	});

	it('jede Abschaltung steht in der Liste der Anleitung, genau so oft', () => {
		const gezaehlt = new Map<string, number>();
		for (const s of alleStellen()) {
			const k = `${s.datei} ${s.regel}`;
			gezaehlt.set(k, (gezaehlt.get(k) ?? 0) + 1);
		}
		const erlaubt = liste();
		const abweichend = [...new Set([...gezaehlt.keys(), ...erlaubt.keys()])]
			.filter((k) => gezaehlt.get(k) !== erlaubt.get(k))
			.sort()
			.map((k) => `${k}: im Code ${gezaehlt.get(k) ?? 0}, in der Liste ${erlaubt.get(k) ?? 0}`);
		expect(abweichend).toEqual([]);
	});

	it('der Wächter erkennt die Formen und übersieht Beispiele in Zeichenketten', () => {
		const f = (t: string) => stellenInZeile('x.svelte', 1, t);
		expect(f('\t// eslint-disable-next-line a/b, c -- weil')).toEqual([
			{ datei: 'x.svelte', zeile: 1, regel: 'a/b' },
			{ datei: 'x.svelte', zeile: 1, regel: 'c' }
		]);
		expect(f('<!-- eslint-disable-next-line a -->')[0]?.mangel).toBe('kein Grund nach „--“');
		expect(f('/* eslint-disable */')[0]?.mangel).toBe('keine Regel genannt');
		expect(f('/* stylelint-disable-next-line color-no-hex -- Grund */')).toEqual([
			{ datei: 'x.svelte', zeile: 1, regel: 'color-no-hex' }
		]);
		expect(f('/* stylelint-disable color-no-hex */')[0]?.mangel).toBeDefined();
		expect(f('<!-- svelte-ignore a11y_x, a11y_y (weil es so ist) -->')).toEqual([
			{ datei: 'x.svelte', zeile: 1, regel: 'a11y_x' },
			{ datei: 'x.svelte', zeile: 1, regel: 'a11y_y' }
		]);
		expect(f('<!-- svelte-ignore a11y_x -->')[0]?.mangel).toBe('kein Grund in Klammern');
		expect(f('// @ts-expect-error -- Typ der Bibliothek fehlt')[0]?.mangel).toBeUndefined();
		expect(f('// @ts-expect-error')[0]?.mangel).toBeDefined();
		expect(f('// @ts-ignore weil')[0]?.mangel).toBe('@ts-ignore ist nicht erlaubt');
		expect(f('/* eslint no-console: off */')[0]?.regel).toBe('no-console');
		expect(f("\t\t'/* stylelint-disable-next-line color-no-hex */\\na { }',")).toEqual([]);
		expect(f('/* stylelint-enable color-no-hex */')).toEqual([]);
		expect(f('// ein normaler Kommentar')).toEqual([]);
		expect(f("const a = 'http://x'; // eslint-disable-line no-x -- weil")[0]?.regel).toBe('no-x');
	});

	it('der Wächter sieht genug: die bekannten Stellen sind dabei', () => {
		const regeln = new Set(alleStellen().map((s) => `${s.datei} ${s.regel}`));
		expect(regeln).toContain('app.css color-no-hex');
		expect(regeln).toContain(
			'lib/components/kopf/Brotkrumen.svelte svelte/no-navigation-without-resolve'
		);
		expect(regeln).toContain('lib/katalog/zeit/Gantt.svelte svelte/prefer-svelte-reactivity');
		expect(regeln).toContain('lib/thema.svelte.spec.ts @typescript-eslint/no-implied-eval');
		expect(regeln).toContain('routes/katalog/Beispiel.svelte a11y_no_noninteractive_tabindex');
	});
});
