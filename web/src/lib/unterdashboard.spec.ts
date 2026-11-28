/**
 * Der Stand an der Grenze (`ladeStand`, `ladeUnterDaten`): Eine Antwort einer
 * anderen Fassung des Dienstes gilt als nicht geladen, und die Auswertung
 * wirft nie. Gegenprobe: der vollständige Stand gilt.
 */
import { describe, expect, it, vi } from 'vitest';
import { beispielProjektStand, json } from '#lib/anstehend.beispiel.ts';
import { istProjektStand, ladeStand, ladeUnterDaten } from '#lib/unterdashboard.ts';

/** /stand/voll und /stand antworten mit `stand`, /api/anstehend mit 401. */
function netz(stand: unknown) {
	return vi.fn((eingabe: RequestInfo | URL) => {
		const url = typeof eingabe === 'string' ? eingabe : '';
		return Promise.resolve(
			url.includes('/stand') ? json(stand) : json({ fehler: 'Anmeldung' }, 401)
		);
	});
}

describe('Stand an der Grenze', () => {
	it('ohne die Felder des Stands: null, kein halber Wert', async () => {
		for (const roh of [
			{ slug: 'p', sichtbar: false },
			null,
			[],
			{ ...beispielProjektStand(), gesamt: null }
		]) {
			expect(istProjektStand(roh)).toBe(false);
			expect(await ladeStand('p', netz(roh))).toBeNull();
		}
		// Gegenprobe: der vollständige Stand gilt.
		const gut = beispielProjektStand();
		expect(istProjektStand(gut)).toBe(true);
		expect(await ladeStand('p', netz(gut))).toEqual(gut);
	});

	it('zuletzt darf null sein (Änderungszeit unbekannt), aber keine Zahl', () => {
		const gut = beispielProjektStand();
		expect(istProjektStand({ ...gut, gesamt: { ...gut.gesamt, zuletzt: null } })).toBe(true);
		expect(istProjektStand({ ...gut, gesamt: { ...gut.gesamt, zuletzt: 1 } })).toBe(false);
	});

	it('wirft die Auswertung trotzdem, ist der Stand nicht geladen und nichts wirft', async () => {
		// Die Form stimmt, ein Eintrag aber nicht: die Auswertung liest e.art.
		const kaputt = { ...beispielProjektStand(), sichtbar: true, eintraege: [null] };
		expect(istProjektStand(kaputt)).toBe(true);
		expect((await ladeUnterDaten('p', netz(kaputt))).stand).toBeNull();
		// Gegenprobe: der vollständige Stand wird ausgewertet.
		expect((await ladeUnterDaten('p', netz(beispielProjektStand()))).stand).not.toBeNull();
	});
});

describe('Stand nur über /stand/voll (Schritt E3: kein freier Rückfall mehr)', () => {
	it.each([401, 403])('nach %i: null und kein zweiter Abruf', async (status) => {
		const f = vi.fn(() => Promise.resolve(json({ fehler: 'Anmeldung' }, status)));
		expect(await ladeStand('p', f)).toBeNull();
		expect(f.mock.calls.map((c: unknown[]) => c[0])).toEqual(['/api/projekte/p/stand/voll']);
	});
});
