/**
 * Etappe B8: die Auswertung der vier Teile der Projektseite
 * (`#lib/projektteile.ts`) und die Verdichtung der Zeitleiste
 * (`routineZusammenfassen`). Jede Regel mit Gegenprobe. Alle Daten erfunden.
 */
import { describe, expect, it, vi } from 'vitest';
import type { AmpelGrund, Backlog, EintragsArt, PlanEtappe, VerlaufEreignis } from '$vertrag';
import { eintrag as anstehendEintrag, json } from '#lib/anstehend.beispiel.ts';
import {
	ampelZiel,
	etappenNamen,
	etappenWort,
	gewachsenSatz,
	inListe,
	ladeBacklog,
	ladePlan,
	ladeProjektVerlauf,
	listenGruppen,
	planArbeitWort,
	standSatz,
	standZahlen,
	ungeleseneMeldungen,
	zaehlung,
	zeilenAngaben,
	zeitleisteEreignisse,
	zeitstreifen
} from '#lib/projektteile.ts';
import { eintrag, routineZusammenfassen } from '#lib/zeit/verlauf.ts';
import { backlog, projektPlan, tagIso } from '../routes/katalog/beispiele.ts';

/** Das Element oder ein Fehler (die Beispiele haben es immer). */
function da<T>(x: T | undefined): T {
	if (x === undefined) throw new Error('Beispiel fehlt');
	return x;
}

const etappe = (e: Partial<PlanEtappe>): PlanEtappe => ({
	...da(projektPlan().etappen[0]),
	...e
});

describe('Stand', () => {
	it('vier Zahlen, jede springt zu ihrer Gruppe oder zur Zeitleiste', () => {
		const z = standZahlen('beispiel', { laeuft: 2, wartet_auf_dich: 1, als_naechstes: 5 }, 6);
		expect(z.map((x) => `${String(x.zahl)} ${x.wort} → ${x.ziel}`)).toEqual([
			'2 laufen → liste-laeuft-beispiel',
			'1 wartet auf dich → liste-dich-beispiel',
			'5 als Nächstes → liste-naechstes-beispiel',
			'6 fertig in 7 Tagen → zeitleiste-beispiel'
		]);
		// Gegenprobe: Einzahl und Mehrzahl kippen.
		const g = standZahlen('beispiel', { laeuft: 1, wartet_auf_dich: 3, als_naechstes: 0 }, 0);
		expect(g.map((x) => x.wort).slice(0, 2)).toEqual(['läuft', 'warten auf dich']);
	});

	it('ein Satz über Laufendes und Fertiges; was wartet, sagt die Ampel', () => {
		expect(standSatz({ laeuft: 4, wartet_auf_dich: 2, als_naechstes: 0 }, 6)).toBe(
			'4 Arbeiten laufen. In den letzten 7 Tagen sind 6 fertig geworden.'
		);
		expect(standSatz({ laeuft: 0, wartet_auf_dich: 0, als_naechstes: 0 }, 1)).toBe(
			'Gerade läuft keine Arbeit. In den letzten 7 Tagen ist eine fertig geworden.'
		);
		expect(standSatz({ laeuft: 1, wartet_auf_dich: 9, als_naechstes: 0 }, 0)).not.toContain('9');
	});

	it('der Grund der Ampel führt zur Gruppe oder Zeitleiste; ohne Ziel kein Link', () => {
		const g = (grund_art: AmpelGrund, bezug_art: EintragsArt | null = null) => ({
			grund_art,
			bezug_art
		});
		expect(ampelZiel(g('wartet-lange', 'frage'), 'x')).toBe('liste-dich-x');
		expect(ampelZiel(g('ohne-bewegung'), 'x')).toBe('liste-laeuft-x');
		expect(ampelZiel(g('fehler-offen', 'meldung'), 'x')).toBe('zeitleiste-x');
		expect(ampelZiel(g('nichts-wartet'), 'x')).toBeNull();
		// Verstrichene Frist: dorthin, wo der Eintrag sicher steht, nicht in die Zeitleiste (nur 14 Tage).
		expect(ampelZiel(g('frist-vorbei', 'aufgabe'), 'x')).toBe('liste-dich-x');
		expect(ampelZiel(g('frist-vorbei', 'frage'), 'x')).toBe('liste-dich-x');
		expect(ampelZiel(g('frist-vorbei', 'arbeit'), 'x')).toBe('aufgabenliste-x');
		// Gegenprobe: eine Frist aus fristen.toml hat kein sicheres Ziel, der Satz steht ohne Link.
		expect(ampelZiel(g('frist-vorbei'), 'x')).toBeNull();
	});
});

describe('Aufgabenliste', () => {
	it('vier Gruppen untereinander; ohne Kasten Fragen und Aufgaben als Zeilen', () => {
		const g = listenGruppen(backlog(), false);
		expect(g.map((x) => `${x.titel} ${String(x.karten.length)}`)).toEqual([
			'Wartet auf dich 2',
			'Läuft 1',
			'Als Nächstes 1',
			'Wartet auf andere 2'
		]);
	});

	it('mit Kasten: in „Wartet auf dich“ nur Arbeiten, bei anderen keine Aufgaben (stehen „Beim Chat“)', () => {
		const b: Backlog = backlog();
		const fuerDich = da(b.spalten[0]);
		const wartet = da(b.spalten[2]);
		const beimChat = { ...da(wartet.karten[0]), art: 'aufgabe' as const, id: 'a-20261104-0009' };
		wartet.karten.push(beimChat);
		const fuerMich = { ...beimChat, art: 'arbeit' as const, id: 'ar-20261104-0001' };
		fuerDich.karten.push(fuerMich);
		const mit = listenGruppen(b, true);
		expect(mit[0]?.karten.map((k) => k.id)).toEqual(['ar-20261104-0001']);
		expect(mit[3]?.karten.map((k) => k.art)).toEqual(['arbeit', 'arbeit']);
		// Gegenprobe: ohne Kasten steht die Aufgabe beim Chat bei den anderen.
		expect(listenGruppen(b, false)[3]?.karten.map((k) => k.id)).toContain('a-20261104-0009');
	});

	it('die Gruppen folgen den Spalten des Servers: eine Arbeit, die über eine Frage auf dich wartet, steht bei „Wartet“', () => {
		const b: Backlog = backlog();
		// Wie im Server: wartet_auf inhaber mit Verweis auf eine offene Frage → Spalte „Wartet“,
		// in `wartend` aber in der Gruppe des Inhabers.
		const ueberFrage = {
			...da(da(b.spalten[2]).karten[0]),
			id: 'ar-20261104-0002',
			wartet_auf: 'inhaber',
			hinweis: 'wartet auf die Frage „farbe“'
		};
		da(b.spalten[2]).karten.push(ueberFrage);
		da(b.wartend[0]).karten.push(ueberFrage);
		for (const mitKasten of [true, false]) {
			const [dich, , , andere] = listenGruppen(b, mitKasten);
			expect(dich?.karten.map((k) => k.id)).not.toContain('ar-20261104-0002');
			expect(andere?.karten.map((k) => k.id)).toContain('ar-20261104-0002');
		}
	});

	it('Angaben einer Zeile: Etappe mit Titel aus dem Plan, wer dran ist, sonst die Kennung', () => {
		const namen = etappenNamen(projektPlan());
		const laeuft = backlog().spalten[1]?.karten[0];
		if (!laeuft) throw new Error('Beispiel fehlt');
		// Mit Schritten sagt die Kurzzeile, wer dran ist; hier nur die Etappe.
		expect(zeilenAngaben(laeuft, namen)).toEqual(['Etappe: Einladung']);
		const ohneSchritte = { ...laeuft, schritte: [] };
		expect(zeilenAngaben(ohneSchritte, new Map())).toEqual([
			'Etappe: einladung',
			'dran: Chat beispiel'
		]);
		const wartet = backlog().wartend[1]?.karten[0];
		if (!wartet) throw new Error('Beispiel fehlt');
		expect(zeilenAngaben(wartet, namen)).toEqual(['wartet auf Chat nachbar']);
		const aufgabe = backlog().spalten[0]?.karten[1];
		if (!aufgabe) throw new Error('Beispiel fehlt');
		expect(zeilenAngaben(aufgabe, namen)).toEqual(['Aufgabe', 'wartet auf dich']);
	});
});

describe('Plan', () => {
	it('Stand-Wort je Etappe, mit „ungefähr“, ohne Datum ohne „seit“', () => {
		expect(etappenWort(etappe({ stand: 'geplant' }))).toBe('geplant');
		expect(etappenWort(etappe({ stand: 'laeuft', begonnen: '2026-11-05T09:00:00+01:00' }))).toBe(
			'läuft seit 05.11.'
		);
		expect(etappenWort(etappe({ stand: 'laeuft', begonnen: null }))).toBe('läuft');
		expect(etappenWort(etappe({ fertig: '2026-11-13T09:00:00+01:00', ungefaehr: false }))).toBe(
			'fertig am 13.11.'
		);
		expect(etappenWort(etappe({ fertig: '2026-11-13T09:00:00+01:00', ungefaehr: true }))).toBe(
			'fertig am 13.11. (ungefähr)'
		);
		expect(etappenWort(etappe({ stand: 'vor-dem-tracker' }))).toBe('vor dem Tracker abgeschlossen');
	});

	it('ein echter Termin (ab) steht im Wort, nicht nur als Raute im Streifen', () => {
		expect(etappenWort(etappe({ stand: 'geplant', ab: '2027-03' }))).toBe('geplant · ab 03/2027');
		expect(
			etappenWort(etappe({ stand: 'laeuft', begonnen: '2026-11-05T09:00:00+01:00', ab: '2026-12' }))
		).toBe('läuft seit 05.11. · ab 12/2026');
		// Gegenprobe: ohne Termin kein Zusatz.
		expect(etappenWort(etappe({ stand: 'geplant', ab: null }))).toBe('geplant');
	});

	it('Zählung, Wachstum und Arbeiten in Worten', () => {
		expect(zaehlung(etappe({ arbeiten_fertig: 3, arbeiten_gesamt: 4 }))).toBe(
			'3 von 4 Arbeiten fertig'
		);
		expect(zaehlung(etappe({ arbeiten_fertig: 0, arbeiten_gesamt: 1 }))).toBe(
			'0 von 1 Arbeit fertig'
		);
		expect(zaehlung(etappe({ arbeiten_gesamt: 0 }))).toBeNull();
		expect(
			gewachsenSatz(etappe({ gewachsen_seit: '2026-11-05T09:00:00+01:00', dazugekommen: 1 }))
		).toBe('seit 05.11. 1 Arbeit dazugekommen');
		expect(gewachsenSatz(etappe({ gewachsen_seit: null, dazugekommen: 0 }))).toBeNull();
		const [fertig, liste] = projektPlan().etappen[0]?.arbeiten ?? [];
		const [laeuft, fremd] = projektPlan().etappen[1]?.arbeiten ?? [];
		if (!fertig || !liste || !laeuft || !fremd) throw new Error('Beispiel fehlt');
		expect(planArbeitWort(fertig)).toMatch(/^fertig am \d\d\.\d\d\. · im Archiv$/);
		expect(planArbeitWort(liste)).toMatch(/\(ungefähr\)$/);
		expect(planArbeitWort(fremd)).toBe('als Nächstes · aus druckerei');
		// Nur offene Arbeiten dieses Projekts haben eine Zeile in der Aufgabenliste.
		expect([fertig, liste, laeuft, fremd].map(inListe)).toEqual([false, false, true, false]);
	});

	it('Zeitstreifen nur aus Ist-Daten: Balken, offener Balken bis heute, Raute, geplant ohne', () => {
		const heute = new Date(2026, 10, 13, 12);
		const s = zeitstreifen(projektPlan(heute), tagIso(0, heute));
		if (s === null) throw new Error('kein Streifen');
		const [fertig, laeuft, fest] = s.zeilen;
		expect(fertig?.balken?.offen).toBe(false);
		expect(laeuft?.balken?.offen).toBe(true);
		// Der laufende Balken endet bei heute (der Strich steht an seinem Ende).
		const ende = (laeuft?.balken?.links ?? 0) + (laeuft?.balken?.breite ?? 0);
		expect(Math.abs(ende - s.heute)).toBeLessThan(2);
		expect(fest?.balken).toBeNull();
		expect(fest?.raute).toBeGreaterThan(s.heute);
		// Der Termin liegt in der Zukunft: die Achse reicht bis zu ihm, heute steht nicht am Rand.
		expect(s.bis).toBe(`${tagIso(40, heute).slice(0, 7)}-01`);
		expect(s.heute).toBeLessThan(95);
		expect(fertig?.balken?.links).toBeGreaterThanOrEqual(0);
		expect(s.von).toBe(tagIso(-20, heute));
		// Gegenprobe: ohne jedes Datum gibt es keinen Streifen (nichts geschätzt).
		const leer = projektPlan(heute);
		leer.etappen = leer.etappen.map((e) => ({ ...e, begonnen: null, fertig: null, ab: null }));
		expect(zeitstreifen(leer, tagIso(0, heute))).toBeNull();
	});
});

describe('Zeitleiste', () => {
	const ereignis = (kennung: string, mehr: Partial<VerlaufEreignis> = {}): VerlaufEreignis => ({
		kennung,
		zeit: '2026-11-13T10:00:00+01:00',
		genauigkeit: 'minute',
		bis: null,
		titel: kennung,
		text: null,
		quelle: 'schritt',
		art: 'routine',
		verweis: null,
		...mehr
	});

	it('„Fertig: …“ und „Bau rot“ stehen nicht als Meldung, wenn der Verlauf sie als eigene Zeile hat', () => {
		const meldung = (id: string, art: 'fertig' | 'fehler', schluessel: string) =>
			anstehendEintrag({
				art: 'meldung',
				id,
				titel: id,
				meldung: {
					art,
					anzahl: 1,
					zuletzt: '2026-11-13T09:00:00+01:00',
					text: null,
					system: false,
					schluessel
				}
			});
		const liste = [
			meldung('fertig', 'fertig', 'beispiel/fertig:ar-1'),
			meldung('rot', 'fehler', 'beispiel/bau-rot'),
			meldung('anderes', 'fehler', 'beispiel/pruefung')
		];
		const verlauf = [
			ereignis('arbeit:ar-1:fertig', { quelle: 'arbeit', art: 'erledigt' }),
			ereignis('bau:v-9', { quelle: 'bau', art: 'problem' })
		];
		expect(ungeleseneMeldungen(liste, verlauf).map((m) => m.id)).toEqual(['anderes']);
		// Gegenprobe: ohne diese Zeilen im Verlauf (älter als 14 Tage, Bau wieder grün) stehen sie als Meldung da.
		expect(ungeleseneMeldungen(liste, []).map((m) => m.id)).toEqual(['fertig', 'rot', 'anderes']);
	});

	it('ungelesene Meldungen stehen eigens, nicht noch einmal im Verlauf', () => {
		const e = [ereignis('meldung:m-1', { quelle: 'meldung' }), ereignis('meldung:m-2')];
		expect(zeitleisteEreignisse(e, [{ id: 'm-1' }]).map((x) => x.kennung)).toEqual(['meldung:m-2']);
		expect(zeitleisteEreignisse(e, [])).toHaveLength(2);
	});

	it('Schritte je Arbeit und Tag in einer Zeile, grüne Bauten zusammen, Rotes einzeln', () => {
		const liste = [
			ereignis('schritt:ar-1/1:0', { text: 'Bei „Projektseiten neu“: fertig' }),
			ereignis('schritt:ar-1/2:1', { text: 'Bei „Projektseiten neu“: läuft', art: 'erledigt' }),
			ereignis('schritt:ar-1/3:2', { text: 'Bei „Projektseiten neu“: läuft' }),
			ereignis('schritt:ar-2/1:0', { text: 'Bei „Anderes“: läuft' }),
			ereignis('bau:v-1', { quelle: 'bau' }),
			ereignis('bau:v-2', { quelle: 'bau' }),
			ereignis('bau:v-3', { quelle: 'bau', art: 'problem' })
		].flatMap((e) => eintrag(e) ?? []);
		const z = routineZusammenfassen(liste, 't');
		expect(z.map((x) => (x.art === 'routine' ? x.titel : x.eintrag.e.kennung))).toEqual([
			'3 Schritte bei „Projektseiten neu“',
			'schritt:ar-2/1:0',
			'2 Bauten',
			'bau:v-3'
		]);
	});
});

describe('Laden der geschützten Wege', () => {
	it('/plan und /verlauf mit seit (14 Tage bis heute); 401 heißt anmelden, Netzfehler nicht', async () => {
		const f = vi.fn(() => Promise.resolve(json({ fehler: 'x' }, 401)));
		expect(await ladePlan('a b', f)).toEqual({ wert: null, anmelden: true });
		expect(f).toHaveBeenCalledWith('/api/projekte/a%20b/plan', expect.anything());
		await ladeProjektVerlauf('x', 14, f, new Date(2026, 10, 13, 12));
		expect(f).toHaveBeenLastCalledWith(
			'/api/projekte/x/verlauf?seit=2026-10-31',
			expect.anything()
		);
		const kaputt = (): Promise<Response> => Promise.reject(new Error('Netz'));
		expect(await ladePlan('x', kaputt)).toEqual({ wert: null, anmelden: false });
		// /backlog wie /plan: 401 heißt anmelden, ein Serverfehler nicht.
		expect(await ladeBacklog('x', f)).toEqual({ wert: null, anmelden: true });
		expect(f).toHaveBeenLastCalledWith('/api/projekte/x/backlog', expect.anything());
		const kaputtServer = vi.fn(() => Promise.resolve(json({ fehler: 'x' }, 500)));
		expect(await ladeBacklog('x', kaputtServer)).toEqual({ wert: null, anmelden: false });
		expect(await ladeBacklog('x', kaputt)).toEqual({ wert: null, anmelden: false });
		const gut = vi.fn(() => Promise.resolve(json(projektPlan())));
		expect((await ladePlan('x', gut)).wert?.etappen).toHaveLength(3);
	});
});
