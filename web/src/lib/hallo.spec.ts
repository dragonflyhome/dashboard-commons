import { describe, expect, it } from 'vitest';
import { alsHallo, zerlegen } from './hallo.ts';

describe('alsHallo', () => {
	it('nimmt nur Textfelder', () => {
		expect(alsHallo({ titel: 'Hallo 👋', text: 'x', datum: 3, fremd: 'y' })).toEqual({
			titel: 'Hallo 👋',
			text: 'x'
		});
		expect(alsHallo(null)).toEqual({});
		expect(alsHallo([1, 2])).toEqual({});
	});
});

describe('zerlegen', () => {
	it('trennt Absätze an Leerzeilen und behält Zeilenumbrüche und Emojis', () => {
		expect(zerlegen('Hallo 👋 🙏\n\nEin Gruß 🌼\nfür den Anfang.\n')).toEqual([
			{ art: 'absatz', text: 'Hallo 👋 🙏' },
			{ art: 'absatz', text: 'Ein Gruß 🌼\nfür den Anfang.' }
		]);
	});

	it('macht aus „\\t1.\\t…“ eine nummerierte Liste', () => {
		const text = 'Check:\n\t1.\tWerkzeug bereitlegen 🧰.\n\t2.\tPlan lesen.\n\nDanach.';
		expect(zerlegen(text)).toEqual([
			{ art: 'absatz', text: 'Check:' },
			{
				art: 'liste',
				nummeriert: true,
				punkte: ['Werkzeug bereitlegen 🧰.', 'Plan lesen.']
			},
			{ art: 'absatz', text: 'Danach.' }
		]);
	});

	it('Striche werden eine einfache Liste, Einrückung setzt einen Punkt fort', () => {
		expect(zerlegen('- eins\n  weiter\n- zwei')).toEqual([
			{ art: 'liste', nummeriert: false, punkte: ['eins weiter', 'zwei'] }
		]);
	});

	it('leerer Text ergibt nichts', () => {
		expect(zerlegen('')).toEqual([]);
		expect(zerlegen('\r\n\r\n')).toEqual([]);
	});
});
