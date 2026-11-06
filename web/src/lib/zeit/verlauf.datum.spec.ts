/**
 * Datumsfälle des Verlaufs (Auflagen aus 3a-1): heute ist Montag, der
 * Monatserste, der 1.1. und die Zeitumstellung. Feste Zeitzone mit
 * Sommerzeit, damit die Umstellungstage stimmen.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { VerlaufEreignis } from '$vertrag';
import { tageZaehlen } from './dichte.ts';
import { tagPlus } from './tage.ts';
import { gruppeMitTag, gruppieren, type Gruppe } from './verlauf.ts';

const vorherTz = process.env.TZ;
beforeAll(() => {
	process.env.TZ = 'Europe/Berlin';
});
afterAll(() => {
	if (vorherTz === undefined) delete process.env.TZ;
	else process.env.TZ = vorherTz;
});

let nr = 0;
/** Ein Ereignis mit Uhrzeit; `zeit` wie der Server sie schreibt (mit Versatz). */
function um(zeit: string, mehr: Partial<VerlaufEreignis> = {}): VerlaufEreignis {
	nr += 1;
	return {
		kennung: `datum:${String(nr)}`,
		zeit,
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

function nurDatum(
	datum: string,
	genauigkeit: VerlaufEreignis['genauigkeit'],
	titel: string
): VerlaufEreignis {
	return um(`${datum}T00:00:00+01:00`, { genauigkeit, titel });
}

const titel = (g: readonly Gruppe[]): string[] => g.map((x) => x.titel);
const mitHeute = (g: readonly Gruppe[]): string[] => g.filter((x) => x.heute).map((x) => x.titel);
const tageDer = (g: Gruppe | undefined): string[] => g?.tage.map((t) => t.tag) ?? [];

describe('heute ist Montag', () => {
	/** Montag, 26.10.2026. */
	const jetzt = new Date(2026, 9, 26, 12, 0);

	it('Gestern (Sonntag) steht für sich; die letzte Woche endet am Samstag und sagt das', () => {
		const g = gruppieren(
			[
				um('2026-10-25T10:00:00+01:00', { titel: 'Sonntag' }),
				um('2026-10-24T10:00:00+02:00', { titel: 'Samstag' }),
				um('2026-10-19T10:00:00+02:00', { titel: 'Montag davor' }),
				um('2026-10-18T10:00:00+02:00', { titel: 'Sonntag davor' })
			],
			jetzt
		);
		expect(titel(g)).toEqual([
			'Heute',
			'Gestern',
			'Letzte Woche (19.10. bis 24.10.)',
			'Oktober 2026'
		]);
		const woche = g.find((x) => x.stufe === 'woche');
		expect(woche?.bis).toBe('2026-10-24');
		expect(tageDer(woche)).toEqual(['2026-10-24', '2026-10-19']);
		expect(gruppeMitTag(g, '2026-10-25')?.titel).toBe('Gestern');
	});

	it('steht gestern nichts, gehört der Sonntag zu keiner Gruppe (Sprung: Leer-Satz)', () => {
		const g = gruppieren([um('2026-10-24T10:00:00+02:00')], jetzt);
		expect(gruppeMitTag(g, '2026-10-25')).toBeNull();
		expect(gruppeMitTag(g, '2026-10-24')?.stufe).toBe('woche');
	});
});

describe('Monatserster und 1.1.: nur Heute trägt heute', () => {
	it('am 1.11. beginnt auch der laufende Monat heute', () => {
		const g = gruppieren(
			[
				nurDatum('2026-11-01', 'monat', 'im November'),
				um('2026-10-31T20:00:00+01:00', { titel: 'Samstag' }),
				um('2026-10-05T10:00:00+02:00', { titel: 'Anfang Oktober' })
			],
			new Date(2026, 10, 1, 9, 0)
		);
		expect(mitHeute(g)).toEqual(['Heute']);
		expect(titel(g)).toEqual(['Heute', 'November 2026', 'Gestern', 'Oktober 2026']);
	});

	it('am 1.1. auch Januar und das laufende Jahr; das Jahr steht unter allen Monaten', () => {
		const g = gruppieren(
			[
				nurDatum('2027-01-01', 'jahr', 'Jahresziel 2027'),
				nurDatum('2027-01-01', 'monat', 'im Januar'),
				um('2026-12-31T18:00:00+01:00', { titel: 'Silvester' }),
				um('2026-12-15T10:00:00+01:00', { titel: 'Mitte Dezember' }),
				um('2026-02-10T10:00:00+01:00', { titel: 'Februar' }),
				um('2025-06-01T10:00:00+02:00', { titel: 'vor zwei Jahren' })
			],
			new Date(2027, 0, 1, 9, 0)
		);
		expect(mitHeute(g)).toEqual(['Heute']);
		expect(titel(g)).toEqual([
			'Heute',
			'Januar 2027',
			'Gestern',
			'Dezember 2026',
			'Februar 2026',
			'2027',
			'2025'
		]);
	});
});

describe('Zeitumstellung', () => {
	it('Ende der Sommerzeit (25.10.2026): beide 2:30 gehören zu heute, 23:30 davor zu gestern', () => {
		const g = gruppieren(
			[
				um('2026-10-25T00:30:00+02:00', { titel: 'nachts Sommerzeit' }),
				um('2026-10-25T02:30:00+02:00', { titel: '2:30 Sommerzeit' }),
				um('2026-10-25T02:30:00+01:00', { titel: '2:30 Winterzeit' }),
				um('2026-10-24T23:30:00+02:00', { titel: 'spät gestern' })
			],
			new Date(2026, 9, 25, 12, 0)
		);
		expect(titel(g)).toEqual(['Heute', 'Gestern']);
		expect(g[0]?.anzahl).toBe(3);
		expect(g[1]?.anzahl).toBe(1);
	});

	it('Beginn der Sommerzeit (29.3.2026): kein Tag fehlt und keiner doppelt', () => {
		const g = gruppieren(
			[
				um('2026-03-29T01:30:00+01:00', { titel: 'vor der Umstellung' }),
				um('2026-03-29T03:30:00+02:00', { titel: 'nach der Umstellung' }),
				um('2026-03-28T23:59:00+01:00', { titel: 'kurz vor Mitternacht' })
			],
			new Date(2026, 2, 29, 12, 0)
		);
		expect(titel(g)).toEqual(['Heute', 'Gestern']);
		expect(g[0]?.anzahl).toBe(2);
		// Der Dichtestreifen über beide Umstellungen: 91 verschiedene, lückenlose Tage.
		for (const heute of ['2026-04-15', '2026-11-20']) {
			const t = tageZaehlen([], heute).map((b) => b.tag);
			expect(new Set(t).size).toBe(91);
			expect(t.every((tag, i) => i === 0 || tagPlus(t[i - 1] ?? '', 1) === tag)).toBe(true);
			expect(t.at(-1)).toBe(heute);
		}
	});
});
