/**
 * Schritt E2b: die Dauerprobe auf der Platte (scripts/oeffentlich-probe.mjs).
 * Ein erfundener Bau mit erfundener Bauliste: Slugs und Seitentexte in einer
 * geschützten Datei sind in Ordnung, in einer öffentlichen ein Fund; frei ist
 * nur, was auch in den Quellen der Hülle steht (bei jedem Lauf abgeleitet,
 * nirgends gespeichert). Blind (Stufe 2, auch im Bau ein Fehler) ist sie, wenn
 * sie die Texte nicht in den geschützten Dateien findet, gar keinen prüfbaren
 * Text hat, eine Seitendatei fehlt oder ein Eintrag unlesbar ist, und je Seite
 * eines Slugs, der ein Wort der Hülle ist. Ein Fund benennt die Liste um.
 */
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type * as Fs from 'node:fs';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	baumTexte,
	baulisteLesen,
	eben,
	fenster,
	hauptprogramm,
	probe,
	satzfolgen
} from '../scripts/oeffentlich-probe.mjs';

// Ein Test sperrt Umbenennen und Löschen der Liste (wie eine Datei, die ein anderes Programm offen hält).
const sperre = vi.hoisted(() => ({ an: false }));
vi.mock('node:fs', async (echt) => {
	const m = await echt<typeof Fs>();
	const gesperrt =
		<A extends unknown[], R>(f: (...a: A) => R) =>
		(...a: A): R => {
			if (sperre.an && String(a[0]).endsWith('oeffentlich.json')) throw new Error('gesperrt');
			return f(...a);
		};
	return { ...m, renameSync: gesperrt(m.renameSync), rmSync: gesperrt(m.rmSync) };
});

let wurzeln: string[] = [];
afterEach(() => {
	for (const w of wurzeln) rmSync(w, { recursive: true, force: true });
	wurzeln = [];
});

const SATZ = 'Ein erfundener Satz der Projektseite';
/** Die Quellen der Hülle im Test: Sie nennen den Weg /pc, nicht den Satz. */
const HUELLE = "resolve('/pc')";
/** Die Bauliste ohne Einträge, wie der Erzeuger sie schreibt (scanner::bauliste_text). */
const LEERE_BAULISTE = [
	'// Erzeugt von dashboard pruefe --bauliste. Nicht von Hand ändern.',
	'',
	"type Laden = () => Promise<{ default: import('svelte').Component }>;",
	'',
	'export const projektSeiten: Record<string, Laden> = {};',
	'',
	'export const projektUnterseiten: Record<string, Laden> = {};',
	''
].join('\n');

/**
 * Ein Ordner wie `web/`: build/, src/lib/projekte.generated.ts und eine Datei der Hülle.
 * `pcSeite`: eigener Inhalt der Seite von „pc“ (sonst dieselbe Datei wie zqxprojekt).
 */
function aufbau(
	oeffentlich: string,
	geschuetzt: string,
	pcSeite?: string
): { bau: string; bauliste: string; web: string } {
	const wurzel = mkdtempSync(join(tmpdir(), 'oeffentlich-probe-'));
	wurzeln.push(wurzel);
	const bau = join(wurzel, 'build');
	mkdirSync(join(bau, '_app', 'immutable', 'chunks'), { recursive: true });
	const seite = join(wurzel, 'zqxprojekt', 'Seite.svelte');
	mkdirSync(join(wurzel, 'zqxprojekt'));
	writeFileSync(
		seite,
		`<script>\n\tconst x = 1;\n</script>\n\n<h2>Kurz</h2>\n<p>${SATZ} mit {x} Rest</p>\n<style>\n\tp { color: red; }\n</style>\n`
	);
	let pc = seite;
	if (pcSeite !== undefined) {
		pc = join(wurzel, 'pc', 'Seite.svelte');
		mkdirSync(join(wurzel, 'pc'));
		writeFileSync(pc, pcSeite);
	}
	mkdirSync(join(wurzel, 'src', 'lib'), { recursive: true });
	const bauliste = join(wurzel, 'src', 'lib', 'projekte.generated.ts');
	writeFileSync(
		bauliste,
		[
			'// Erzeugt von dashboard pruefe --bauliste. Nicht von Hand ändern.',
			'',
			"type Laden = () => Promise<{ default: import('svelte').Component }>;",
			'',
			'export const projektSeiten: Record<string, Laden> = {',
			`\tzqxprojekt: () => import('${seite.replaceAll('\\', '/')}'),`,
			`\tpc: () => import('${pc.replaceAll('\\', '/')}')`,
			'};',
			'',
			'export const projektUnterseiten: Record<string, Laden> = {};',
			''
		].join('\n')
	);
	writeFileSync(join(wurzel, 'src', 'huelle.ts'), HUELLE);
	writeFileSync(join(bau, 'index.html'), '<a href="/pc">PC</a>');
	writeFileSync(join(bau, '_app', 'immutable', 'chunks', 'oeffentlich.js'), oeffentlich);
	writeFileSync(join(bau, '_app', 'immutable', 'chunks', 'geschuetzt.js'), geschuetzt);
	writeFileSync(
		join(bau, '_app', 'oeffentlich.json'),
		JSON.stringify(['_app/immutable/chunks/oeffentlich.js', '_app/oeffentlich.json'])
	);
	return { bau, bauliste, web: wurzel };
}

describe('Dauerprobe der öffentlichen Bau-Dateien', () => {
	it('liest Slugs, Texte (auch Attribute und Zeichenketten) und Fenster', () => {
		expect(
			baulisteLesen("\t'a-b': () => import('/x/S.svelte'),\n\tc: () =>\n\t\timport('/y')").slugs
		).toEqual(['a-b', 'c']);
		expect(
			satzfolgen(
				`<p title="Ein Titel im Attribut hier">${SATZ} mit {x} Rest</p><script>const s = 'eins zwei drei vier';</script><style>p { a: b c d e; }</style>`
			)
		).toEqual([`${SATZ} mit`, 'Ein Titel im Attribut hier', 'eins zwei drei vier']);
		expect(fenster('eins zwei drei')).toEqual(['eins zwei drei']);
		expect(fenster(`${SATZ} mit`)).toEqual([
			'Ein erfundener Satz der',
			'erfundener Satz der Projektseite',
			'Satz der Projektseite mit'
		]);
		// Svelte lässt Zeilenumbrüche und Tabs im Text stehen; Entitäten wie im HTML.
		expect(eben('a\n\t\tb\n\tc &amp; d&nbsp;e')).toBe('a b c & d e');
	});

	it('grün, wenn Slug und Text nur in geschützten Dateien stehen; ein Wort der Hülle ist frei', () => {
		const { bau, bauliste } = aufbau(
			'export const a = "/pc";',
			`export const s = "zqxprojekt Ein erfundener Satz der\n\t\tProjektseite mit";`
		);
		const e = probe({ bau, bauliste, huelle: HUELLE });
		expect(e.stufe).toBe(0);
		expect(e.frei).toBe(1);
		expect([e.texte, e.gesehen]).toEqual([1, 1]);
		// Ohne die Hülle ist der Weg /pc ein Slug in öffentlichen Dateien: rot.
		const ohne = probe({ bau, bauliste, huelle: '' });
		expect(ohne.stufe).toBe(1);
		expect(ohne.funde.map((f) => f.datei)).toEqual([
			'_app/immutable/chunks/oeffentlich.js',
			'index.html'
		]);
	});

	it('rot bei einem Slug oder einem Textfenster in einer öffentlichen Datei, nur Zahlen', () => {
		const geschuetzt = `export const s = "${SATZ} mit";`;
		const { bau, bauliste } = aufbau(`export const s = "zqxprojekt";`, geschuetzt);
		const e = probe({ bau, bauliste, huelle: HUELLE });
		expect(e.stufe).toBe(1);
		expect(e.funde).toEqual([{ datei: '_app/immutable/chunks/oeffentlich.js', anzahl: 1 }]);
		expect(JSON.stringify(e)).not.toContain('zqxprojekt');
		expect(JSON.stringify(e)).not.toContain('erfundener');
		// Ein Fenster genügt, auch umgebrochen; steht es in der Hülle, ist es frei.
		const satz = aufbau('const t = `x erfundener Satz der\n\tProjektseite y`;', geschuetzt);
		expect(probe({ ...satz, huelle: HUELLE }).stufe).toBe(1);
		expect(probe({ ...satz, huelle: `${HUELLE} 'erfundener Satz der Projektseite'` }).stufe).toBe(
			0
		);
	});

	it('ohne oder mit kaputter Liste nicht prüfbar (2); ohne Projektseiten grün (0)', () => {
		const { bau, bauliste } = aufbau('', `"${SATZ} mit"`);
		writeFileSync(join(bau, '_app', 'oeffentlich.json'), '["_app/oeffentlich.json"');
		expect(probe({ bau, bauliste, huelle: HUELLE }).stufe).toBe(2);
		rmSync(join(bau, '_app', 'oeffentlich.json'));
		expect(probe({ bau, bauliste, huelle: HUELLE }).stufe).toBe(2);
		const leer = aufbau('', '');
		writeFileSync(leer.bauliste, LEERE_BAULISTE);
		expect(probe({ ...leer, huelle: HUELLE }).stufe).toBe(0);
	});

	it('leere Bauliste neben einem Bau mit mehr geschützten Dateien oder in fremder Form: blind (2)', () => {
		// Der Bau trägt Seiten aus einer anderen Bauliste: nicht ungezählt grün.
		const anders = aufbau(`"zqxprojekt"`, '"x"');
		writeFileSync(join(anders.bau, '_app', 'immutable', 'chunks', 'seite.js'), `"${SATZ} mit"`);
		writeFileSync(anders.bauliste, LEERE_BAULISTE);
		const e = probe({ ...anders, huelle: HUELLE });
		expect([e.stufe, e.geschuetzt]).toEqual([2, 2]);
		// Andere Form (etwa import.meta.glob): keine lesbaren Einträge, aber nicht leer.
		const glob = aufbau('', '');
		writeFileSync(
			glob.bauliste,
			"export const projektSeiten = import.meta.glob('/p/*/Seite.svelte');\nexport const projektUnterseiten = {};\n"
		);
		expect(probe({ ...glob, huelle: HUELLE }).stufe).toBe(2);
	});

	it('blind (2): Texte nicht gesehen, kein prüfbarer Text, Datei fehlt, Eintrag unlesbar', () => {
		// Der Text steht nicht einmal in der geschützten Datei: Die Probe sähe kein Leck.
		const blind = probe({ ...aufbau('', 'export {};'), huelle: HUELLE });
		expect([blind.stufe, blind.texte, blind.gesehen]).toEqual([2, 1, 0]);
		// Alle Texte stehen auch in der Hülle: nichts prüfbar.
		const ohneText = probe({
			...aufbau('', `"${SATZ} mit"`),
			huelle: `${HUELLE} '${SATZ} mit'`
		});
		expect([ohneText.stufe, ohneText.texte]).toEqual([2, 0]);
		// Eine Seitendatei der Bauliste fehlt: nicht still weglassen, auch wenn die übrige gesehen ist.
		const pcText = 'Ausführlicher Hinweis ausschließlich dieser Unterseite';
		const fehlt = aufbau('', `"${pcText}"`, `<p>${pcText}</p>\n`);
		rmSync(join(fehlt.web, 'zqxprojekt', 'Seite.svelte'));
		const e = probe({ ...fehlt, huelle: HUELLE });
		expect([e.stufe, e.fehlend, e.texte, e.gesehen]).toEqual([2, 1, 1, 1]);
		// Ein Eintrag in anderer Form als erwartet: nicht als „keine Projektseiten“ grün.
		const fremd = aufbau('', `"${SATZ} mit"`);
		writeFileSync(fremd.bauliste, 'export const projektSeiten = { "zqx": () => import("/x") };\n');
		expect(baulisteLesen(readFileSync(fremd.bauliste, 'utf8')).unlesbar).toBe(1);
		expect(probe({ ...fremd, huelle: HUELLE }).stufe).toBe(2);
	});

	it('blind je Seite eines Slugs, der ein Wort der Hülle ist, auch wenn alles andere gesehen ist', () => {
		const geschuetzt = `"${SATZ} mit"`;
		// Der Text der Seite von „pc“ steht ganz in der Hülle: den Slug zählt die Probe nicht,
		// und kein Fenster der Seite ist prüfbar. Gesamt ist alles gesehen, die Seite aber blind.
		const gemeinsam = 'Gemeinsamer Hinweistext zwischen Oberfläche und Projektseite';
		const e = probe({
			...aufbau('', geschuetzt, `<p>${gemeinsam}</p>\n`),
			huelle: `${HUELLE} '${gemeinsam}'`
		});
		expect([e.stufe, e.texte, e.gesehen, e.freieSeiten, e.blindeSeiten]).toEqual([2, 1, 1, 1, 1]);
		// Eine Seite ganz ohne eigenen Text hat nichts zu suchen: textlos, nicht blind.
		const leer = probe({ ...aufbau('', geschuetzt, '<p>{x}</p>\n'), huelle: HUELLE });
		expect([leer.stufe, leer.textlos, leer.blindeSeiten]).toEqual([0, 1, 0]);
		// Mit einem Text, der in einer geschützten Datei steht: grün.
		const pc = '<p>Ausführlicher Hinweis ausschließlich dieser Unterseite</p>\n';
		const gut = probe({
			...aufbau('', `${geschuetzt} "Ausführlicher Hinweis ausschließlich dieser Unterseite"`, pc),
			huelle: HUELLE
		});
		expect([gut.stufe, gut.blindeSeiten]).toEqual([0, 0]);
	});

	it('Seite eines Slugs, der ein Wort der Hülle ist: auch kurze Stücke zählen, nicht textlos', () => {
		const geschuetzt = `"${SATZ} mit"`;
		const pc = '<h2>Messprotokoll</h2>\n<button>Wert speichern</button>\n<p>{x}</p>\n';
		// Kurze Beschriftungen in einer öffentlichen Datei: ein Fund.
		const leck = probe({
			...aufbau('"Wert speichern"', `${geschuetzt} "Messprotokoll Wert speichern"`, pc),
			huelle: HUELLE
		});
		expect([leck.stufe, leck.textlos]).toEqual([1, 0]);
		const wort = probe({
			...aufbau('"Messprotokoll"', `${geschuetzt} "Messprotokoll Wert speichern"`, pc),
			huelle: HUELLE
		});
		expect(wort.stufe).toBe(1);
		// Nur geschützt: grün, gesehen.
		const gut = probe({
			...aufbau('', `${geschuetzt} "Messprotokoll Wert speichern"`, pc),
			huelle: HUELLE
		});
		expect([gut.stufe, gut.textlos, gut.blindeSeiten]).toEqual([0, 0, 0]);
		// Nirgends gesehen: blind statt textlos.
		const blind = probe({ ...aufbau('', geschuetzt, pc), huelle: HUELLE });
		expect([blind.stufe, blind.textlos, blind.blindeSeiten]).toEqual([2, 0, 1]);
		// Für Seiten anderer Slugs bleibt es bei Satzfolgen (der Slug selbst wird gesucht).
		expect(satzfolgen(pc)).toEqual([]);
		expect(satzfolgen(pc, true)).toEqual(['Messprotokoll', 'Wert speichern']);
		// Klassen und Importpfade sind keine kurzen Stücke, wohl aber zwei Wörter im Attribut.
		expect(
			satzfolgen(
				'<script>import A from "./Ansichtsteil.svelte";</script><div class="eineklasse" title="Zwei Worte">x</div>',
				true
			)
		).toEqual(['Zwei Worte']);
	});

	it('Kommentare im Skript sind kein Seitentext: eine Seite eines Wortes der Hülle mit Satz im Kommentar ist nicht blind', () => {
		const geschuetzt = `"${SATZ} mit"`;
		const text = 'Ausführlicher Hinweis ausschließlich dieser Unterseite';
		// Der Bau trägt nur den Text, nie die Kommentare (der Minifier wirft sie weg).
		const pc = [
			'<script lang="ts">',
			"\t// 'Ein kommentierter Satz steht nur hier'",
			'\t/* "Noch ein Satz im Blockkommentar hier" */',
			"\tconst weg = 'http://x/y'; // 'Satz hinter Code im Kommentar'",
			'</script>',
			`<p>${text}</p>`,
			''
		].join('\n');
		const e = probe({
			...aufbau('', `${geschuetzt} "${text}" "http://x/y"`, pc),
			huelle: HUELLE
		});
		expect([e.stufe, e.freieSeiten, e.blindeSeiten]).toEqual([0, 1, 0]);
		expect(satzfolgen(pc, true)).toEqual([text]);
		// Zeichenketten mit // oder /* bleiben.
		expect(baumTexte("<script>a = 'x // y /* z'; // w\n</script>").sonst).toEqual(['x // y /* z']);
	});

	it('Zeichenketten aus dem Syntaxbaum: Regex-Literale verschlucken nichts, Typen zählen nicht', () => {
		const s = [
			'<script lang="ts">',
			"\ttype Art = 'Ein Satz nur im Typ';",
			"\tconst a = s.replace(/^\\/*/, '') as 'Noch ein Satz im Typ';",
			"\tconst t = 'Erster echter Satz der Seite';",
			"\tconst b = 'x */ y';",
			"\tconst re = /https?:\\/\\//; const u = 'Zweiter echter Satz der Seite';",
			'\tconst q = s.replace(/"/g, \'\'); const v = "Dritter echter Satz der Seite";',
			"\tconst w = `Vorn ${x ? `innen` : 'b'} Vierter echter Satz der Seite`;",
			'</script>',
			'<script module>export const m = "Fünfter echter Satz im Modul";</script>'
		].join('\n');
		expect(satzfolgen(s)).toEqual([
			'Erster echter Satz der Seite',
			'Zweiter echter Satz der Seite',
			'Dritter echter Satz der Seite',
			'Vierter echter Satz der Seite',
			'Fünfter echter Satz im Modul'
		]);
		// Eine Seitendatei, die sich nicht lesen lässt, macht die Probe blind statt grün.
		const e = probe({
			...aufbau('', `"zqxprojekt ${SATZ} mit"`, '<script>const = ;</script>'),
			huelle: HUELLE
		});
		expect(e.stufe).toBe(2);
	});

	it('Texte des Markups aus dem Syntaxbaum: Ausdrücke, Attribut-Ausdrücke, Attribute in einfachen Anführungszeichen; Typ-Schlüssel nicht', () => {
		const s = [
			'<script lang="ts">',
			"\tinterface Props { 'Ein Schlüssel nur im Interface': string }",
			"\tlet { x }: { 'Noch ein Schlüssel im Typ': number } = $props();",
			'</script>',
			"<p>{c ? 'Erster Satz im Ausdruck hier' : 'Zweiter Satz im Ausdruck hier'}</p>",
			'<Knopf label={"Dritter Satz im Attribut-Ausdruck"} />',
			"<p title='Vierter Satz in einfachen Anführungszeichen'>{@html 'Fünfter Satz aus dem html-Tag'}</p>"
		].join('\n');
		expect(satzfolgen(s)).toEqual([
			'Erster Satz im Ausdruck hier',
			'Zweiter Satz im Ausdruck hier',
			'Dritter Satz im Attribut-Ausdruck',
			'Vierter Satz in einfachen Anführungszeichen',
			'Fünfter Satz aus dem html-Tag'
		]);
	});
});

describe('Aufruf der Dauerprobe (Kommandozeile und Bau)', () => {
	const still = { env: {}, aus: () => undefined };

	it('Rückgabe je Stufe, mit und ohne --nach-bau gleich', () => {
		const gruen = aufbau('', `"zqxprojekt ${SATZ} mit"`);
		const blind = aufbau('', 'export {};');
		const ohneListe = aufbau('', `"${SATZ} mit"`);
		rmSync(join(ohneListe.bau, '_app', 'oeffentlich.json'));
		for (const args of [[], ['--nach-bau'], ['build', '--nach-bau']]) {
			expect(hauptprogramm(args, { ...still, web: gruen.web })).toBe(0);
			expect(hauptprogramm(args, { ...still, web: blind.web })).toBe(2);
			expect(hauptprogramm(args, { ...still, web: ohneListe.web })).toBe(2);
		}
		// Der Bauordner auch aus DASHBOARD_BUILD_ZIEL (wie vite.config.ts).
		expect(
			hauptprogramm([], { ...still, web: blind.web, env: { DASHBOARD_BUILD_ZIEL: gruen.bau } })
		).toBe(0);
	});

	it('ein Fund: Rückgabe 1, die Liste heißt danach oeffentlich.json.fund (fail closed)', () => {
		const { bau, web } = aufbau('export const s = "zqxprojekt";', `"${SATZ} mit"`);
		const zeilen: string[] = [];
		expect(hauptprogramm(['--nach-bau'], { env: {}, web, aus: (z) => zeilen.push(z) })).toBe(1);
		expect(existsSync(join(bau, '_app', 'oeffentlich.json'))).toBe(false);
		expect(existsSync(join(bau, '_app', 'oeffentlich.json.fund'))).toBe(true);
		expect(zeilen.join('\n')).not.toContain('zqxprojekt');
		// Ohne Liste prüft der nächste Lauf nicht mehr grün.
		expect(hauptprogramm([], { ...still, web })).toBe(2);
	});

	it('ein Fund außerhalb von _app/: die Datei liegt danach unter _app/fund/ (fail closed)', () => {
		const { bau, web } = aufbau('', `"${SATZ} mit"`);
		writeFileSync(join(bau, 'index.html'), '<title>zqxprojekt</title>');
		writeFileSync(join(bau, 'robots.txt'), 'User-agent: *');
		expect(hauptprogramm([], { ...still, web })).toBe(1);
		expect(existsSync(join(bau, 'index.html'))).toBe(false);
		expect(readFileSync(join(bau, '_app', 'fund', 'index.html'), 'utf8')).toContain('zqxprojekt');
		// Was nichts verrät, bleibt.
		expect(existsSync(join(bau, 'robots.txt'))).toBe(true);
		expect(existsSync(join(bau, '_app', 'oeffentlich.json.fund'))).toBe(true);
	});

	it('blind: Rückgabe 2 mit der Marke _app/oeffentlich.blind; der nächste Lauf entfernt sie', () => {
		const { bau, web } = aufbau('', 'export {};');
		const marke = join(bau, '_app', 'oeffentlich.blind');
		expect(hauptprogramm([], { ...still, web })).toBe(2);
		expect(readFileSync(marke, 'utf8')).toContain('blind');
		writeFileSync(
			join(bau, '_app', 'immutable', 'chunks', 'geschuetzt.js'),
			`"zqxprojekt ${SATZ} mit"`
		);
		expect(hauptprogramm([], { ...still, web })).toBe(0);
		expect(existsSync(marke)).toBe(false);
		// Ein Fund ist kein „blind“: keine Marke.
		writeFileSync(join(bau, '_app', 'immutable', 'chunks', 'oeffentlich.js'), '"zqxprojekt"');
		expect(hauptprogramm([], { ...still, web })).toBe(1);
		expect(existsSync(marke)).toBe(false);
	});

	it('ein Fund, Umbenennen geht nicht: Liste und Fund-Datei werden gelöscht, mit Satz', () => {
		const { bau, web } = aufbau('', `"${SATZ} mit"`);
		writeFileSync(join(bau, 'index.html'), '<title>zqxprojekt</title>');
		// Am Ziel steht schon ein Ordner mit Inhalt: renameSync scheitert.
		mkdirSync(join(bau, '_app', 'oeffentlich.json.fund', 'x'), { recursive: true });
		mkdirSync(join(bau, '_app', 'fund', 'index.html', 'x'), { recursive: true });
		const zeilen: string[] = [];
		expect(hauptprogramm([], { env: {}, web, aus: (z) => zeilen.push(z) })).toBe(1);
		expect(existsSync(join(bau, '_app', 'oeffentlich.json'))).toBe(false);
		expect(existsSync(join(bau, 'index.html'))).toBe(false);
		expect(zeilen.filter((z) => z.includes('umbenennen ging nicht'))).toHaveLength(2);
		expect(zeilen.join('\n')).not.toContain('NICHT verschlossen');
		expect(zeilen.join('\n')).not.toContain('zqxprojekt');
	});

	it('ein Fund, die Liste lässt sich weder umbenennen noch löschen: eine Marke oeffentlich.json.fund für die Skripte', () => {
		const { bau, web } = aufbau('export const s = "zqxprojekt";', `"${SATZ} mit"`);
		const zeilen: string[] = [];
		sperre.an = true;
		try {
			expect(hauptprogramm([], { env: {}, web, aus: (z) => zeilen.push(z) })).toBe(1);
		} finally {
			sperre.an = false;
		}
		expect(existsSync(join(bau, '_app', 'oeffentlich.json'))).toBe(true);
		expect(readFileSync(join(bau, '_app', 'oeffentlich.json.fund'), 'utf8')).toContain(
			'Öffentliche Dateien nennen'
		);
		expect(zeilen.join('\n')).toContain('NICHT verschlossen');
		expect(zeilen.join('\n')).not.toContain('zqxprojekt');
	});

	it('npm run build ruft die Probe nach vite build auf', () => {
		const paket = JSON.parse(
			readFileSync(fileURLToPath(new URL('../package.json', import.meta.url)), 'utf8')
		) as { scripts: Record<string, string> };
		const schritte = (paket.scripts.build ?? '').split('&&').map((s) => s.trim());
		const vite = schritte.indexOf('vite build');
		expect(vite).toBeGreaterThanOrEqual(0);
		expect(schritte.slice(vite + 1)).toContain('node scripts/oeffentlich-probe.mjs');
	});
});
