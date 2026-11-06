import { describe, expect, it } from 'vitest';
import type { VerlaufEreignis } from '$vertrag';
import {
	ausschnittText,
	dichteSpannen,
	fensterMitCursor,
	fensterZoomen,
	laenge,
	naechsteSpanne,
	tageZaehlen
} from './dichte';

function e(
	datum: string,
	art: VerlaufEreignis['art'],
	genauigkeit: VerlaufEreignis['genauigkeit'] = 'tag'
): VerlaufEreignis {
	return {
		kennung: `k:${datum}:${art}`,
		zeit: `${datum}T00:00:00+01:00`,
		genauigkeit,
		bis: null,
		titel: 't',
		text: null,
		quelle: 'update',
		art,
		verweis: null
	};
}

describe('Dichtestreifen', () => {
	it('zählt je Tag der letzten 91 Tage (wie der Server liefert), Routine extra', () => {
		const t = tageZaehlen(
			[
				e('2026-10-30', 'routine'),
				e('2026-10-30', 'erledigt'),
				e('2026-08-01', 'erledigt'),
				e('2026-07-31', 'erledigt'),
				e('2026-10-01', 'erledigt', 'monat')
			],
			'2026-10-30'
		);
		expect(t).toHaveLength(91);
		// Der älteste der 91 Tage (heute minus 90) ist dabei, der Tag davor nicht.
		expect(t[0]).toEqual({ tag: '2026-08-01', anzahl: 1, wichtig: 1 });
		expect(t[90]).toEqual({ tag: '2026-10-30', anzahl: 2, wichtig: 1 });
		expect(t.reduce((n, b) => n + b.anzahl, 0)).toBe(3);
		expect(tageZaehlen([], '2026-10-30', 90)).toHaveLength(90);
	});

	it('Ausschnitt folgt dem Cursor und bleibt in den Grenzen', () => {
		const g = { von: '2026-08-01', bis: '2026-10-30' };
		const f = fensterZoomen(14, '2026-10-30', g);
		expect(f).toEqual({ von: '2026-10-17', bis: '2026-10-30' });
		expect(laenge(f)).toBe(14);
		expect(fensterMitCursor(f, '2026-10-10', g)).toEqual({ von: '2026-10-10', bis: '2026-10-23' });
		expect(fensterMitCursor(f, '2026-07-01', g).von).toBe('2026-08-01');
		expect(fensterZoomen(91, '2026-09-01', g)).toEqual(g);
	});

	it('Zoomstufen: alle Tage, 30, 14', () => {
		expect(dichteSpannen(91)).toEqual([91, 30, 14]);
		expect(dichteSpannen(20)).toEqual([20, 14]);
		expect(naechsteSpanne(91, 1)).toBe(30);
		expect(naechsteSpanne(14, 1)).toBe(14);
		expect(naechsteSpanne(30, -1)).toBe(91);
		expect(naechsteSpanne(30, -1, 60)).toBe(60);
	});

	it('Ausschnitt als Satz: bis heute „die letzten“, sonst mit Datum', () => {
		const d = (t: string): string => t.split('-').reverse().join('.');
		expect(ausschnittText({ von: '2026-08-01', bis: '2026-10-30' }, '2026-10-30', d)).toBe(
			'die letzten 91 Tage bis heute'
		);
		expect(ausschnittText({ von: '2026-10-01', bis: '2026-10-14' }, '2026-10-30', d)).toBe(
			'14 Tage vom 01.10.2026 bis 14.10.2026'
		);
	});
});
