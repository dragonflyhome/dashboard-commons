/**
 * Etappe B2: Die Liste der geschützten Dateien in `daten/`, die die
 * Oberfläche ohne Anmeldung gar nicht erst anfragt (`GESCHUETZTE_DATEN` in
 * laden.svelte.ts), ist dieselbe wie im Server (`sicherheit::GESCHUETZTE_DATEIEN`,
 * die Liste hinter `geschuetzt`). Eine Liste aus
 * dem Vertrag gibt es dafür nicht (die Regel ist eine Funktion, kein Typ);
 * darum liest dieser Test die Quelle des Servers.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { GESCHUETZTE_DATEN, datenAdresse } from '#lib/projekt/laden.svelte.ts';

const SICHERHEIT = readFileSync(
	join(import.meta.dirname, '..', '..', 'src', 'sicherheit.rs'),
	'utf8'
);

/**
 * Die geschützten Dateien des Servers: seit dem Hotfix Prozent die eine Liste
 * `GESCHUETZTE_DATEIEN` in sicherheit.rs (aus ihr lesen `geschuetzt`, der
 * Handler `daten` und `/api/projects/<slug>`). Einträge sind Texte oder Namen
 * von Konstanten (etwa `FRAGEN_DATEI`); ein unbekannter Eintrag bleibt ein Fehler.
 */
function serverDaten(): string[] {
	const konstanten = new Map(
		[...SICHERHEIT.matchAll(/pub const ([A-Z_]+): &str = "([^"]+)";/g)].map((m) => [m[1], m[2]])
	);
	const liste = /pub const GESCHUETZTE_DATEIEN: \[&str; \d+\] = \[([^\]]*)\];/.exec(SICHERHEIT);
	if (!liste) return ['unbekannt: GESCHUETZTE_DATEIEN fehlt'];
	return (liste[1] ?? '')
		.split(',')
		.map((t) => t.trim())
		.filter((t) => t !== '')
		.map((teil) =>
			teil.startsWith('"') ? teil.slice(1, -1) : (konstanten.get(teil) ?? `unbekannt: ${teil}`)
		);
}

describe('Geschützte Daten: Server und Oberfläche gleich', () => {
	it('dieselben Dateien wie sicherheit::geschuetzt', () => {
		const server = serverDaten();
		expect(server.length).toBeGreaterThan(0);
		expect([...GESCHUETZTE_DATEN].sort()).toEqual([...server].sort());
	});

	it('die Adressen der Dateien passen zum Muster des Servers', () => {
		for (const name of GESCHUETZTE_DATEN) {
			expect(datenAdresse('beispiel', name)).toBe(`/api/projekte/beispiel/daten/${name}`);
		}
	});
});

/** Die Wurzeln, unter denen der Server nur mit Windows-Anmeldung antwortet (`WURZELN` in `geschuetzt`). */
function serverWurzeln(): string[] {
	const liste = /const WURZELN: \[&str; \d+\] = \[([\s\S]*?)\];/.exec(SICHERHEIT);
	return [...(liste?.[1] ?? '').matchAll(/^\s*"([^"]+)",/gm)].map((m) => m[1] ?? '');
}

describe('System-Seiten: Daten geschützt, die Seiten zeigen ohne Anmeldung den Weg über Edge', () => {
	it.each([
		['entscheidungen', '/api/entscheidungen'],
		['zielbild', '/api/zielbild'],
		// Etappe 6b: die Anleitung aus docs/anleitung, geschützt wie das Zielbild.
		['anleitung', '/api/anleitung']
	])('/%s lädt %s, und der Weg ist geschützt', (seite, weg) => {
		expect(serverWurzeln()).toContain(weg);
		const laden = readFileSync(join(import.meta.dirname, 'routes', seite, '+page.ts'), 'utf8');
		expect(laden).toMatch(/e\.status === 401/);
		expect(laden).toMatch(/anmelden: SATZ_[A-Z]+_ANMELDEN/);
	});
});
