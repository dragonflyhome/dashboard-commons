/**
 * Prüfbericht, Funde der Oberfläche (reine Auswertung, ohne
 * Komponenten): Kopf „Seite in Ordnung“ gegen rote Karte, archiviertes
 * Projekt ruhig, die Zahlen der Startseite, die Zahl-Kacheln in „Anstehend“.
 * Alles mit erfundenen Beispielen.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	SATZ_ARCHIV_RUHIG,
	auswerten,
	filterAus,
	gefiltert,
	nachPrioritaet,
	standAnsicht,
	startZahlen,
	unterDashboardAnsicht,
	zahlKacheln
} from './anstehend.ts';
import { SATZ_GEBAUT_STAND_ROT, bauAnzeige, kopfMitStand } from './projekte.ts';
import {
	beispielListe,
	beispielProjektStand,
	beispielZahlen,
	eintrag
} from './anstehend.beispiel.ts';
import type { Aktualisierung } from '$vertrag';

const JETZT = new Date(2026, 10, 2, 10, 0);
const GUT = new Date(2026, 10, 1, 18, 36).toISOString();

describe('Kopf der Projektseite: kein „Seite in Ordnung“ neben roter Karte', () => {
	it('grün und Stand nicht rot: bleibt, wie es ist', () => {
		const a = bauAnzeige({ zustand: 'gruen', letzte_gute: GUT }, null, null, JETZT);
		expect(kopfMitStand(a, false)).toBe(a);
		expect(a?.zeile.startsWith('Seite in Ordnung')).toBe(true);
	});
	it('grün, aber Stand rot: „Seite gebaut“, ohne Grün, mit Verweis auf die Karte', () => {
		const a = kopfMitStand(
			bauAnzeige({ zustand: 'gruen', letzte_gute: GUT }, null, null, JETZT),
			true
		);
		expect(a?.bedeutung).toBe('neutral');
		expect(a?.wort).toBe('Seite gebaut');
		expect(a?.zeile).not.toContain('in Ordnung');
		expect(a?.zeile).toContain(SATZ_GEBAUT_STAND_ROT);
		expect(a?.zeile).toContain('Fassung von');
	});
	it('rot, läuft oder ohne Seite: unverändert', () => {
		const rot = bauAnzeige({ zustand: 'rot', letzte_gute: GUT }, null, null, JETZT);
		expect(kopfMitStand(rot, true)).toBe(rot);
		const laeuft = bauAnzeige({ zustand: 'laeuft', letzte_gute: GUT }, null, null, JETZT);
		expect(kopfMitStand(laeuft, true)).toBe(laeuft);
		expect(kopfMitStand(null, true)).toBeNull();
	});
});

describe('Archiviertes Projekt: ruhig, ohne Ampel und ohne „neu“', () => {
	const meldung = (id: string): Aktualisierung => ({
		quelle: 'meldung',
		id,
		art: 'fehler',
		titel: 'Eine Meldung',
		zeit: '2026-11-01T10:00:00+01:00',
		ziel: `/projects/ruhig#meldung-${id}`,
		gelesen: false,
		anzahl: 1,
		behoben: false,
		text: null
	});
	const archiviert = beispielProjektStand({
		slug: 'ruhig',
		gesamt: {
			...beispielProjektStand().gesamt,
			status: 'archiviert',
			ampel: 'rot',
			grund: 'frist-vorbei',
			bezug_art: 'aufgabe',
			bezug: null
		}
	});

	it('standAnsicht: keine rote Ampel, der ruhige Satz', () => {
		const s = standAnsicht(archiviert, [], []);
		expect(s.gesamt.ruhig).toBe(true);
		expect(s.gesamt.ampel).not.toBe('rot');
		expect(s.gesamt.grund).toBe(SATZ_ARCHIV_RUHIG);
	});

	it('mit Anmeldung: drei ungelesene Meldungen zählen nicht als „neu“', () => {
		const akt = [meldung('m-1'), meldung('m-2'), meldung('m-3')];
		const u = unterDashboardAnsicht(archiviert, [], akt);
		expect(u.ungelesen).toBe(0);
		expect(u.stand?.gesamt.ampel).not.toBe('rot');
		// Aktiv: dieselben drei zählen.
		const aktiv = beispielProjektStand({ slug: 'ruhig' });
		expect(unterDashboardAnsicht(aktiv, [], akt).ungelesen).toBe(3);
	});

	it('ohne Anmeldung: die Zahl der Meldungen ohne „neu“', () => {
		const frei = {
			...archiviert,
			sichtbar: false,
			zahlen: beispielZahlen({ meldungen: { neu: 3, gesamt: 3 } })
		};
		const u = unterDashboardAnsicht(frei, null, null);
		expect(u.ungelesen).toBe(0);
		expect(u.meldungsZahlen).toEqual({ neu: 0, gesamt: 3 });
	});
});

describe('Startseite: was jede Zahl zählt', () => {
	it('vier Zahlen aus derselben Liste, je mit Wort und Ort', () => {
		const roh = beispielListe();
		roh.eintraege.push(
			eintrag({ art: 'meldung', id: 'm-1', stufe: 'jetzt', wartet_auf_dich: false }),
			eintrag({ id: 'a-2', titel: 'Noch eine Aufgabe', stufe: 'irgendwann' })
		);
		const l = auswerten(roh);
		const z = startZahlen(l);
		expect(z.map((x) => x.wort)).toEqual([
			'jetzt dran, alle Arten',
			'warten auf dich',
			'offene Aufgaben',
			'neue Meldung'
		]);
		const [jetzt, wartet, aufgaben, meldungen] = z;
		expect(jetzt?.zahl).toBe(l.zaehler.jetzt);
		expect(wartet?.zahl).toBe(l.zaehler.wartet_auf_dich);
		expect(aufgaben?.zahl).toBe(2);
		expect(meldungen?.zahl).toBe(1);
		for (const x of z) expect(x.wo).not.toBe('');
		// Die Aufgaben sind ein Teil von „warten auf dich“, nicht mehr.
		expect(aufgaben?.zahl ?? 0).toBeLessThanOrEqual(wartet?.zahl ?? 0);
	});
});

describe('„Anstehend“: die Zahl-Kacheln sind Links auf ihr Ziel', () => {
	/** Erfundene Liste: Fragen und Aufgaben über alle Stufen, nicht alle warten auf den Inhaber. */
	const liste = auswerten({
		...beispielListe(),
		eintraege: [
			eintrag({ art: 'frage', id: 'f-1', projekt: 'alpha', stufe: 'jetzt', wartet_auf_dich: true }),
			eintrag({ art: 'frage', id: 'f-2', projekt: 'beta', stufe: 'bald', wartet_auf_dich: true }),
			eintrag({ art: 'aufgabe', id: 'a-1', stufe: 'jetzt', wartet_auf_dich: true }),
			eintrag({ art: 'aufgabe', id: 'a-2', stufe: 'irgendwann', wartet_auf_dich: true }),
			eintrag({ art: 'meldung', id: 'm-1', stufe: 'jetzt', wartet_auf_dich: false })
		]
	}).eintraege;

	it('Stufen auf ihre Gruppe in „nach Priorität“, „wartet auf dich“ auf den Filter', () => {
		const k = zahlKacheln(liste, filterAus(new URLSearchParams('ansicht=schwerpunkt&art=frage')));
		expect(k.map((x) => [x.zahl, x.wort, x.suche])).toEqual([
			[1, 'Jetzt', '?art=frage#stufe-jetzt'],
			[1, 'Bald', '?art=frage#stufe-bald'],
			[0, 'Irgendwann', '?art=frage#stufe-irgendwann'],
			[2, 'wartet auf dich', '?ansicht=schwerpunkt&art=frage&nur=1']
		]);
		expect(k.map((x) => x.klasse)).toEqual(['jetzt', '', '', 'dich']);
	});

	it('jede Kachel zählt genau die Liste, die ihr Link zeigt', () => {
		for (const suche of ['', 'art=frage', 'art=aufgabe,meldung', 'nur=1', 'ansicht=schwerpunkt']) {
			const f = filterAus(new URLSearchParams(suche));
			for (const k of zahlKacheln(liste, f)) {
				const [abfrage = '', anker = ''] = k.suche.split('#');
				const ziel = filterAus(new URLSearchParams(abfrage.replace(/^\?/, '')));
				const gezeigt = gefiltert([...liste], ziel);
				const erwartet =
					anker === ''
						? gezeigt.length
						: (nachPrioritaet(gezeigt).find((g) => `stufe-${g.stufe}` === anker)?.eintraege
								.length ?? -1);
				expect(k.zahl, `${suche} → ${k.wort}`).toBe(erwartet);
			}
		}
	});

	it('ohne Filter sind es die Zahlen von Reiter und Tab', () => {
		const z = auswerten({ ...beispielListe(), eintraege: liste }).zaehler;
		const k = zahlKacheln(liste, filterAus(new URLSearchParams('')));
		expect(k.map((x) => x.zahl)).toEqual([z.jetzt, z.bald, z.irgendwann, z.wartet_auf_dich]);
	});
});

describe('Startseite: „Jetzt für dich“ zuerst, Aufgaben gekürzt, Zahlen erklärt', () => {
	const seite = readFileSync(join(import.meta.dirname, '..', 'routes', '+page.svelte'), 'utf8');
	it('„Jetzt für dich“ steht vor dem Kasten „Für dich“', () => {
		const jetzt = seite.indexOf('<JetztFuerDich');
		const kasten = seite.indexOf('<AufgabenKasten');
		expect(jetzt).toBeGreaterThan(0);
		expect(kasten).toBeGreaterThan(jetzt);
	});
	it('der Kasten zeigt nur die ersten Aufgaben, die Zahlenzeile kommt aus startZahlen', () => {
		expect(seite).toContain('hoechstens={AUFGABEN_OBEN}');
		expect(seite).toMatch(/const AUFGABEN_OBEN = \d+;/);
		expect(seite).toContain('startZahlen(anstehend.liste)');
	});
});
