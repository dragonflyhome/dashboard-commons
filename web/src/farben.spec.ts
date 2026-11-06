/**
 * Regel „keine freien Farben“ für ganz web/src (Plan 3a, ab 3a-2).
 *
 * Arbeitsteilung, damit nichts doppelt gemeldet wird:
 * - Stylelint (stylelint.config.js) prüft alle Stile: *.css, die
 *   <style>-Blöcke in *.svelte und über postcss-html auch die Attribute
 *   style="…" darin.
 * - Dieser Test prüft, was Stylelint nicht sieht: Markup und Skript in
 *   *.svelte (style:…, fill=, stroke=, Zeichenketten) und *.ts; style="…"
 *   prüft er zur Sicherheit auch.
 *
 * Ausgenommen sind die Token-Definitionen (app.css, Stylelint mit
 * Begründung), erzeugte Dateien (ts-rs in lib/vertrag, *.generated.ts) und
 * Tests (*.spec.ts, sie enthalten absichtlich Beispiele).
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC = import.meta.dirname;

/**
 * Hex-Farben (#abc, #aabbcc, #aabbccdd) und Farbfunktionen, auch color()
 * und device-cmyk() wie in Stylelint. Ein Anker ist keine Farbe (Auflage aus
 * 3a-2): nicht nach href=, nicht hinter einem Pfad (/pc#bad) oder Wort,
 * nicht vor einem Bindestrich oder weiteren Zeichen (#bad-stelle).
 */
const FREIE_FARBE =
	/(?<![\w/.-]|\bhref\s*=\s*["'{`]*)#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})(?![\w-])|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|device-cmyk)\s*\(/gi;

/** Farbnamen, die in Stilen vorkommen könnten (ohne transparent, currentColor, inherit). */
const FARBNAMEN =
	'white|black|red|green|blue|yellow|orange|gray|grey|silver|gold|purple|pink|navy|teal|lime|maroon|olive|aqua|fuchsia|cyan|magenta|brown|beige|ivory|tomato|crimson|coral|salmon|violet|indigo|turquoise';

/**
 * Farbnamen nur dort, wo sie Farbe sind: in style="…", style:x="…",
 * style:x={'…'} oder style:x={"…"}, fill="…", fill={"…"} und stroke=… ebenso.
 * Im Fließtext wäre „red“ kein Fehler.
 */
const FARBNAME_IM_MARKUP = new RegExp(
	`(?:\\bstyle(?::[a-z-]+)?|\\bfill|\\bstroke)\\s*=\\s*(?:\\{?"[^"]*?|\\{?'[^']*?)\\b(${FARBNAMEN})\\b`,
	'gi'
);

function dateien(ordner: string): string[] {
	return readdirSync(ordner).flatMap((name) => {
		const pfad = join(ordner, name);
		if (statSync(pfad).isDirectory()) {
			return pfad.endsWith(`${sep}lib${sep}vertrag`) ? [] : dateien(pfad);
		}
		if (/\.(spec|test)\.ts$/.test(name) || name.endsWith('.generated.ts')) return [];
		return /\.(svelte|ts)$/.test(name) ? [pfad] : [];
	});
}

/** Stile prüft Stylelint; übrig bleiben Markup und Skript (Zeilen bleiben gleich). */
function ohneStilbloecke(text: string): string {
	return text.replace(/<style[^>]*>[\s\S]*?<\/style>/g, (block) => block.replace(/[^\n]/g, ' '));
}

/** Funde mit Zeile: „3: #fff“. */
function freieFarben(text: string): string[] {
	const zeile = (i: number): number => text.slice(0, i).split('\n').length;
	return [
		...[...text.matchAll(FREIE_FARBE)].map((m) => `${String(zeile(m.index))}: ${m[0]}`),
		...[...text.matchAll(FARBNAME_IM_MARKUP)].map(
			(m) => `${String(zeile(m.index))}: ${String(m[1])}`
		)
	];
}

describe('keine freien Farben', () => {
	it('erkennt Hex, Farbfunktionen und Farbnamen in Stil-Attributen', () => {
		expect(
			freieFarben(
				'<a style="color: #fff"></a><b style:background="rgb(1 2 3)"></b>\n<path fill="red" stroke="hsla(0, 0%, 0%, 1)" /><i style:color={\'white\'}></i>'
			).map((f) => f.slice(f.indexOf(' ') + 1))
		).toEqual(['#fff', 'rgb(', 'hsla(', 'red', 'white']);
	});

	it('erkennt color(), device-cmyk() und Farbnamen in fill={"…"} (Auflage aus 3a-2)', () => {
		expect(
			freieFarben(
				'<b style:color="color(srgb 1 0 0)"></b><i style="color: device-cmyk(0 0 0 1)"></i>\n<path fill={"red"} stroke={"#abc"} /><u style:background={"navy"}></u>'
			).map((f) => f.slice(f.indexOf(' ') + 1))
		).toEqual(['color(', 'device-cmyk(', '#abc', 'red', 'navy']);
	});

	it('Anker wie #bad sind keine Farbe, auch nicht mit Pfad oder in href={…}', () => {
		expect(
			freieFarben(
				"<a href=\"#bad\">x</a><a href='#add'>y</a><a href={'#fade'}>z</a><a href={`#cafe`}>w</a>\nconst ziel = `/pc#bad`; const z2 = resolve('/') + '/anstehend#dead-beef';"
			)
		).toEqual([]);
		// Dieselben Buchstaben als Farbe bleiben ein Fund.
		expect(freieFarben('<i style="color: #bad"></i>')).toEqual(['1: #bad']);
	});

	it('lässt Token, Anker, Importe und Wörter im Text in Ruhe', () => {
		expect(
			freieFarben(
				'<a style:color="var(--accent)" href=\'#ziel\'>red</a><path fill="currentColor" stroke="none" />\nimport x from \'#lib/zeit\''
			)
		).toEqual([]);
	});

	it('überlässt <style> der Stil-Prüfung (Stylelint)', () => {
		const svelte = '<p>x</p>\n<style>\n\tp { color: #fff; }\n</style>\n<i style="color: #000"></i>';
		expect(freieFarben(ohneStilbloecke(svelte))).toEqual(['5: #000']);
	});

	it('ganz web/src nutzt nur Token', () => {
		const funde = dateien(SRC).flatMap((pfad) => {
			const text = readFileSync(pfad, 'utf8');
			return freieFarben(pfad.endsWith('.svelte') ? ohneStilbloecke(text) : text).map(
				(f) => `${relative(SRC, pfad)}:${f}`
			);
		});
		expect(funde).toEqual([]);
	});
});
