import { describe, expect, it } from 'vitest';
import {
	begrenzen,
	beschriften,
	fensterFuerStufe,
	marken,
	passendeStufe,
	position,
	schrittFuer,
	textBreite,
	zeitpunkt,
	zoomen,
	type Fenster
} from './achse';
import { tagAusZahl, tagZahl, wochenanfang, wochentag } from './tage';

const tag = (t: string): number => tagZahl(t);

describe('Tage', () => {
	it('rechnet ohne Zeitumstellung', () => {
		expect(tagAusZahl(tagZahl('2026-10-25') + 1)).toBe('2026-10-26');
		expect(tagAusZahl(tagZahl('2026-03-29'))).toBe('2026-03-29');
		expect(wochentag('2026-10-30')).toBe(5);
		expect(wochenanfang('2026-10-30')).toBe('2026-10-26');
		expect(wochenanfang('2026-10-26')).toBe('2026-10-26');
		expect(wochenanfang('2026-11-01')).toBe('2026-10-26');
	});
});

describe('Zeit und Position', () => {
	const f: Fenster = { von: 100, bis: 200 };
	it('hin und zurück', () => {
		expect(position(f, 500, 150)).toBe(250);
		expect(zeitpunkt(f, 500, 250)).toBe(150);
	});
	it('Zoom um einen Anker und Begrenzung', () => {
		expect(zoomen(f, 2, 0.5)).toEqual({ von: 125, bis: 175 });
		expect(begrenzen({ von: 50, bis: 100 }, f)).toEqual({ von: 100, bis: 150 });
		expect(begrenzen({ von: 0, bis: 500 }, f)).toEqual(f);
		expect(begrenzen({ von: Number.NaN, bis: 3 }, f, { von: 110, bis: 120 })).toEqual({
			von: 110,
			bis: 120
		});
	});
});

describe('Stufen', () => {
	it('die Stufe legt die Länge des Ausschnitts fest', () => {
		expect(fensterFuerStufe('tag', 440, 1000)).toEqual({ von: 995, bis: 1005 });
		expect(schrittFuer('woche')).toBe(7);
		expect(schrittFuer('monat', true)).toBe(1);
	});
	it('passende Stufe: feiner, je mehr Platz je Tag', () => {
		expect(passendeStufe({ von: 0, bis: 14 }, 800)).toBe('tag');
		expect(passendeStufe({ von: 0, bis: 90 }, 800)).toBe('woche');
		expect(passendeStufe({ von: 0, bis: 400 }, 800)).toBe('monat');
		expect(passendeStufe({ von: 0, bis: 3000 }, 800)).toBe('jahr');
	});
	it('Marken je Stufe', () => {
		const f = { von: tag('2026-10-29'), bis: tag('2026-11-02') };
		expect(marken(f, 'tag').map((m) => m.text)).toEqual([
			'Do 29.',
			'Fr 30.',
			'Sa 31.',
			'1. Nov.',
			'Mo 2.'
		]);
		const w = { von: tag('2026-10-20'), bis: tag('2026-11-10') };
		expect(marken(w, 'woche').map((m) => m.text)).toEqual(['26.10.', '2.11.', '9.11.']);
		const m = { von: tag('2026-10-15'), bis: tag('2027-02-15') };
		expect(marken(m, 'monat').map((x) => x.text)).toEqual(['Nov.', 'Dez.', '2027', 'Feb.']);
		const j = { von: tag('2019-06-01'), bis: tag('2022-06-01') };
		expect(marken(j, 'jahr').map((x) => x.text)).toEqual(['2020', '2021', '2022']);
		expect(marken({ von: 0, bis: 6 }, 'jahr', true).map((x) => x.text)).toEqual([
			'Monat 0',
			'Monat 3',
			'Monat 6'
		]);
		expect(marken({ von: 1, bis: Number.NaN }, 'tag')).toEqual([]);
	});
});

describe('Beschriftung ohne Überlappung', () => {
	it('lässt Beschriftungen weg, die sich überdecken würden; Hauptmarken zuerst', () => {
		const f = { von: tag('2026-01-01'), bis: tag('2026-12-31') };
		const b = beschriften(marken(f, 'tag'), f, 300);
		const sichtbar = b.filter((x) => x.sichtbar);
		expect(sichtbar.length).toBeGreaterThan(0);
		// Paarweise kein Überdecken.
		for (let i = 0; i < sichtbar.length; i++) {
			for (let k = i + 1; k < sichtbar.length; k++) {
				const a = sichtbar[i];
				const c = sichtbar[k];
				if (!a || !c) continue;
				const [l, r] = a.x < c.x ? [a, c] : [c, a];
				expect(l.x + 3 + textBreite(l.text)).toBeLessThanOrEqual(r.x + 3);
			}
		}
		// Hauptmarken (Monatsanfänge) gewinnen.
		expect(sichtbar.every((x) => x.haupt)).toBe(true);
	});
	it('nichts ragt rechts hinaus', () => {
		const f = { von: 0, bis: 10 };
		const b = beschriften([{ at: 9.9, text: 'ganz am Rand', haupt: true }], f, 200);
		expect(b[0]?.sichtbar).toBe(false);
	});
});
