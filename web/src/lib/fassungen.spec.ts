import { describe, expect, it } from 'vitest';
import type { FassungsZeilen } from '$vertrag';
import { abschnitteMitZeilen, bruchTeilen, fussText, hatZeilen } from './fassungen.ts';

const leer: FassungsZeilen = {
	hinzugefuegt: [],
	geaendert: [],
	entfernt: [],
	behoben: [],
	brueche: []
};

describe('Fassungen', () => {
	it('zeigt nur Unterabschnitte mit Zeilen, in der Reihenfolge des Changelogs', () => {
		const z = { ...leer, behoben: ['B'], hinzugefuegt: ['A1', 'A2'] };
		expect(abschnitteMitZeilen(z)).toEqual([
			{ titel: 'Hinzugefügt', zeilen: ['A1', 'A2'] },
			{ titel: 'Behoben', zeilen: ['B'] }
		]);
		expect(hatZeilen(z)).toBe(true);
		// Brüche stehen auch unter Geändert oder Entfernt; allein zählen sie nicht.
		expect(hatZeilen({ ...leer, brueche: ['Bruch: x'] })).toBe(false);
		expect(abschnitteMitZeilen(leer)).toEqual([]);
	});

	it('teilt das Wort Bruch ab', () => {
		const b = ['Bruch: Alte Form weg.'];
		expect(bruchTeilen('Bruch: Alte Form weg.', b)).toEqual({
			bruch: true,
			rest: 'Alte Form weg.'
		});
		// Unter Hinzugefügt oder Behoben ist es kein Bruch (der Dienst nennt ihn nicht).
		expect(bruchTeilen('Bruch: anderswo', b)).toEqual({ bruch: false, rest: 'Bruch: anderswo' });
		expect(bruchTeilen('Kein Bruch: nur Text', b)).toEqual({
			bruch: false,
			rest: 'Kein Bruch: nur Text'
		});
	});

	it('nennt die laufende Fassung, zwischen zwei Fassungen mit Unveröffentlicht', () => {
		expect(fussText('0.2.0', false)).toBe('Fassung 0.2.0');
		expect(fussText('0.2.0', true)).toBe('Fassung 0.2.0 + Unveröffentlicht');
	});
});
