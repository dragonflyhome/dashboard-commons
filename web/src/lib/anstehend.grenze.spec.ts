/**
 * Etappe 3a+ (Grenze „Zustand im Server, Auswertung im Frontend“): Anstehend
 * wird an genau einer Stelle geordnet, ausgewählt, gruppiert und gezählt,
 * in web/src/lib/anstehend.ts. Dieser Wächter sucht in Seiten, Komponenten
 * und Widgets nach `.sort(` und `.toSorted(`; erlaubt sind nur die Dateien
 * unten, die nichts aus Anstehend ordnen (mit Grund).
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC = join(import.meta.dirname, '..');

/** Wo Seiten und Komponenten liegen, dazu die Lade-Hilfen rund um Anstehend. */
const ORTE = [
	'routes',
	'lib/components',
	'lib/katalog',
	'lib/unterdashboard.ts',
	'lib/anstehend.svelte.ts'
];

/** Erlaubt, weil sie keine Anstehend-Daten ordnen (Pfad relativ zu web/src, mit /). */
const ERLAUBT: Readonly<Record<string, string>> = {
	'lib/components/projekt/NeueFormRahmen.svelte': 'Unterseiten nach order',
	'lib/components/projekt/NeueForm.svelte': 'Unterseiten nach order',
	'lib/components/projekt/Standardaufbau.svelte': 'Dateien in daten/ nach fester Reihenfolge'
};

const SORTIEREN = /\.(?:sort|toSorted)\(/;

function dateien(pfad: string): string[] {
	if (statSync(pfad).isFile()) return [pfad];
	return readdirSync(pfad).flatMap((n) => dateien(join(pfad, n)));
}

function quellen(): string[] {
	return ORTE.flatMap((o) => dateien(join(SRC, o)))
		.filter((p) => /\.(svelte|ts)$/.test(p) && !/\.(spec|test)\.ts$/.test(p))
		.map((p) => relative(SRC, p).split('\\').join('/'));
}

describe('Anstehend wird nur in anstehend.ts geordnet (Etappe 3a+)', () => {
	it('keine Seite und keine Komponente sortiert selbst (außer der Erlaubnisliste)', () => {
		const funde = quellen().filter(
			(p) => !(p in ERLAUBT) && SORTIEREN.test(readFileSync(join(SRC, p), 'utf8'))
		);
		expect(funde, 'Sortieren gehört nach web/src/lib/anstehend.ts').toEqual([]);
	});

	it('die Erlaubnisliste ist aktuell: jede Datei gibt es und sie sortiert wirklich', () => {
		for (const p of Object.keys(ERLAUBT)) {
			expect(SORTIEREN.test(readFileSync(join(SRC, p), 'utf8')), p).toBe(true);
		}
	});

	it('der Wächter sieht genug: Seiten, Komponenten und Widgets sind dabei', () => {
		const q = quellen();
		expect(q).toContain('routes/anstehend/+page.svelte');
		expect(q).toContain('lib/components/projekt/ProjektTeile.svelte');
		expect(q).toContain('lib/components/anstehend/JetztFuerDich.svelte');
		expect(q).toContain('lib/katalog/widgets/worte.ts');
		expect(q.length).toBeGreaterThan(40);
	});

	it('die Kopfleisten-Zahl kommt aus derselben Auswertung', () => {
		const speicher = readFileSync(join(SRC, 'lib/anstehend.svelte.ts'), 'utf8');
		expect(speicher).toContain('auswerten(l.liste)');
		expect(speicher).toContain('this.liste?.zaehler.jetzt');
	});

	/**
	 * Abnahme 3a+ (Auflage): auch Zählungen über Einträge (`.filter(…).length`)
	 * gehören nach anstehend.ts. Geprüft in Dateien, die Anstehend-Daten
	 * einbinden (Typen AnstehendEintrag oder Aktualisierung, anstehend.ts,
	 * unterdashboard.ts). Andere Zählungen (Schlüssel, PC-Schritte, Kapitel)
	 * sind keine Anstehend-Daten und bleiben außen vor.
	 */
	it('keine Seite und keine Komponente zählt Anstehend-Daten selbst (.filter(…).length)', () => {
		const ZAEHLEN = /\.filter\((?:[^()]|\((?:[^()]|\([^()]*\))*\))*\)\s*\.length/;
		const ANSTEHEND =
			/import[^;]*\b(?:AnstehendEintrag|Aktualisierung)\b|from '(?:#lib|\.)\/(?:anstehend|unterdashboard)\.ts'/;
		const ZAEHLEN_ERLAUBT: Readonly<Record<string, string>> = {
			'lib/katalog/widgets/Frage.svelte':
				'zählt nur, wie viele Gruppen (Fragen, Aufgaben, Arbeiten) da sind'
		};
		const funde = quellen().filter((p) => {
			if (p in ZAEHLEN_ERLAUBT) return false;
			const text = readFileSync(join(SRC, p), 'utf8');
			return ANSTEHEND.test(text) && ZAEHLEN.test(text);
		});
		expect(funde, 'Zählen gehört nach web/src/lib/anstehend.ts').toEqual([]);
		for (const p of Object.keys(ZAEHLEN_ERLAUBT)) {
			expect(ZAEHLEN.test(readFileSync(join(SRC, p), 'utf8')), p).toBe(true);
		}
		// Der Wächter erkennt das alte Muster des Unter-Dashboards.
		expect(
			ZAEHLEN.test("aktualisierungen.filter((a) => a.quelle === 'meldung' && !a.gelesen).length")
		).toBe(true);
		expect(ZAEHLEN.test('x.filter((e) => e.art === "frage")\n\t\t.length')).toBe(true);
	});
});

/**
 * Etappe 3b, Startseite als Spalten: Projekte werden wie Anstehend an genau
 * einer Stelle gesucht, gefiltert, sortiert, gezählt und nach Status
 * gruppiert, in web/src/lib/projekte.ts (`auswerten`). Wer von dort liest
 * (Startseite, Spalten), rechnet nicht selbst nach.
 */
describe('Projekte werden nur in projekte.ts ausgewertet (Etappe 3b)', () => {
	const PROJEKTE = /from '(?:#lib|\.\.?)\/(?:lib\/)?projekte\.ts'/;
	const SELBST_RECHNEN = /\.(?:sort|toSorted|filter|reduce|reduceRight)\(|new Set\(/;

	it('Startseite und Spalten lesen aus projekte.ts und rechnen nicht selbst', () => {
		const leser = quellen().filter((p) => PROJEKTE.test(readFileSync(join(SRC, p), 'utf8')));
		expect(leser).toContain('routes/+page.svelte');
		expect(leser).toContain('lib/components/ProjektSpalten.svelte');
		const funde = leser.filter((p) => SELBST_RECHNEN.test(readFileSync(join(SRC, p), 'utf8')));
		expect(funde, 'Projekte auswerten gehört nach web/src/lib/projekte.ts').toEqual([]);
	});

	it('die Startseite zählt die Spalten nicht selbst und hat keine zweite Zählstelle', () => {
		const seite = readFileSync(join(SRC, 'routes/+page.svelte'), 'utf8');
		expect(seite).toContain('auswerten(data.projects');
		expect(seite).not.toMatch(/data\.projects\s*\./);
		// Die alte Zählstelle gibt es nicht mehr.
		expect(() => statSync(join(SRC, 'lib/zaehlung.ts'))).toThrow();
		// Der Wächter erkennt das alte Muster der Startseite.
		expect(SELBST_RECHNEN.test("data.projects.filter((p) => status === 'alle')")).toBe(true);
	});
});

/**
 * Etappe 4c (Plan 4: „Bau-Stand auf der Startseite nur über die eine
 * Auswertungsstelle, keine zweite Zählung“): Den Bau-Stand werten nur
 * `bauAnzeige` in projekte.ts aus. Seiten und Komponenten
 * lesen weder die Zeit (`letzte_gute`) noch formatieren sie sie, und sie
 * zählen rote Seiten nicht selbst.
 */
describe('Bau-Stand nur in projekte.ts (Etappe 4c)', () => {
	it('keine Seite und keine Komponente liest letzte_gute, formatiert sie oder zählt rote Seiten', () => {
		const SELBST = /letzte_gute|fmtTagUhrzeit|seitenRot|zustand === 'rot'\)\.length/;
		const funde = quellen().filter((p) => SELBST.test(readFileSync(join(SRC, p), 'utf8')));
		expect(funde, 'Den Bau-Stand wertet nur web/src/lib/projekte.ts aus').toEqual([]);
	});

	it('Startseite: Bau-Stand und Zählzeile aus derselben Auswertung', () => {
		const seite = readFileSync(join(SRC, 'routes/+page.svelte'), 'utf8');
		expect(seite).toContain(
			// Entscheidung projektgruppe: dazu die Gruppen aus der Projektliste.
			'auswerten(data.projects, { suche: query, status, sortierung: sort }, bauLage.live, data.gruppen)'
		);
		expect(seite).toContain('bau={a.bau[k.projekt.slug] ?? null}');
		const projekte = readFileSync(join(SRC, 'lib/projekte.ts'), 'utf8');
		expect(projekte).toContain('const bau = bauJeProjekt(projekte, live);');
		expect(projekte).toContain(
			'const zaehlung = zaehleProjekte(alleKacheln.map(zaehlbar(bau)), bau);'
		);
	});
});
