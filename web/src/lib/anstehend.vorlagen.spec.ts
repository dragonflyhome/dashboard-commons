/**
 * Etappe 3a+: Gleichwertigkeit der Auswertung im Web mit der früheren
 * Rechnung im Server. Die Vorlagen liegen unter tests/vorlagen/:
 *
 * - `alt/`: was der Server vor dem Umbau für eine feste Testdatenlage
 *   lieferte (/api/anstehend, /api/anstehend?projekt=, /stand, /stand/voll;
 *   einmal geschrieben, bleibt unverändert als Bezug).
 * - `neu/`: was der Server jetzt für dieselben Daten liefert (alle Einträge,
 *   neutral geordnet, ohne Zahlen; cargo test hält die Dateien aktuell).
 * - `faelle/`: die Fälle der früheren Sortier-Tests in Rust (src/anstehend.rs).
 *
 * Gewollte Unterschiede (einzeln, nur auf dem freien Weg ohne Anmeldung;
 * Entscheidung „/stand ohne Anmeldung nur Zahlen“):
 * 1. Keine Arbeiten mehr mit Titel, nur ihre Zahl je Gruppe (`weitere`).
 * 2. Keine aktuelle Frage mehr, nur die Zahl (`frage.aktuell` null, `weitere` 0).
 * 3. Ampel rot wegen einer Frist, die vorbei ist: ohne Titel des Eintrags,
 *    auch bei Arbeiten und Fragen („Die Frist einer Arbeit ist vorbei.“).
 * 4. Etappe B2: Notizen (open[]) nur als Zahl (`notizen_zahl`).
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	STUFEN,
	aktualisierungenWaehlen,
	auswerten,
	bandMeldungen,
	gefiltert,
	jetztFuerDich,
	nachPrioritaet,
	nachSchwerpunkt,
	standAnsicht,
	standZahlen,
	unterDashboardAnsicht,
	zaehlen,
	type AnstehendZaehler,
	type StandAnsicht
} from './anstehend.ts';
import type {
	Aktualisierung,
	AnstehendEintrag,
	AnstehendListe,
	MeldungsZahlen,
	ProjektStand
} from '$vertrag';

const ORDNER = join(import.meta.dirname, '..', '..', '..', 'tests', 'vorlagen');

/**
 * Etappe B2: Seit der Warteschlange wählt der Server die sichtbare Frage
 * (Stufe, dann Frist, dann Reihenfolge in der Datei; `warteschlange`). Diese
 * Vergleiche prüfen die Auswertung der Oberfläche gegen die alte Rechnung;
 * darum lesen sie die Vorlagen ohne den Platz, und `auswaehlen` nimmt die alte
 * Regel. Die neue Regel prüfen die Rust-Tests (`fragen_warteschlange`) und
 * backlog.svelte.spec.ts.
 */
function ohnePlatz(wert: unknown): unknown {
	if (Array.isArray(wert)) return wert.map(ohnePlatz);
	if (wert !== null && typeof wert === 'object') {
		return Object.fromEntries(
			Object.entries(wert)
				.filter(([k]) => k !== 'warteschlange')
				.map(([k, v]) => [k, ohnePlatz(v)])
		);
	}
	return wert;
}

function lies(name: string): unknown {
	return ohnePlatz(JSON.parse(readFileSync(join(ORDNER, name), 'utf8')));
}

// ----- Die alten Formen (vor 3a+), nur für den Vergleich -----

type AltEintrag = Omit<AnstehendEintrag, 'schwerpunkt_reihenfolge' | 'wartet_auf_dich'>;
interface AltListe {
	heute: string;
	frist_bald_tage: number;
	eintraege: AltEintrag[];
	zaehler: AnstehendZaehler;
	schwerpunkte: { name: string; reihenfolge: number; projekte: string[] }[];
	hinweise: string[];
	aktualisierungen: Aktualisierung[];
}
interface AltGruppe {
	eintraege: AltEintrag[];
	weitere: number;
}
interface AltStand {
	slug: string;
	schwerpunkt: string;
	gesamt: {
		titel: string;
		status: string;
		bedeutung: string;
		ampel: string;
		grund: string;
		zuletzt: string;
	};
	kurz: StandAnsicht['kurz'];
	frage: { aktuell: AltEintrag | null; weitere: number };
	aufgaben: { offen: AltEintrag[]; anzahl: number; weitere: number; sichtbar: boolean };
	aktualisierungen: Aktualisierung[];
	meldungen: MeldungsZahlen;
	arbeiten: { laufen: AltGruppe; wartet: AltGruppe; als_naechstes: AltGruppe };
	notizen: string[];
	speicher_fehlt: boolean;
}

/** Ein Eintrag ohne die Fakten, die 3a+ neu dazugibt. */
function alt(e: AnstehendEintrag): AltEintrag {
	const kopie: Partial<AnstehendEintrag> = { ...e };
	delete kopie.schwerpunkt_reihenfolge;
	delete kopie.wartet_auf_dich;
	// Etappe B2: der Platz in der Fragen-Warteschlange kam als Fakt dazu (gewählt
	// wird dieselbe Frage; die Reihenfolge vergleicht der Test oben).
	delete kopie.warteschlange;
	// Kopfleiste, Fassung 3: der Schlüssel einer Meldung kam als Fakt dazu.
	if (kopie.meldung) {
		const m = { ...kopie.meldung };
		delete m.schluessel;
		kopie.meldung = m;
	}
	return kopie as AltEintrag;
}
const ids = (l: readonly { id: string }[]): string[] => l.map((e) => e.id);

/**
 * Die Gruppierung „nach Schwerpunkt“, wie sie bis 3a+ im Web stand (mit der
 * Liste `schwerpunkte` des Servers): eingefroren als Bezug.
 */
function altNachSchwerpunkt(eintraege: AltEintrag[], schwerpunkte: AltListe['schwerpunkte']) {
	const nachFrist = (a: AltEintrag, b: AltEintrag): number => {
		if (a.frist === b.frist) return 0;
		if (a.frist === null) return 1;
		if (b.frist === null) return -1;
		return a.frist < b.frist ? -1 : 1;
	};
	const nachStufe = (a: AltEintrag, b: AltEintrag): number =>
		STUFEN.indexOf(a.stufe.wirksam) - STUFEN.indexOf(b.stufe.wirksam) || nachFrist(a, b);
	const namen = schwerpunkte.map((s) => s.name);
	for (const e of eintraege) if (!namen.includes(e.schwerpunkt)) namen.push(e.schwerpunkt);
	const gruppen: {
		name: string;
		projekte: { projekt: string; ids: string[] }[];
		anzahl: number;
	}[] = [];
	for (const name of namen) {
		const darin = eintraege.filter((e) => e.schwerpunkt === name);
		if (darin.length === 0) continue;
		const reihe = [...(schwerpunkte.find((s) => s.name === name)?.projekte ?? [])];
		for (const e of darin) if (!reihe.includes(e.projekt)) reihe.push(e.projekt);
		const projekte = reihe
			.map((projekt) => ({
				projekt,
				ids: ids(darin.filter((e) => e.projekt === projekt).sort(nachStufe))
			}))
			.filter((p) => p.ids.length > 0);
		gruppen.push({ name, projekte, anzahl: darin.length });
	}
	return gruppen;
}

const SLUGS = ['beispiel', 'dashboard', 'hallo'] as const;

describe('Gleichwertig mit dem Server vor 3a+: /api/anstehend', () => {
	const vorher = lies('alt/anstehend.json') as AltListe;
	const roh = lies('neu/anstehend.json') as AnstehendListe;
	const jetzt = auswerten(roh);

	it('der neue Server liefert alles, neutral nach Projekt, Art, Kennung, ohne Zahlen', () => {
		expect(roh).not.toHaveProperty('zaehler');
		expect(roh).not.toHaveProperty('schwerpunkte');
		const rang = { aufgabe: 0, frage: 1, arbeit: 2, meldung: 3 };
		const schluessel = roh.eintraege.map((e) => `${e.projekt}|${String(rang[e.art])}|${e.id}`);
		expect(schluessel).toEqual([...schluessel].sort());
		// Mehr als vorher: alle offenen Fragen (vorher eine je Projekt).
		expect(roh.eintraege.length).toBeGreaterThan(vorher.eintraege.length);
	});

	it('dieselbe Reihenfolge und dieselben Einträge (eine Frage je Projekt, mit Vermerk)', () => {
		expect(ids(jetzt.eintraege)).toEqual(ids(vorher.eintraege));
		expect(jetzt.eintraege.map(alt)).toEqual(vorher.eintraege);
	});

	it('dieselben Zahlen', () => {
		expect(jetzt.zaehler).toEqual(vorher.zaehler);
	});

	it('dieselben Gruppen nach Priorität und nach Schwerpunkt', () => {
		const p = (l: readonly AltEintrag[]) =>
			nachPrioritaet(l as AnstehendEintrag[]).map((g) => [g.stufe, ids(g.eintraege)]);
		expect(p(jetzt.eintraege)).toEqual(p(vorher.eintraege));
		const neu = nachSchwerpunkt(jetzt.eintraege).map((g) => ({
			name: g.name,
			projekte: g.projekte.map((x) => ({ projekt: x.projekt, ids: ids(x.eintraege) })),
			anzahl: g.anzahl
		}));
		expect(neu).toEqual(altNachSchwerpunkt(vorher.eintraege, vorher.schwerpunkte));
		// Sonstiges (mit dem unbekannten Projekt) steht zuletzt.
		expect(neu.at(-1)?.name).toBe('Sonstiges');
	});

	it('Startseite und Band: dieselben Projekte und Meldungen', () => {
		// Die alten Einträge kennen den Fakt noch nicht: die alte Regel des Servers dazu.
		const mitFakt = (e: AltEintrag): AnstehendEintrag => ({
			...e,
			schwerpunkt_reihenfolge: 0,
			wartet_auf_dich:
				e.art === 'aufgabe' ||
				e.art === 'frage' ||
				(e.art === 'arbeit' && e.wartet_auf === 'inhaber' && e.vermerk === null)
		});
		const j = (l: readonly AltEintrag[]) =>
			jetztFuerDich(l.map(mitFakt)).map((x) => [x.projekt, x.frage?.id ?? null, ids(x.jetzt)]);
		expect(j(jetzt.eintraege)).toEqual(j(vorher.eintraege));
		const b = (l: readonly AltEintrag[]) => ids(bandMeldungen(l as AnstehendEintrag[]).alle);
		expect(b(jetzt.eintraege)).toEqual(b(vorher.eintraege));
	});

	it.each(SLUGS)('mit Filter projekt=%s: Einträge, Zahlen, Aktualisierungen', (slug) => {
		const a = lies(`alt/anstehend-${slug}.json`) as AltListe;
		const n = auswerten(lies(`neu/anstehend-${slug}.json`) as AnstehendListe);
		expect(n.eintraege.map(alt)).toEqual(a.eintraege);
		expect(n.zaehler).toEqual(a.zaehler);
		expect(aktualisierungenWaehlen(n.aktualisierungen)).toEqual(a.aktualisierungen);
	});
});

describe('Gleichwertig mit dem Server vor 3a+: Unter-Dashboard', () => {
	function vergleichbar(s: StandAnsicht | AltStand) {
		const gruppe = (g: AltGruppe | StandAnsicht['arbeiten']['laufen']) => ({
			ids: ids(g.eintraege),
			weitere: g.weitere
		});
		return {
			slug: s.slug,
			schwerpunkt: s.schwerpunkt,
			gesamt: {
				titel: s.gesamt.titel,
				status: s.gesamt.status,
				bedeutung: s.gesamt.bedeutung,
				ampel: s.gesamt.ampel,
				grund: s.gesamt.grund,
				zuletzt: s.gesamt.zuletzt
			},
			kurz: s.kurz,
			frage: {
				aktuell: s.frage.aktuell === null ? null : alt(s.frage.aktuell as AnstehendEintrag),
				weitere: s.frage.weitere
			},
			aufgaben: { ...s.aufgaben, offen: s.aufgaben.offen.map((e) => alt(e as AnstehendEintrag)) },
			aktualisierungen: s.aktualisierungen,
			meldungen: s.meldungen,
			arbeiten: {
				laufen: gruppe(s.arbeiten.laufen),
				wartet: gruppe(s.arbeiten.wartet),
				als_naechstes: gruppe(s.arbeiten.als_naechstes)
			},
			notizen: s.notizen,
			speicher_fehlt: s.speicher_fehlt
		};
	}

	it.each(SLUGS)(
		'/stand/voll %s: alles gleich (Gruppen, Zahlen, Satz, Aktualisierungen)',
		(slug) => {
			const a = lies(`alt/stand-${slug}-voll.json`) as AltStand;
			const roh = lies(`neu/stand-${slug}-voll.json`) as ProjektStand;
			expect(roh.zahlen).toBeNull();
			expect(vergleichbar(standAnsicht(roh))).toEqual(vergleichbar(a));
		}
	);

	it.each(SLUGS)(
		'/stand ohne Anmeldung %s: Zahlen gleich, Einträge nur als Zahl (gewollt)',
		(slug) => {
			const a = lies(`alt/stand-${slug}-frei.json`) as AltStand;
			const roh = lies(`neu/stand-${slug}-frei.json`) as ProjektStand;
			expect(roh.eintraege).toEqual([]);
			const n = vergleichbar(standAnsicht(roh));
			const v = vergleichbar(a);
			expect(n.kurz).toEqual(v.kurz);
			expect(n.aufgaben).toEqual(v.aufgaben);
			expect(n.meldungen).toEqual(v.meldungen);
			expect(n.aktualisierungen).toEqual([]);
			// Gewollt 4 (Etappe B2): Notizen (open[]) ohne Anmeldung nur als Zahl.
			expect(n.notizen).toEqual([]);
			expect(standAnsicht(roh).notizen_zahl).toBe(v.notizen.length);
			expect({ ...n.gesamt, grund: '' }).toEqual({ ...v.gesamt, grund: '' });
			// Gewollt 1: Arbeiten nur als Zahl, die Summe je Gruppe bleibt.
			for (const g of ['laufen', 'wartet', 'als_naechstes'] as const) {
				expect(n.arbeiten[g].ids).toEqual([]);
				expect(n.arbeiten[g].weitere).toBe(v.arbeiten[g].ids.length + v.arbeiten[g].weitere);
			}
			// Gewollt 2: keine aktuelle Frage ohne Anmeldung.
			expect(n.frage).toEqual({ aktuell: null, weitere: 0 });
		}
	);

	it('gewollt 3: der Satz zur Ampel ohne Titel, nur dort, wo der alte ihn nannte', () => {
		const satz = (slug: string): [string, string] => [
			(lies(`alt/stand-${slug}-frei.json`) as AltStand).gesamt.grund,
			standAnsicht(lies(`neu/stand-${slug}-frei.json`) as ProjektStand).gesamt.grund
		];
		expect(satz('beispiel')).toEqual(['Ein Fehler ist offen.', 'Ein Fehler ist offen.']);
		expect(satz('dashboard')).toEqual(['Ein Fehler ist offen.', 'Ein Fehler ist offen.']);
		// Früher nannte der freie Weg den Titel einer Arbeit; jetzt nicht mehr.
		expect(satz('hallo')).toEqual([
			'Die Frist von „Arbeit ar-20261029-0010“ ist vorbei.',
			'Die Frist einer Arbeit ist vorbei.'
		]);
	});
});

describe('Unter-Dashboard mit Anmeldung: unterDashboardAnsicht gegen die Vorlage', () => {
	// Abnahme 3a+, Nachbesserung 1: Die Komponente zeigt nur, was diese Funktion
	// liefert; mit Anmeldung muss das zur alten Antwort von /stand/voll passen.
	const faelle = SLUGS.flatMap((slug) => [
		{ slug, weg: 'Liste und Stand' as const },
		{ slug, weg: 'nur /stand/voll' as const }
	]);
	it.each(faelle)('$slug ($weg)', ({ slug, weg }) => {
		const a = lies(`alt/stand-${slug}-voll.json`) as AltStand;
		const stand = lies(`neu/stand-${slug}-voll.json`) as ProjektStand;
		const liste = lies(`neu/anstehend-${slug}.json`) as AnstehendListe;
		const u =
			weg === 'Liste und Stand'
				? unterDashboardAnsicht(stand, liste.eintraege, liste.aktualisierungen)
				: unterDashboardAnsicht(stand);
		expect(u.angemeldet).toBe(true);
		expect(u.meldungsZahlen).toBeNull();
		// Wartet auf dich: Zahl, Aufgaben (die ersten drei wie früher), die eine Frage, Arbeiten für dich.
		expect(u.wartet.zahl).toBe(a.kurz.wartet_auf_dich);
		expect(u.wartet.aufgabenZahl).toBe(a.aufgaben.anzahl);
		expect(ids(u.wartet.aufgaben.slice(0, 3))).toEqual(ids(a.aufgaben.offen));
		expect(u.wartet.fragen.map(alt)).toEqual(a.frage.aktuell === null ? [] : [a.frage.aktuell]);
		expect(u.wartet.arbeiten.every((e) => e.art === 'arbeit' && e.wartet_auf_dich)).toBe(true);
		// Arbeiten je Zustand: mit Anmeldung alle; die ersten wie die alte Gruppe, die Summe gleich.
		const alteGruppen = [
			['läuft', a.arbeiten.laufen],
			['wartet', a.arbeiten.wartet],
			['als Nächstes', a.arbeiten.als_naechstes]
		] as const;
		const erwartet = alteGruppen
			.filter(([, g]) => g.eintraege.length + g.weitere > 0)
			.map(([wort, g]) => ({
				wort,
				vorn: ids(g.eintraege),
				summe: g.eintraege.length + g.weitere
			}));
		expect(
			u.arbeitsGruppen.map((g) => ({
				wort: g.wort,
				vorn: ids(g.eintraege.slice(0, 2)),
				summe: g.eintraege.length + g.weitere
			}))
		).toEqual(erwartet);
		// Aktualisierungen: die fünf neuesten wie früher; „N neu“ über alle Meldungen des Projekts.
		expect(u.aktualisierungen).toEqual(a.aktualisierungen);
		expect(u.ungelesen).toBe(a.meldungen.neu);
		expect(u.stand?.kurz).toEqual(a.kurz);
		expect(u.stand?.gesamt.grund).toBe(a.gesamt.grund);
		expect(u.zeigen).toBe(true);
	});

	it('„N neu“ zählt alle ungelesenen Meldungen, nicht nur die fünf angezeigten', () => {
		const vorlage = lies('neu/stand-hallo-voll.json') as ProjektStand;
		const ungelesen = Array.from({ length: 7 }, (_, n): Aktualisierung => ({
			quelle: 'meldung',
			id: `m-x${String(n)}`,
			art: 'info',
			titel: `Meldung ${String(n)}`,
			zeit: `2026-10-29T1${String(n)}:00:00+01:00`,
			ziel: `/projects/hallo#meldung-m-x${String(n)}`,
			gelesen: false,
			anzahl: 1,
			behoben: false,
			text: null
		}));
		const erste = ungelesen[0];
		if (erste === undefined) throw new Error('Beispiel fehlt');
		const gelesen: Aktualisierung = {
			...erste,
			id: 'm-alt',
			gelesen: true,
			zeit: '2026-10-20T08:00:00+01:00'
		};
		const u = unterDashboardAnsicht({ ...vorlage, aktualisierungen: [...ungelesen, gelesen] });
		expect(u.aktualisierungen).toHaveLength(5);
		expect(u.ungelesen).toBe(7);
		expect(u.stand?.meldungen).toEqual({ neu: 7, gesamt: 8 });
	});
});

describe('Freier Weg: die Zahlen des Servers gleich der Zählung im Web', () => {
	it.each(SLUGS)('%s: zahlen aus /stand = standZahlen aus /stand/voll', (slug) => {
		const frei = lies(`neu/stand-${slug}-frei.json`) as ProjektStand;
		const voll = lies(`neu/stand-${slug}-voll.json`) as ProjektStand;
		expect(frei.zahlen).not.toBeNull();
		expect(standZahlen(voll.eintraege, voll.aktualisierungen)).toEqual(frei.zahlen);
	});
});

// ----- Die Fälle der früheren Sortier-Tests in Rust (src/anstehend.rs) -----

describe('Fälle aus Rust: Sortierung, eine Frage, Zahlen', () => {
	const grund = auswerten(lies('faelle/grund.json') as AnstehendListe);

	it('Stufe, dann Frist (ohne zuletzt), dann Alter; eine Frage je Projekt', () => {
		expect(ids(grund.eintraege)).toEqual([
			'a-20261029-0002', // jetzt (Frist heute)
			'a-20261029-0003', // jetzt (Vorschlag), ohne Frist
			// „eilig“ (bald durch Frist morgen) und „schlusssatz“ (bald) gleichauf: die ältere gilt.
			'schlusssatz',
			'ar-20261029-0001', // bald, ohne Frist, jünger
			'ar-20261029-0003', // irgendwann, älter
			'a-20261029-0001' // irgendwann, jünger
		]);
		expect(grund.eintraege.find((e) => e.id === 'schlusssatz')?.vermerk).toBe(
			'noch 1 Frage wartet'
		);
		expect(grund.eintraege.some((e) => e.id === 'eilig')).toBe(false);
		// Je Projekt zählt höchstens eine Frage.
		expect(grund.zaehler).toEqual({ jetzt: 2, bald: 2, irgendwann: 2, wartet_auf_dich: 4 });
	});

	it('der Inhaber stuft um: die Reihenfolge ändert sich', () => {
		const l = auswerten(lies('faelle/einstufung.json') as AnstehendListe);
		const pos = ids(l.eintraege).indexOf('a-20261029-0001');
		expect(pos).toBeGreaterThanOrEqual(0);
		expect(pos).toBeLessThan(3);
		expect(l.eintraege[pos]?.stufe.grund).toBe('inhaber');
		expect(ids(grund.eintraege).at(-1)).toBe('a-20261029-0001');
	});

	it('Filter: Zahlen bleiben beim Filter nach Art; nach Schwerpunkt gezählt; nur für mich', () => {
		const fragen = gefiltert(grund.eintraege, {
			ansicht: 'prioritaet',
			arten: ['frage'],
			nur: false
		});
		expect(ids(fragen)).toEqual(['schlusssatz']);
		const pc = grund.eintraege.filter((e) => e.schwerpunkt === 'PC');
		expect(ids(pc)).toEqual(['ar-20261029-0001', 'a-20261029-0001']);
		expect(zaehlen(pc)).toEqual({ jetzt: 0, bald: 1, irgendwann: 1, wartet_auf_dich: 1 });
		const nur = gefiltert(grund.eintraege, { ansicht: 'prioritaet', arten: [], nur: true });
		expect(nur.every((e) => e.art !== 'arbeit')).toBe(true);
		expect(nur.length).toBe(grund.zaehler.wartet_auf_dich);
		// Die Schwerpunkte in der Reihenfolge der Datei, Sonstiges zuletzt.
		expect(nachSchwerpunkt(grund.eintraege).map((g) => g.name)).toEqual([
			'Arbeit',
			'PC',
			'Sonstiges'
		]);
	});

	it('eine Arbeit mit Verweis auf eine offene Frage zählt nicht doppelt', () => {
		const l = auswerten(lies('faelle/verweis.json') as AnstehendListe);
		const a = l.eintraege.find((e) => e.id === 'ar-20261029-0009');
		expect(a?.vermerk).toBe('wartet auf deine Antwort');
		expect(l.zaehler.wartet_auf_dich).toBe(grund.zaehler.wartet_auf_dich);
	});

	it('beantwortet: die nächste Frage rückt nach, der Vermerk zählt mit', () => {
		const vorher = standAnsicht(lies('faelle/beantwortet-0.json') as ProjektStand);
		expect([vorher.frage.aktuell?.id, vorher.frage.weitere]).toEqual(['schlusssatz', 2]);
		expect(vorher.frage.aktuell?.vermerk).toBe('noch 2 Fragen warten');
		const nachher = standAnsicht(lies('faelle/beantwortet-1.json') as ProjektStand);
		expect([nachher.frage.aktuell?.id, nachher.frage.weitere]).toEqual(['eilig', 1]);
		expect(nachher.frage.aktuell?.vermerk).toBe('noch 1 Frage wartet');
		expect(nachher.kurz.wartet_auf_dich).toBe(vorher.kurz.wartet_auf_dich);
	});

	it('höchstens drei Aufgaben, dann „weitere“', () => {
		const s = standAnsicht(lies('faelle/viele-aufgaben.json') as ProjektStand);
		expect([s.aufgaben.offen.length, s.aufgaben.anzahl, s.aufgaben.weitere]).toEqual([3, 6, 3]);
	});

	it('Meldungen nach Stufe, dann Alter; Aktualisierungen neueste zuerst, höchstens fünf', () => {
		const l = auswerten(lies('faelle/meldungen.json') as AnstehendListe);
		const m = l.eintraege.filter((e) => e.art === 'meldung');
		expect(ids(m)).toEqual([
			'm-1',
			'm-4',
			'm-7',
			'm-2',
			'm-10',
			'm-11',
			'm-3',
			'm-12',
			'm-13',
			'm-14'
		]);
		const h = lies('faelle/meldungen-hallo.json') as AnstehendListe;
		expect(h.aktualisierungen.length).toBeGreaterThan(5);
		// Bei gleicher Zeit (m-3 und m-11 um 10 Uhr) nach Kennung absteigend, wie früher im Server.
		expect(ids(aktualisierungenWaehlen(h.aktualisierungen))).toEqual([
			'm-14',
			'm-13',
			'm-12',
			'm-3',
			'm-11'
		]);
	});
});
