/**
 * Etappe 4c: Bau-Stand der Seiten, ausgewertet nur in projekte.ts
 * (`bauAnzeige`, `auswerten`, `leistenGruppen`). Ohne Zeitzeile
 * und und ohne Knopf zum Bauen.
 */
import { describe, expect, it } from 'vitest';
import type { BauEvent, ProjectSummary, ProjektBau } from '$vertrag';
import { fmtTagUhrzeit } from './format.ts';
import {
	auswerten,
	bauAnzeige,
	bauJeProjekt,
	leistenGruppen,
	zaehleProjekte,
	zaehlzeile
} from './projekte.ts';

/** Samstag, 31.10.2026, 14:05 Ortszeit. */
const GUT = new Date(2026, 9, 31, 14, 5).toISOString();
const JETZT = new Date(2026, 9, 31, 18, 0);

function projekt(slug: string, bau?: ProjektBau): ProjectSummary {
	return {
		slug,
		title: slug,
		status: 'aktiv',
		bedeutung: 'laeuft',
		last_change: '2026-10-31',
		summary: '',
		chat: null,
		backup: null,
		backup_bedeutung: null,
		backup_note: '',
		vorschau: null,
		order: 100,
		open: [],
		repos: [],
		file_modified: null,
		...(bau === undefined ? {} : { bau })
	};
}

const ereignis = (slug: string, zustand: BauEvent['zustand'], gescheitert = false): BauEvent => ({
	slug,
	zustand,
	meldung: 'x',
	gescheitert
});

describe('Tag und Uhrzeit (Etappe 4c)', () => {
	it('„Sa 31.10., 14:05 Uhr“, aus einem anderen Jahr mit Jahr; leer oder unlesbar null', () => {
		expect(fmtTagUhrzeit(GUT, JETZT)).toBe('Sa 31.10., 14:05 Uhr');
		expect(fmtTagUhrzeit(new Date(2025, 11, 30, 9, 0).toISOString(), JETZT)).toBe(
			'Di 30.12.2025, 09:00 Uhr'
		);
		expect(fmtTagUhrzeit(null, JETZT)).toBeNull();
		expect(fmtTagUhrzeit('', JETZT)).toBeNull();
		expect(fmtTagUhrzeit('kaputt', JETZT)).toBeNull();
	});
});

describe('bauAnzeige: die eine Auswertung des Bau-Stands (Etappe 4c)', () => {
	it('grün, rot mit „zeigt die letzte gute Fassung von …“, läuft, noch nicht gebaut', () => {
		const gruen = bauAnzeige(
			{ zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT },
			null,
			null,
			JETZT
		);
		expect(gruen).toMatchObject({
			zustand: 'gruen',
			bedeutung: 'ok',
			wort: 'Seite in Ordnung',
			// Abnahme 4c, live: „gebaut …“ sah nach einem frischen Bau aus, als sei nichts passiert.
			zeile: 'Seite in Ordnung',
			satz: null
		});
		const rot = bauAnzeige(
			{ zustand: 'rot', letzte_gute: GUT, gebaut_um: GUT },
			null,
			'Satz vom Server.',
			JETZT
		);
		expect(rot).toMatchObject({
			zustand: 'rot',
			bedeutung: 'problem',
			wort: 'Seite mit Fehler',
			zeile: 'Seite mit Fehler: zeigt die letzte gute Fassung von Sa 31.10., 14:05 Uhr',
			satz: 'Satz vom Server.',
			zeit: 'Sa 31.10., 14:05 Uhr'
		});
		expect(bauAnzeige({ zustand: 'rot', letzte_gute: null }, null, null, JETZT)?.zeile).toBe(
			'Seite mit Fehler, gerade nicht zu sehen'
		);
		expect(bauAnzeige({ zustand: 'unbekannt', letzte_gute: null })).toMatchObject({
			bedeutung: 'wartet',
			zeile: 'Bau-Stand gerade nicht lesbar'
		});
		expect(
			bauAnzeige({ zustand: 'laeuft', letzte_gute: GUT, gebaut_um: GUT }, null, null, JETZT)
		).toMatchObject({
			bedeutung: 'laeuft',
			zeile: 'Seite wird gebaut …'
		});
		expect(bauAnzeige({ zustand: 'noch_nicht', letzte_gute: null })).toMatchObject({
			bedeutung: 'neutral',
			zeile: 'Seite noch nicht gebaut'
		});
	});

	it('ohne Bau-Stand (Projekt ohne eigene Seite) nichts, auch wenn ein Ereignis kommt', () => {
		expect(bauAnzeige(undefined, ereignis('x', 'rot'))).toBeNull();
		expect(bauAnzeige(null)).toBeNull();
	});

	it('das Ereignis `bau` gilt vor der Liste (live); der Satz nur bei rot und nicht geprüft', () => {
		const frei: ProjektBau = { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT };
		expect(bauAnzeige(frei, ereignis('a', 'laeuft'))?.zustand).toBe('laeuft');
		expect(bauAnzeige(frei, ereignis('a', 'rot'), 'Satz', JETZT)).toMatchObject({
			zustand: 'rot',
			satz: 'Satz'
		});
		expect(bauAnzeige(frei, null, 'alter Satz')?.satz).toBeNull();
		const ng: ProjektBau = { zustand: 'nicht_geprueft', letzte_gute: GUT, gebaut_um: GUT };
		expect(bauAnzeige(ng, null, 'Satz', JETZT)).toMatchObject({
			bedeutung: 'wartet',
			zeile: `Seite noch nicht geprüft: zeigt die letzte geprüfte Fassung von ${fmtTagUhrzeit(GUT, JETZT)}`,
			satz: 'Satz'
		});
		// Ohne Anmeldung (kein Satz) nur der Zustand.
		expect(bauAnzeige(ng, null, null, JETZT)?.zeile).toBe('Seite noch nicht geprüft');
		// Ein Ereignis grün zeigt bis zur nächsten Liste „wird gebaut“, nie „in Ordnung“.
		expect(bauAnzeige(ng, ereignis('a', 'gruen'))?.zustand).toBe('laeuft');
		expect(bauAnzeige(frei, ereignis('a', 'gruen'))?.zustand).toBe('laeuft');
	});

	it('Abnahme 4c: ein ganzer Bau, der nicht durchlief, macht die Seite nicht rot', () => {
		const frei: ProjektBau = { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT };
		expect(bauAnzeige(frei, ereignis('a', 'rot', true))?.zustand).toBe('gruen');
		// Stand beim Laden „läuft“: ohne neue Liste unbekannt statt eines falschen Rot.
		expect(
			bauAnzeige(
				{ zustand: 'laeuft', letzte_gute: GUT, gebaut_um: GUT },
				ereignis('a', 'rot', true)
			)?.zustand
		).toBe('unbekannt');
	});
});

describe('Startseite und Seitenleiste lesen dieselbe Auswertung (Etappe 4c)', () => {
	const LISTE = [
		projekt('gut', { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT }),
		projekt('kaputt', { zustand: 'rot', letzte_gute: GUT, gebaut_um: GUT }),
		projekt('baut', { zustand: 'laeuft', letzte_gute: null }),
		projekt('alt-form')
	];
	const ALLE = { suche: '', status: 'alle', sortierung: 'order' } as const;

	it('auswerten: Bau-Stand je Projekt mit eigener Seite und „1 Seite mit Fehler“ in der Zählzeile', () => {
		const a = auswerten(LISTE, ALLE);
		expect(Object.keys(a.bau).sort()).toEqual(['baut', 'gut', 'kaputt']);
		expect(a.zaehlung.seitenRot).toBe(1);
		expect(a.zeile).toBe('4 Projekte: 4 aktiv · 1 Seite mit Fehler');
		// Live: ein zweites Projekt wird rot, die Zählung folgt ohne neue Liste.
		const b = auswerten(LISTE, ALLE, { gut: ereignis('gut', 'rot') });
		expect(b.zaehlung.seitenRot).toBe(2);
		expect(b.zeile).toBe('4 Projekte: 4 aktiv · 2 Seiten mit Fehler');
		expect(b.bau.gut?.zustand).toBe('rot');
	});

	it('Zählzeile ohne rote Seiten wie bisher; Zählung ohne Bau-Stand zählt keine', () => {
		const z = zaehleProjekte([projekt('a'), projekt('b')]);
		expect(z.seitenRot).toBe(0);
		expect(zaehlzeile(z)).toBe('2 Projekte: 2 aktiv');
	});

	it('Seitenleiste: derselbe Bau-Stand je Projekt, live wie auf der Startseite', () => {
		const g = leistenGruppen(LISTE, {}, { baut: ereignis('baut', 'gruen') });
		const je = Object.fromEntries(g.flatMap((x) => x.projekte).map((p) => [p.slug, p.bau]));
		const start = bauJeProjekt(LISTE, { baut: ereignis('baut', 'gruen') });
		expect(je['alt-form']).toBeNull();
		for (const slug of ['gut', 'kaputt', 'baut']) expect(je[slug]).toEqual(start[slug]);
		// Ein grünes Ereignis zeigt bis zur nächsten Liste „wird gebaut“.
		expect(je.baut?.zustand).toBe('laeuft');
	});
});
