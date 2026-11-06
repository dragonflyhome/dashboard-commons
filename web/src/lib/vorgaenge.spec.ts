import { describe, expect, it } from 'vitest';
import {
	artWort,
	fmtRelativ,
	fmtZeitpunkt,
	herkunftWort,
	istUeberfaellig,
	seitText,
	zustandBedeutung
} from './vorgaenge.ts';
import type { Vorgang } from '$vertrag';

/** „Jetzt“ in Ortszeit: 26.10.2026, 12:00. */
const jetzt = new Date(2026, 9, 26, 12, 0, 0);
const vor = (ms: number) => new Date(jetzt.getTime() - ms).toISOString();
const MIN = 60_000;
const STD = 60 * MIN;

const vorgang = (teil: Partial<Vorgang>): Vorgang => ({
	id: 'v-20261026-0001',
	art: 'frage',
	slug: 'beispiel',
	titel: 'Schlusssatz?',
	bezug: null,
	zustand: 'offen',
	erstellt: vor(5 * MIN),
	geaendert: vor(5 * MIN),
	letzte_nr: 1,
	von: 'chat:beispiel',
	notiz: null,
	...teil
});

describe('fmtRelativ', () => {
	it('sagt Minuten, Stunden, gestern und sonst das Datum', () => {
		expect(fmtRelativ(vor(20_000), jetzt)).toBe('gerade eben');
		expect(fmtRelativ(vor(3 * MIN), jetzt)).toBe('vor 3 Min.');
		expect(fmtRelativ(vor(2 * STD + 10 * MIN), jetzt)).toBe('vor 2 Std.');
		expect(fmtRelativ(new Date(2026, 9, 25, 8, 0).toISOString(), jetzt)).toBe('gestern');
		expect(fmtRelativ(new Date(2026, 9, 20, 8, 0).toISOString(), jetzt)).toBe('20.10.2026');
	});
	it('lässt Fehlendes und Unlesbares stehen', () => {
		expect(fmtRelativ(null, jetzt)).toBe('–');
		expect(fmtRelativ('kaputt', jetzt)).toBe('kaputt');
	});
});

describe('fmtZeitpunkt', () => {
	it('zeigt heute nur die Uhrzeit', () => {
		expect(fmtZeitpunkt(new Date(2026, 9, 26, 9, 14).toISOString(), jetzt)).toBe('09:14 Uhr');
		expect(fmtZeitpunkt(new Date(2026, 9, 25, 9, 14).toISOString(), jetzt)).toBe(
			'gestern 09:14 Uhr'
		);
		expect(fmtZeitpunkt(new Date(2026, 9, 1, 9, 14).toISOString(), jetzt)).toBe(
			'01.10.2026, 09:14 Uhr'
		);
	});
});

describe('Vorgangs-Helfer', () => {
	it('ordnet jedem Zustand eine Farbe zu', () => {
		expect(zustandBedeutung('offen')).toBe('wartet');
		expect(zustandBedeutung('zugestellt')).toBe('neutral');
		expect(zustandBedeutung('uebernommen')).toBe('laeuft');
		expect(zustandBedeutung('erledigt')).toBe('ok');
	});
	it('markiert nur Vorgänge, die über einen Tag zugestellt sind', () => {
		expect(
			istUeberfaellig(vorgang({ zustand: 'zugestellt', geaendert: vor(25 * STD) }), jetzt)
		).toBe(true);
		expect(
			istUeberfaellig(vorgang({ zustand: 'zugestellt', geaendert: vor(2 * STD) }), jetzt)
		).toBe(false);
		expect(istUeberfaellig(vorgang({ zustand: 'offen', geaendert: vor(50 * STD) }), jetzt)).toBe(
			false
		);
	});
	it('schreibt „seit wann“ und die Herkunft in einfachen Worten', () => {
		expect(seitText(vorgang({}), jetzt)).toBe('entstanden vor 5 Min.');
		expect(
			seitText(
				vorgang({ zustand: 'erledigt', geaendert: new Date(2026, 9, 20).toISOString() }),
				jetzt
			)
		).toBe('erledigt am 20.10.2026');
		expect(herkunftWort('browser')).toBe('dir (Browser)');
		expect(herkunftWort('chat:dashboard')).toBe('Chat dashboard');
	});
});

describe('artWort', () => {
	it('nennt auch Merge-Requests (Etappe 4b)', () => {
		expect(artWort('merge')).toBe('Merge-Request');
		expect(artWort('passkey-antrag')).toBe('Passkey-Antrag');
		expect(artWort('oeffnen')).toBe('Geöffnet');
		expect(artWort('abnahme')).toBe('Abnahme');
		expect(artWort('schritt')).toBe('Teilschritt');
	});
});
