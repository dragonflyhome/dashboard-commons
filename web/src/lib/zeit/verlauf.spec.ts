import { describe, expect, it } from 'vitest';
import type { VerlaufEreignis } from '$vertrag';
import {
	bedeutungAusArt,
	datumText,
	eintrag,
	ereignisseDesTeils,
	filtern,
	gruppeMitTag,
	gruppieren,
	kennungAusText,
	projektVerlauf,
	quellenZaehlen,
	routineZusammenfassen,
	zeitText,
	type Eintrag
} from './verlauf';

/** Freitag, 30.10.2026, mittags (Ortszeit). */
const JETZT = new Date(2026, 9, 30, 12, 0);

let nr = 0;
function um(
	j: number,
	m: number,
	t: number,
	h: number,
	min: number,
	mehr: Partial<VerlaufEreignis> = {}
): VerlaufEreignis {
	nr += 1;
	return {
		kennung: `test:${String(nr)}`,
		zeit: new Date(j, m - 1, t, h, min).toISOString(),
		genauigkeit: 'minute',
		bis: null,
		titel: `Ereignis ${String(nr)}`,
		text: null,
		quelle: 'hand',
		art: 'erledigt',
		verweis: null,
		...mehr
	};
}

function amTag(datum: string, mehr: Partial<VerlaufEreignis> = {}): VerlaufEreignis {
	nr += 1;
	return {
		kennung: `test:${String(nr)}`,
		zeit: `${datum}T00:00:00+01:00`,
		genauigkeit: 'tag',
		bis: null,
		titel: `Tag ${datum}`,
		text: null,
		quelle: 'hand',
		art: 'erledigt',
		verweis: null,
		...mehr
	};
}

function x(e: VerlaufEreignis): Eintrag {
	const r = eintrag(e);
	if (r === null) throw new Error('kein Eintrag');
	return r;
}

describe('art zu Bedeutung', () => {
	it('ordnet jede Art genau einer Bedeutung zu', () => {
		expect(bedeutungAusArt('erledigt')).toBe('ok');
		expect(bedeutungAusArt('begonnen')).toBe('laeuft');
		expect(bedeutungAusArt('geplant')).toBe('info');
		expect(bedeutungAusArt('problem')).toBe('problem');
		expect(bedeutungAusArt('routine')).toBe('neutral');
		expect(bedeutungAusArt('abgebrochen')).toBe('neutral');
	});
});

describe('Zeit in der Zeile', () => {
	it('Uhrzeit bei minute, sonst ganzer Tag, Monat oder Jahr, Spannen von–bis', () => {
		expect(zeitText(x(um(2026, 10, 30, 8, 5)))).toBe('08:05');
		expect(zeitText(x(amTag('2026-10-24')))).toBe('ganzer Tag');
		expect(zeitText(x(amTag('2026-10-24', { bis: '2026-10-26T00:00:00+01:00' })))).toBe(
			'24.10. – 26.10.'
		);
		const monat = amTag('2026-02-01', { genauigkeit: 'monat' });
		expect(zeitText(x(monat))).toBe('Februar 2026');
		expect(zeitText(x({ ...monat, bis: '2026-03-01T00:00:00+01:00' }))).toBe('Feb. – März 2026');
		expect(zeitText(x({ ...monat, bis: '2027-01-01T00:00:00+01:00' }))).toBe(
			'Feb. 2026 – Jan. 2027'
		);
		const jahr = amTag('2019-01-01', { genauigkeit: 'jahr' });
		expect(zeitText(x(jahr))).toBe('2019');
		expect(zeitText(x({ ...jahr, bis: '2021-01-01T00:00:00+01:00' }))).toBe('2019 – 2021');
		expect(datumText(x(amTag('2026-10-24')))).toBe('24.10.2026');
		expect(datumText(x(um(2026, 10, 24, 9, 0)))).toBe('24.10.2026, 09:00');
	});

	it('Spannen enden am Ende des letzten Abschnitts', () => {
		const e = x(amTag('2026-02-01', { genauigkeit: 'monat', bis: '2026-03-01T00:00:00+01:00' }));
		expect(e.endTag).toBe('2026-03-31');
		expect(x(amTag('2024-01-01', { genauigkeit: 'jahr' })).endTag).toBe('2024-12-31');
	});

	it('unlesbare Zeit fällt weg', () => {
		expect(eintrag({ ...amTag('2026-10-01'), zeit: 'später' })).toBeNull();
	});
});

describe('gruppieren (PC)', () => {
	const liste = [
		um(2026, 10, 30, 9, 0, { titel: 'heute früh' }),
		um(2026, 10, 30, 11, 0, { titel: 'heute später' }),
		um(2026, 10, 29, 18, 0, { titel: 'gestern' }),
		um(2026, 10, 27, 10, 0, { titel: 'Dienstag' }),
		um(2026, 10, 21, 10, 0, { titel: 'letzte Woche' }),
		um(2026, 10, 5, 10, 0, { titel: 'Anfang Oktober' }),
		um(2026, 3, 5, 10, 0, { titel: 'März' }),
		um(2025, 3, 5, 10, 0, { titel: 'voriges Jahr' }),
		amTag('2026-09-01', { genauigkeit: 'monat', titel: 'irgendwann im September' })
	];
	const g = gruppieren(liste, JETZT);

	it('neueste oben: Heute, Gestern, Wochentage, letzte Woche, Monate, Jahre', () => {
		expect(g.map((x) => x.titel)).toEqual([
			'Heute',
			'Gestern',
			'Dienstag, 27.10.',
			'Letzte Woche (19.10. bis 25.10.)',
			'Oktober 2026',
			'September 2026',
			'März 2026',
			'2025'
		]);
	});

	it('Heute und Gestern offen, alles andere zu, mit Anzahl', () => {
		expect(g.map((x) => x.offen)).toEqual([true, true, false, false, false, false, false, false]);
		expect(g[0]?.anzahl).toBe(2);
		expect(g[0]?.heute).toBe(true);
	});

	it('im Tag neueste zuerst', () => {
		const zeilen = g[0]?.tage[0]?.zeilen ?? [];
		expect(zeilen.map((z) => (z.art === 'eintrag' ? z.eintrag.e.titel : z.titel))).toEqual([
			'heute später',
			'heute früh'
		]);
	});

	it('nur Monat bekannt: eigene Liste in der Monatsgruppe', () => {
		const sept = g.find((x) => x.titel === 'September 2026');
		expect(sept?.tage).toEqual([]);
		expect(sept?.ohneTag).toHaveLength(1);
	});

	it('Heute steht auch ohne Einträge da', () => {
		const leer = gruppieren([], JETZT);
		expect(leer.map((x) => [x.titel, x.anzahl])).toEqual([['Heute', 0]]);
	});

	it('Kommendes steht ganz oben, aufsteigend', () => {
		const k = gruppieren(
			[um(2026, 11, 3, 9, 0), um(2026, 11, 1, 9, 0), um(2026, 10, 30, 9, 0)],
			JETZT
		);
		expect(k[0]?.titel).toBe('Demnächst');
		expect(k[0]?.tage.map((t) => t.tag)).toEqual(['2026-11-01', '2026-11-03']);
	});

	it('Jahr im laufenden Jahr, Monat in der Zukunft, Monat der letzten Woche: nie von nach bis', () => {
		const k = gruppieren(
			[
				amTag('2026-01-01', { genauigkeit: 'jahr', titel: 'Jahresziel 2026' }),
				amTag('2027-01-01', { genauigkeit: 'jahr', titel: 'Plan 2027' }),
				amTag('2026-11-01', { genauigkeit: 'monat', titel: 'im November' }),
				um(2026, 3, 5, 10, 0, { titel: 'März' })
			],
			JETZT
		);
		for (const x of k) expect(x.von <= x.bis, `${x.titel}: ${x.von} bis ${x.bis}`).toBe(true);
		expect(k.map((x) => x.titel)).toEqual(['Demnächst', 'Heute', 'März 2026', '2026']);
		// Demnächst einheitlich aufsteigend, auch ohne genauen Tag (Auflage aus 3a-1).
		expect(k[0]?.ohneTag.map((z) => (z.art === 'eintrag' ? z.eintrag.e.titel : ''))).toEqual([
			'im November',
			'Plan 2027'
		]);
		// Am 3.11. reicht der November in die laufende Woche: nur seine Einträge ohne Tag.
		const nov = gruppieren(
			[
				amTag('2026-11-01', { genauigkeit: 'monat', titel: 'im November' }),
				um(2026, 11, 1, 9, 0, { titel: 'Sonntag' })
			],
			new Date(2026, 10, 3, 12, 0)
		);
		for (const x of nov) expect(x.von <= x.bis, `${x.titel}: ${x.von} bis ${x.bis}`).toBe(true);
		expect(gruppeMitTag(nov, '2026-11-01')?.stufe).toBe('woche');
	});

	it('findet die Gruppe zu einem Tag', () => {
		expect(gruppeMitTag(g, '2026-10-22')?.stufe).toBe('woche');
		expect(gruppeMitTag(g, '2026-10-02')?.titel).toBe('Oktober 2026');
	});
});

describe('Routine zusammenfassen', () => {
	it('fasst Routine je Quelle ab zwei zusammen, an der Stelle des neuesten', () => {
		const tag = [
			um(2026, 10, 30, 11, 0, { quelle: 'update', art: 'routine' }),
			um(2026, 10, 30, 10, 0, { titel: 'wichtig', art: 'problem' }),
			um(2026, 10, 30, 9, 0, { quelle: 'update', art: 'routine' }),
			um(2026, 10, 30, 8, 0, { quelle: 'update', art: 'routine' }),
			um(2026, 10, 30, 7, 0, { quelle: 'update', art: 'routine' }),
			um(2026, 10, 30, 6, 0, { quelle: 'start', art: 'routine' })
		].map(x);
		const z = routineZusammenfassen(tag, 'tag:2026-10-30');
		expect(z.map((r) => (r.art === 'routine' ? r.titel : r.eintrag.e.titel))).toEqual([
			'4 Updates installiert',
			'wichtig',
			tag[5]?.e.titel
		]);
		const r = z[0];
		expect(r?.art === 'routine' ? r.eintraege.length : 0).toBe(4);
	});

	it('wichtige Updates bleiben einzeln', () => {
		const tag = [
			um(2026, 10, 30, 11, 0, { quelle: 'update', art: 'erledigt' }),
			um(2026, 10, 30, 10, 0, { quelle: 'update', art: 'erledigt' })
		].map(x);
		expect(routineZusammenfassen(tag, 't').every((z) => z.art === 'eintrag')).toBe(true);
	});
});

describe('Filter nach Quelle und Zeitraum', () => {
	const liste = [
		um(2026, 10, 30, 9, 0, { quelle: 'update' }),
		um(2026, 10, 29, 9, 0, { quelle: 'update' }),
		um(2026, 10, 20, 9, 0, { quelle: 'start' }),
		amTag('2026-10-01', { genauigkeit: 'monat' })
	];
	it('zählt je Quelle, häufigste zuerst', () => {
		expect(quellenZaehlen(liste)).toEqual([
			{ quelle: 'update', name: 'Updates', anzahl: 2 },
			{ quelle: 'start', name: 'Starts', anzahl: 1 },
			{ quelle: 'hand', name: 'von Hand', anzahl: 1 }
		]);
	});
	it('blendet Quellen aus und begrenzt auf einen Zeitraum (Spannen, die ihn berühren, bleiben)', () => {
		expect(filtern(liste, new Set(['update']))).toHaveLength(2);
		expect(filtern(liste, new Set(), { von: '2026-10-19', bis: '2026-10-29' })).toHaveLength(3);
		expect(filtern(liste, new Set(), { von: '2026-10-29', bis: '2026-10-30' })).toHaveLength(3);
	});
});

describe('Projektseiten', () => {
	it('Kommendes über heute aufsteigend, Vergangenes darunter absteigend je Jahr', () => {
		const p = projektVerlauf(
			[
				amTag('2027-07-01', { genauigkeit: 'monat', titel: 'Steuer', art: 'geplant' }),
				amTag('2026-11-06', { titel: 'Termin', art: 'geplant' }),
				amTag('2026-10-01', { genauigkeit: 'monat', titel: 'läuft diesen Monat', art: 'geplant' }),
				amTag('2026-10-24', { titel: 'gestern erledigt' }),
				amTag('2025-03-02', { titel: 'alt' }),
				amTag('2019-01-01', {
					genauigkeit: 'jahr',
					bis: '2021-01-01T00:00:00+01:00',
					titel: 'Lehrgang'
				})
			],
			JETZT
		);
		expect(p.kommend.map((x) => x.e.titel)).toEqual(['läuft diesen Monat', 'Termin', 'Steuer']);
		expect(p.vergangen.map((a) => [a.titel, a.eintraege.map((x) => x.e.titel)])).toEqual([
			['2026', ['gestern erledigt']],
			['2025', ['alt']],
			['2019', ['Lehrgang']]
		]);
	});

	it('ordnet die Ereignisse eines Teils über die Kennung zu, wie der Server sie bildet', () => {
		expect(kennungAusText('Größe prüfen: Über 100 %!')).toBe('groesse-pruefen-ueber-100');
		expect(kennungAusText('2019 – 2021')).toBe('2019-2021');
		const alle = [
			{ ...amTag('2026-10-01'), kennung: 'hand:pilot' },
			{ ...amTag('2026-10-02'), kennung: 'hand:2026-10:frist-a' },
			{ ...amTag('2026-10-03'), kennung: 'hand:2026-10:frist-a-2' },
			{ ...amTag('2026-10-04'), kennung: 'hand:anderer-teil' }
		];
		const teil = ereignisseDesTeils(alle, [
			{ kennung: 'pilot', datum: '2026-10-01', titel: 'Pilot' },
			{ datum: '2026-10', titel: 'Frist A' }
		]);
		expect(teil.map((e) => e.kennung)).toEqual([
			'hand:pilot',
			'hand:2026-10:frist-a',
			'hand:2026-10:frist-a-2'
		]);
	});

	it('Teile mit Kennung schritt und schritt-2 vermischen sich nicht (Auflage aus 3a-1)', () => {
		const ev = (kennung: string): VerlaufEreignis => um(2026, 10, 1, 9, 0, { kennung });
		const liste = [ev('hand:schritt'), ev('hand:schritt-2'), ev('hand:schritt-3')];
		const a = { kennung: 'schritt', datum: '2026-10-01', titel: 'A' };
		const b = { kennung: 'schritt-2', datum: '2026-10-02', titel: 'B' };
		// schritt-3 ist die Doppel-Endung von eindeutig_machen (zweites „schritt“ in A).
		expect(ereignisseDesTeils(liste, [a], [a, b]).map((e) => e.kennung)).toEqual([
			'hand:schritt',
			'hand:schritt-3'
		]);
		expect(ereignisseDesTeils(liste, [b], [a, b]).map((e) => e.kennung)).toEqual([
			'hand:schritt-2'
		]);
	});
});
