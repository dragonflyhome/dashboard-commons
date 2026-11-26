/**
 * Etappe B2: Die Dateien in `daten/`, die die Oberfläche ohne Anmeldung gar
 * nicht erst anfragt (`GESCHUETZTE_DATEN` in laden.svelte.ts), verlangen auch
 * im Server die Anmeldung. Seit Schritt E2 ist der ganze Weg ohne Anmeldung
 * zu (die Seiten dazu leiten zur Anmeldung, siehe
 * routes/zur-anmeldung.svelte.spec.ts); seit Schritt E3 weist der Handler
 * `daten` (src/api/neue_form.rs) als zweite Wand jede Datei ohne Anmeldung ab,
 * nicht nur eine Liste. Eine Regel aus dem Vertrag gibt es dafür nicht; darum
 * liest dieser Test die Quelle des Servers.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { GESCHUETZTE_DATEN, datenAdresse } from '#lib/projekt/laden.svelte.ts';

const HANDLER = readFileSync(
	join(import.meta.dirname, '..', '..', 'src', 'api', 'neue_form.rs'),
	'utf8'
);

describe('Geschützte Daten: Server und Oberfläche', () => {
	it('der Handler daten weist jede Datei ohne Anmeldung ab, ohne Liste davor', () => {
		const daten = /pub async fn daten\([^)]*\)[^{]*\{([\s\S]*?)\n\}/.exec(HANDLER)?.[1] ?? '';
		expect(daten).toMatch(/\n\s*if angemeldet\.is_none\(\) \{\n[^}]*bitte_anmelden\(None\)/);
		expect(daten).not.toMatch(/geschuetzt|GESCHUETZT/);
		// Die Oberfläche fragt die persönlichen Dateien ohne Anmeldung gar nicht erst an.
		expect([...GESCHUETZTE_DATEN].sort()).toEqual(['fragen', 'fristen', 'gebete', 'standorte']);
	});

	it('die Adressen der Dateien passen zum Muster des Servers', () => {
		for (const name of GESCHUETZTE_DATEN) {
			expect(datenAdresse('beispiel', name)).toBe(`/api/projekte/beispiel/daten/${name}`);
		}
	});
});
