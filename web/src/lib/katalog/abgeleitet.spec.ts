import { describe, expect, it } from 'vitest';
import type { ListeAbgeleitet } from '$vertrag';
import { erledigtSatz, istGeladen, listenPfad, naechsterSatz, tageSatz } from './abgeleitet.ts';

const liste: ListeAbgeleitet = {
	anzahl: 3,
	erledigt: { feld: 'erledigt', anzahl: 2, anteil: 2 / 3 },
	summe: {},
	datum: { termin: { tage_bis: [-3, 0, 12], tage_seit: [3, 0, -12], naechster: 1, letzter: 0 } },
	marken: {}
};

describe('Hilfen für Katalog-Bausteine', () => {
	it('Pfad einer Liste, bei Teilen mit teil.<name>.', () => {
		expect(listenPfad('balken', null)).toBe('balken');
		expect(listenPfad('balken', 'plan')).toBe('teil.plan.balken');
	});
	it('Erledigtes nur, wenn der Server es zählen konnte', () => {
		expect(erledigtSatz(liste)).toBe('2 von 3 erledigt');
		expect(erledigtSatz({ ...liste, erledigt: null })).toBeNull();
		expect(erledigtSatz({ ...liste, anzahl: 0 })).toBeNull();
		expect(erledigtSatz(null)).toBeNull();
	});
	it('Tage als Wörter', () => {
		expect(tageSatz(0)).toBe('heute');
		expect(tageSatz(1)).toBe('morgen');
		expect(tageSatz(-1)).toBe('gestern');
		expect(tageSatz(12)).toBe('in 12 Tagen');
		expect(tageSatz(-4)).toBe('vor 4 Tagen');
	});
	it('nächster Termin aus der Datumsspalte', () => {
		const titel = ['A', 'B', 'C'];
		expect(naechsterSatz(liste, 'termin', (i) => titel[i])).toBe('Als Nächstes: B, heute');
		expect(naechsterSatz(liste, 'fehlt', (i) => titel[i])).toBeNull();
		expect(naechsterSatz(null, 'termin', (i) => titel[i])).toBeNull();
	});
	it('erkennt das Objekt aus ladeDaten', () => {
		expect(istGeladen({ kennzahl: [] })).toBe(false);
		expect(
			istGeladen({
				zustand: 'laedt',
				inhalt: null,
				abgeleitet: null,
				satz: null,
				neuLaden: () => Promise.resolve()
			})
		).toBe(true);
	});
});
