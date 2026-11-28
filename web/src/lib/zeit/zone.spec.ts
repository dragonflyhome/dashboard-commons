import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { PlanEtappe } from '$vertrag';
import { fmtDate, fmtDateTime, fmtTagUhrzeit, gitStandHinweis } from '../format.ts';
import { zeitText as schrittZeit } from '../katalog/widgets/worte.ts';
import { projektPlan } from '../../routes/katalog/beispiele.ts';
import { etappenWort } from '../projektteile.ts';
import { PUFFER_MS } from '../tageswechsel.ts';
import { kurzZeit } from '../unterdashboard.ts';
import { fmtRelativ, fmtZeitpunkt } from '../vorgaenge.ts';
import { tagzahlAus } from './achse.ts';
import { beiTageswechselInZone, msBisMitternachtInZone } from './zonentag.ts';
import { tagZahl } from './tage.ts';
import { eintrag, zeitText } from './verlauf.ts';
import { anzeigeZone, tagInZone, teileInZone, uhrInZone, zoneSetzen } from './zone.svelte.ts';

// Gerät in Berlin (MEZ, +1), Blickort in Auckland (NZDT, +13): zwölf Stunden auseinander.
const vorherTz = process.env.TZ;
beforeAll(() => {
	process.env.TZ = 'Europe/Berlin';
});
afterAll(() => {
	if (vorherTz === undefined) delete process.env.TZ;
	else process.env.TZ = vorherTz;
});
afterEach(() => {
	zoneSetzen(null);
	vi.useRealTimers();
});

const AUCKLAND = 'Pacific/Auckland';
/** Berlin Fr 27.11.2026 12:30, Auckland schon Sa 28.11.2026 00:30. */
const UEBER_MITTERNACHT = '2026-11-27T11:30:00Z';
const JETZT = new Date(UEBER_MITTERNACHT);

/** Text in der Gerätezone und in Auckland. */
function beide<T>(f: () => T): [T, T] {
	zoneSetzen(null);
	const geraet = f();
	zoneSetzen(AUCKLAND);
	const fern = f();
	zoneSetzen(null);
	return [geraet, fern];
}

describe('Anzeigezone: Helfer', () => {
	it('ohne Zone gilt die Gerätezone', () => {
		expect(anzeigeZone()).toBeNull();
		expect(tagInZone(JETZT)).toBe('2026-11-27');
		expect(uhrInZone(JETZT)).toBe('12:30');
	});

	it('Teile über Mitternacht: Tag, Uhrzeit und Wochentag in der Zone', () => {
		zoneSetzen(AUCKLAND);
		expect(teileInZone(JETZT)).toEqual({
			jahr: 2026,
			monat: 11,
			tag: 28,
			stunde: 0,
			minute: 30,
			sekunde: 0,
			wochentag: 6
		});
		expect(tagInZone(JETZT)).toBe('2026-11-28');
		expect(uhrInZone(JETZT)).toBe('00:30');
	});

	it('Jahreswechsel: Auckland ist schon im neuen Jahr', () => {
		expect(beide(() => tagInZone(new Date('2026-12-31T11:30:00Z')))).toEqual([
			'2026-12-31',
			'2027-01-01'
		]);
	});

	it('eine Zone, die der Browser nicht kennt, zeigt Gerätezeit statt zu werfen', () => {
		zoneSetzen('Nirgendwo/Gibtsnicht');
		expect(tagInZone(JETZT)).toBe('2026-11-27');
		expect(fmtDateTime(UEBER_MITTERNACHT)).toBe('27.11.2026, 12:30');
	});
});

describe('Anzeigezone: Formatierer über Mitternacht', () => {
	it('fmtDateTime', () => {
		expect(beide(() => fmtDateTime(UEBER_MITTERNACHT))).toEqual([
			'27.11.2026, 12:30',
			'28.11.2026, 00:30'
		]);
	});

	it('fmtTagUhrzeit: Wochentag und Jahr nach der Zone', () => {
		expect(beide(() => fmtTagUhrzeit(UEBER_MITTERNACHT, JETZT))).toEqual([
			'Fr 27.11., 12:30 Uhr',
			'Sa 28.11., 00:30 Uhr'
		]);
		// Silvester: in Auckland schon das neue Jahr, „jetzt“ ebenso, also ohne Jahr.
		const silvester = new Date('2026-12-31T11:30:00Z');
		expect(beide(() => fmtTagUhrzeit('2026-12-30T11:30:00Z', silvester))).toEqual([
			'Mi 30.12., 12:30 Uhr',
			'Do 31.12.2026, 00:30 Uhr'
		]);
	});

	it('gitStandHinweis: gleicher Tag nur mit Uhrzeit, sonst mit Datum', () => {
		expect(beide(() => gitStandHinweis('2026-11-27T10:50:00Z', JETZT))).toEqual([
			'Git-Stand von 11:50',
			'Git-Stand von 27.11. 23:50'
		]);
	});

	it('Vorgänge: heute, gestern und älter nach dem Tag der Zone', () => {
		expect(beide(() => fmtZeitpunkt('2026-11-27T10:50:00Z', JETZT))).toEqual([
			'11:50 Uhr',
			'gestern 23:50 Uhr'
		]);
		expect(beide(() => fmtZeitpunkt('2026-11-25T11:30:00Z', JETZT))).toEqual([
			'25.11.2026, 12:30 Uhr',
			'26.11.2026, 00:30 Uhr'
		]);
		// 30 Stunden zurück: in Berlin gestern, in Auckland vorgestern.
		expect(beide(() => fmtRelativ('2026-11-26T05:30:00Z', JETZT))).toEqual([
			'gestern',
			'26.11.2026'
		]);
	});

	it('Verlauf: Tag und Uhrzeit eines Ereignisses auf die Minute', () => {
		const e = eintrag({
			kennung: 'x',
			zeit: UEBER_MITTERNACHT,
			genauigkeit: 'minute',
			bis: null,
			titel: 'Etwas',
			text: null,
			quelle: 'hand',
			art: 'erledigt',
			verweis: null
		});
		if (e === null) throw new Error('Eintrag fehlt.');
		expect(beide(() => eintrag(e.e)?.tag)).toEqual(['2026-11-27', '2026-11-28']);
		expect(beide(() => zeitText(e))).toEqual(['12:30', '00:30']);
	});

	it('tagzahlAus: Tag und Bruchteil in der Zone', () => {
		zoneSetzen(AUCKLAND);
		expect(tagzahlAus(JETZT)).toBeCloseTo(tagZahl('2026-11-28') + 0.5 / 24, 9);
	});

	it('kurzZeit: RFC 3339 in die Zone, Unlesbares bleibt', () => {
		expect(beide(() => kurzZeit('2026-11-27T12:30:00+01:00'))).toEqual([
			'27.11. 12:30',
			'28.11. 00:30'
		]);
		expect(kurzZeit('gestern')).toBe('gestern');
	});

	it('zeitText eines Schritts', () => {
		expect(
			beide(() => schrittZeit({ stand: 'laeuft', seit: '2026-11-27T12:30:00+01:00' }))
		).toEqual(['seit 27.11., 12:30 Uhr', 'seit 28.11., 00:30 Uhr']);
	});

	it('Etappen: ein Zeitpunkt nimmt den Tag der Zone, ein reiner Tag bleibt', () => {
		const etappe = (e: Partial<PlanEtappe>): PlanEtappe => {
			const erste = projektPlan().etappen[0];
			if (erste === undefined) throw new Error('Beispielplan ohne Etappe.');
			return { ...erste, ab: null, ...e };
		};
		const laeuft = etappe({ stand: 'laeuft', begonnen: '2026-11-27T12:30:00+01:00' });
		expect(beide(() => etappenWort(laeuft))).toEqual(['läuft seit 27.11.', 'läuft seit 28.11.']);
		const tag = etappe({ stand: 'laeuft', begonnen: '2026-11-27' });
		expect(beide(() => etappenWort(tag))).toEqual(['läuft seit 27.11.', 'läuft seit 27.11.']);
	});

	it('ganztägige Daten rechnet die Zone nie um', () => {
		expect(beide(() => fmtDate('2026-11-27'))).toEqual(['27.11.2026', '27.11.2026']);
	});
});

describe('Tageswechsel am Blickort', () => {
	it('msBisMitternachtInZone: bis 0:00 in der Zone, nie 0', () => {
		zoneSetzen(AUCKLAND);
		// Auckland 00:30 → 23,5 Stunden bis zur nächsten Mitternacht dort.
		expect(msBisMitternachtInZone(JETZT)).toBe(23.5 * 3_600_000);
		expect(msBisMitternachtInZone(new Date('2026-11-27T11:00:00Z'))).toBe(24 * 3_600_000);
	});

	it('Tage mit 23 und 25 Stunden: genau bis 0:00, in der Gerätezone und in einer Zone', () => {
		const stunden = (h: number): number => h * 3_600_000 - 1000;
		// Berlin So 28.3.2027 00:00:01 (Sommerzeit ab 2 Uhr) und So 31.10.2027 00:00:01 (Winterzeit ab 3 Uhr).
		const fruehling = new Date('2027-03-27T23:00:01Z');
		const herbst = new Date('2027-10-30T22:00:01Z');
		expect(msBisMitternachtInZone(fruehling)).toBe(stunden(23));
		expect(msBisMitternachtInZone(herbst)).toBe(stunden(25));
		zoneSetzen('Europe/Berlin');
		expect(msBisMitternachtInZone(fruehling)).toBe(stunden(23));
		expect(msBisMitternachtInZone(herbst)).toBe(stunden(25));
		// Auckland So 26.9.2027 00:00:01 NZST (Sommerzeit ab 2 Uhr) und So 4.4.2027 00:00:01 NZDT (Winterzeit ab 3 Uhr).
		zoneSetzen(AUCKLAND);
		expect(msBisMitternachtInZone(new Date('2027-09-25T12:00:01Z'))).toBe(stunden(23));
		expect(msBisMitternachtInZone(new Date('2027-04-03T11:00:01Z'))).toBe(stunden(25));
	});

	it('ruft zur Mitternacht in Auckland, nicht zur Berliner', () => {
		vi.useFakeTimers();
		zoneSetzen(AUCKLAND);
		// Auckland Fr 27.11. 23:59:00 = Berlin 11:59.
		vi.setSystemTime(new Date('2026-11-27T10:59:00Z'));
		const rufen = vi.fn();
		const ab = beiTageswechselInZone(rufen, () => new Date(), null);
		vi.advanceTimersByTime(60_000 + PUFFER_MS - 1);
		expect(rufen).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(rufen).toHaveBeenCalledTimes(1);
		ab();
	});
});
