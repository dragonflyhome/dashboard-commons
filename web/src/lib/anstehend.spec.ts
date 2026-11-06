import { describe, expect, it } from 'vitest';
import {
	artUmschalten,
	filterAus,
	fristDringend,
	fristWorte,
	gefiltert,
	jetztFuerDich,
	auswerten,
	bandMeldungen,
	meldungsOrt,
	nachPrioritaet,
	titelMitZahl,
	nachSchwerpunkt,
	sucheAus,
	vermerke,
	wartetAufDich
} from './anstehend.ts';
import { beispielListe, eintrag } from './anstehend.beispiel.ts';

describe('Filter in der Adresse', () => {
	it('liest Ansicht, Arten und „nur“ und schreibt nur Abweichungen zurück', () => {
		const f = filterAus(new URLSearchParams('ansicht=schwerpunkt&art=frage,aufgabe&nur=1'));
		expect(f).toEqual({ ansicht: 'schwerpunkt', arten: ['aufgabe', 'frage'], nur: true });
		expect(sucheAus(f)).toBe('?ansicht=schwerpunkt&art=aufgabe,frage&nur=1');
		expect(sucheAus(filterAus(new URLSearchParams('')))).toBe('');
	});
	it('ignoriert Unbekanntes und macht aus allen vier Arten „alle“', () => {
		expect(filterAus(new URLSearchParams('ansicht=quer&art=notiz,x&nur=ja'))).toEqual({
			ansicht: 'prioritaet',
			arten: [],
			nur: false
		});
		expect(filterAus(new URLSearchParams('art=aufgabe,frage,arbeit,meldung')).arten).toEqual([]);
		const f = artUmschalten(
			{ ansicht: 'prioritaet', arten: ['aufgabe', 'frage', 'meldung'], nur: false },
			'arbeit'
		);
		expect(f.arten).toEqual([]);
		expect(artUmschalten(f, 'frage').arten).toEqual(['frage']);
	});
	it('filtert nach Art und „nur was auf mich wartet“ wie der Server', () => {
		const l = beispielListe().eintraege;
		const nur = gefiltert(l, { ansicht: 'prioritaet', arten: [], nur: true });
		expect(nur.map((e) => e.id)).toEqual(['a-1', 'schlusssatz', 'ar-2']);
		const mitVerweis = eintrag({
			art: 'arbeit',
			wartet_auf: 'inhaber',
			vermerk: 'wartet auf deine Antwort'
		});
		expect(wartetAufDich(mitVerweis)).toBe(false);
		expect(gefiltert(l, { ansicht: 'prioritaet', arten: ['arbeit'], nur: false })).toHaveLength(2);
	});
});

describe('Gruppen', () => {
	it('nach Priorität: drei Gruppen, darin nach Frist (ohne Frist zuletzt)', () => {
		const ohne = eintrag({ id: 'a-9', stufe: 'jetzt' });
		const g = nachPrioritaet([ohne, ...beispielListe().eintraege]);
		expect(g.map((x) => x.stufe)).toEqual(['jetzt', 'bald', 'irgendwann']);
		expect(g[0]?.eintraege.map((e) => e.id)).toEqual(['schlusssatz', 'a-1', 'a-9']);
		expect(g[1]?.eintraege.map((e) => e.id)).toEqual(['ar-2']);
		expect(g[2]?.eintraege.map((e) => e.id)).toEqual(['ar-1']);
	});
	it('nach Schwerpunkt: nach dessen reihenfolge (Fakt je Eintrag), Projekte darunter, darin nach Stufe', () => {
		const g = nachSchwerpunkt(auswerten(beispielListe()).eintraege);
		expect(g.map((x) => x.name)).toEqual(['Chor und Verein', 'Bienen und Honig']);
		expect(g[0]?.projekte.map((p) => p.titel)).toEqual(['Beispiel']);
		expect(g[0]?.projekte[0]?.eintraege.map((e) => e.id)).toEqual(['schlusssatz', 'ar-2']);
		expect(g[1]?.projekte[0]?.eintraege.map((e) => e.id)).toEqual(['a-1', 'ar-1']);
	});
});

describe('Worte', () => {
	it('Frist und Vermerke', () => {
		expect([-2, 0, 1, 5, null].map(fristWorte)).toEqual([
			'überfällig',
			'heute',
			'morgen',
			'in 5 Tagen',
			null
		]);
		const e = eintrag({ art: 'frage', id: 'f' });
		e.stufe = { vorschlag: 'bald', inhaber: null, wirksam: 'jetzt', grund: 'frist' };
		expect(vermerke(e)).toEqual(['wartet auf deine Antwort', 'Frist']);
	});
});

function meldung(
	id: string,
	art: 'fehler' | 'frist' | 'fertig' | 'info',
	zuletzt: string,
	system = false
): ReturnType<typeof eintrag> {
	const stufe = art === 'fertig' ? 'bald' : art === 'info' ? 'irgendwann' : 'jetzt';
	const e = eintrag({
		art: 'meldung',
		id,
		titel: `Meldung ${id}`,
		zustand: 'ungelesen',
		wartet_auf: null,
		ziel: system ? `/pc#meldung-${id}` : `/projects/dashboard#meldung-${id}`,
		meldung: { art, anzahl: 1, zuletzt, text: null, system },
		erstellt: zuletzt
	});
	e.stufe = { vorschlag: stufe, inhaber: null, wirksam: stufe, grund: 'fest' };
	return e;
}

describe('Meldungen (Etappe A3)', () => {
	it('das Band: nur ungelesene der Stufe Jetzt, neueste zuerst, höchstens drei', () => {
		const l = [
			meldung('m-1', 'fehler', '2026-10-29T08:00:00+01:00'),
			meldung('m-2', 'fertig', '2026-10-29T12:00:00+01:00'),
			meldung('m-3', 'frist', '2026-10-29T09:00:00+01:00'),
			meldung('m-4', 'fehler', '2026-10-29T11:00:00+01:00', true),
			meldung('m-5', 'info', '2026-10-29T13:00:00+01:00'),
			meldung('m-6', 'frist', '2026-10-29T10:00:00+01:00'),
			eintrag({ id: 'a-1', stufe: 'jetzt' })
		];
		const b = bandMeldungen(l);
		expect(b.vorn.map((e) => e.id)).toEqual(['m-4', 'm-6', 'm-3']);
		expect(b.alle.map((e) => e.id)).toEqual(['m-4', 'm-6', 'm-3', 'm-1']);
		const [erste, zweite] = b.vorn;
		if (erste === undefined || zweite === undefined) throw new Error('Band zu kurz');
		expect(meldungsOrt(erste)).toBe('PC');
		expect(meldungsOrt(zweite)).toBe('Dashboard');
		expect(bandMeldungen([]).vorn).toEqual([]);
	});

	it('Vermerke einer Meldung: PC und wie oft, keine Stufen-Vermerke', () => {
		const m = meldung('m-1', 'fehler', '2026-10-29T08:00:00+01:00', true);
		expect(vermerke(m)).toEqual(['PC']);
		if (m.meldung) m.meldung.anzahl = 3;
		expect(vermerke(m)).toEqual(['PC', '3-mal']);
		expect(wartetAufDich(m)).toBe(false);
	});

	it('Meldungen im Filter (?art=meldung) und im Tab-Titel', () => {
		const f = filterAus(new URLSearchParams('art=meldung'));
		expect(f.arten).toEqual(['meldung']);
		expect(sucheAus(f)).toBe('?art=meldung');
		const l = [meldung('m-1', 'fehler', '2026-10-29T08:00:00+01:00'), eintrag({ id: 'a-1' })];
		expect(gefiltert(l, f).map((e) => e.id)).toEqual(['m-1']);
		expect(titelMitZahl('Anstehend', 2)).toBe('(2) Anstehend · Dashboard');
		expect(titelMitZahl('PC', 0)).toBe('PC · Dashboard');
	});
});

describe('Jetzt für dich', () => {
	it('je Projekt die aktuelle Frage und Arbeiten unter Jetzt, die auf dich warten; ohne Aufgaben und Meldungen', () => {
		const frageJetzt = eintrag({
			art: 'frage',
			id: 'farbe',
			projekt: 'schaufenster',
			projekt_titel: 'Schaufenster',
			stufe: 'jetzt',
			vermerk: 'noch 2 Fragen warten'
		});
		// Etappe 3b-2: Fragen aus Bald und Irgendwann stehen nicht in „Jetzt für dich“.
		const frageBald = eintrag({
			art: 'frage',
			id: 'logo',
			projekt: 'dashboard',
			stufe: 'bald'
		});
		const frageIrgendwann = eintrag({
			art: 'frage',
			id: 'notiz',
			projekt: 'notizen',
			projekt_titel: 'Notizen',
			stufe: 'irgendwann'
		});
		const arbeitJetzt = eintrag({
			art: 'arbeit',
			id: 'ar-9',
			titel: 'Bau reparieren',
			stufe: 'jetzt',
			zustand: 'wartet',
			wartet_auf: 'inhaber'
		});
		// Läuft nur, wartet nicht auf dich: gehört nicht in „Jetzt für dich“ (Auflage aus A1).
		const arbeitLaeuft = eintrag({ art: 'arbeit', id: 'ar-7', stufe: 'jetzt' });
		const meldungJetzt = meldung('m-1', 'fehler', '2026-10-29T10:00:00+01:00');
		const arbeitBald = eintrag({ art: 'arbeit', id: 'ar-8', stufe: 'bald' });
		const aufgabeJetzt = eintrag({ id: 'a-9', stufe: 'jetzt' });
		const p = jetztFuerDich([
			aufgabeJetzt,
			meldungJetzt,
			arbeitLaeuft,
			arbeitJetzt,
			frageBald,
			frageJetzt,
			frageIrgendwann,
			arbeitBald
		]);
		expect(
			p.map((x) => [x.projekt, x.titel, x.frage?.id ?? null, x.jetzt.map((e) => e.id)])
		).toEqual([
			['dashboard', 'Dashboard', null, ['ar-9']],
			['schaufenster', 'Schaufenster', 'farbe', []]
		]);
		// Aus der Beispiel-Liste: die Frage von beispiel (jetzt), sonst nichts (Aufgabe, Arbeiten nicht jetzt).
		expect(jetztFuerDich(beispielListe().eintraege).map((x) => x.projekt)).toEqual(['beispiel']);
		expect(jetztFuerDich([])).toEqual([]);
	});
});

describe('Frist-Stufe (Etappe 3a-4)', () => {
	it('hervorgehoben nur nach der Stufe des Servers, nicht nach den Tagen', () => {
		expect(fristDringend('ueberfaellig')).toBe(true);
		expect(fristDringend('heute')).toBe(true);
		expect(fristDringend('bald')).toBe(false);
		expect(fristDringend('spaeter')).toBe(false);
		expect(fristDringend(null)).toBe(false);
		// Tage 0, aber der Server sagt bald (etwa kurz vor Mitternacht geladen): nicht hervorgehoben.
		const e = eintrag({ frist: '2026-10-30', tage_bis: 0, frist_stufe: 'bald' });
		expect(fristDringend(e.frist_stufe)).toBe(false);
	});
});
