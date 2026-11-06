import { describe, expect, it } from 'vitest';
import type {
	BackupLage,
	Belegung,
	DefenderBefund,
	DefenderGrund,
	DefenderStatus,
	KopieStand,
	PcBenutzer,
	SchluesselGrund,
	SchluesselStand
} from '$vertrag';
import { ApiError } from './api/client.ts';
import {
	backupKopien,
	backupVerdict,
	belegungBedeutung,
	benutzerdaten,
	benutzerHinweis,
	cpuVerlauf,
	defenderVerdict,
	keysVerdict,
	ladeBenutzer,
	percent,
	rebootVerdict,
	schluesselAusLadung,
	stepsToColumns
} from './pc';

describe('pc', () => {
	it('Backup: Ampel und Satz aus regel_321 und den Kopien', () => {
		const lage = (
			regel_321: BackupLage['regel_321'],
			medium: KopieStand,
			cloud: KopieStand
		): BackupLage => ({
			kopien: [
				{ ort: 'pc', stand: 'vorhanden', text: null },
				{ ort: 'zweites-medium', stand: medium, text: null },
				{ ort: 'anderer-ort', stand: cloud, text: null }
			],
			regel_321
		});
		expect(backupVerdict(lage('nicht-erfuellt', 'fehlt', 'fehlt'))).toEqual({
			bedeutung: 'problem',
			text: 'keine Sicherung'
		});
		expect(backupVerdict(lage('teilweise', 'fehlt', 'teilweise'))).toEqual({
			bedeutung: 'wartet',
			text: 'eine Teilkopie in der Cloud, Platte fehlt'
		});
		expect(backupVerdict(lage('teilweise', 'fehlt', 'vorhanden'))).toEqual({
			bedeutung: 'wartet',
			text: 'Kopie in der Cloud, Platte fehlt'
		});
		expect(backupVerdict(lage('teilweise', 'vorhanden', 'teilweise')).text).toBe(
			'Platte da, die Cloud nur teilweise'
		);
		expect(backupVerdict(lage('teilweise', 'vorhanden', 'fehlt')).text).toBe(
			'zwei Kopien, aber keine außer Haus'
		);
		expect(backupVerdict(lage('erfuellt', 'vorhanden', 'vorhanden'))).toEqual({
			bedeutung: 'ok',
			text: 'gut gesichert'
		});
		expect(backupVerdict(null)).toEqual({ bedeutung: 'neutral', text: 'Stand unbekannt' });
	});
	it('Backup: Kopien in fester Reihenfolge mit Titel, fehlender Ort gilt als fehlt', () => {
		const k = backupKopien({
			kopien: [
				{ ort: 'anderer-ort', stand: 'teilweise', text: 'Cloud-Speicher' },
				{ ort: 'pc', stand: 'vorhanden', text: null }
			],
			regel_321: 'teilweise'
		});
		expect(k.map((x) => [x.titel, x.stand, x.text])).toEqual([
			['Kopie 1: dieser PC', 'vorhanden', ''],
			['Kopie 2: externe Platte', 'fehlt', ''],
			['Kopie 3: anderer Ort', 'teilweise', 'Cloud-Speicher']
		]);
	});
	// Etappe 3a-4: Die Regel rechnet der Server (befund, gruende); hier nur Farbe und Satz.
	const defender = (befund: DefenderBefund, gruende: DefenderGrund[]): DefenderStatus => ({
		checked_at: '2026-10-30T08:00:00+01:00',
		antivirus_enabled: true,
		realtime_enabled: true,
		signature_version: null,
		signature_updated: null,
		// Absichtlich alt: die Oberfläche rechnet nicht selbst nach.
		signature_age_days: 99,
		last_quick_scan: null,
		error: null,
		befund,
		gruende,
		signatur_schwelle_tage: 3
	});
	it('Defender: Farbe und Satz nur aus befund und gruende', () => {
		expect(defenderVerdict(defender('in-ordnung', []))).toEqual({
			bedeutung: 'ok',
			text: 'Defender aktuell'
		});
		expect(defenderVerdict(defender('ansehen', ['echtzeit-aus', 'signatur-alt']))).toEqual({
			bedeutung: 'problem',
			text: 'Defender: Echtzeitschutz aus, Signaturen veraltet (älter als 3 Tage)'
		});
	});
	it('Defender: unbekannt ist neutral mit Grund, nie wie in Ordnung', () => {
		const v = defenderVerdict(defender('unbekannt', ['noch-nicht-gelesen']));
		expect(v).toEqual({ bedeutung: 'neutral', text: 'Defender: wird gelesen' });
		expect(defenderVerdict(defender('unbekannt', ['abfrage-fehlgeschlagen'])).text).toBe(
			'Defender: Abfrage gescheitert'
		);
		expect(defenderVerdict(defender('unbekannt', ['signatur-alter-unbekannt']))).toEqual({
			bedeutung: 'neutral',
			text: 'Defender: Alter der Signaturen unbekannt'
		});
		// Ein Grund, den die Oberfläche nicht kennt, bleibt stehen.
		const neu = defender('unbekannt', ['neu-im-server' as DefenderGrund]);
		expect(defenderVerdict(neu)).toEqual({ bedeutung: 'neutral', text: 'Defender: neu-im-server' });
		// Ein unbekannter befund-Wert ist nie grün.
		expect(defenderVerdict(defender('spaeter' as DefenderBefund, [])).bedeutung).toBe('neutral');
	});
	it('Neustart: Farbe und Satz nur aus neustart und gruende', () => {
		expect(rebootVerdict({ neustart: 'nicht-noetig', gruende: [] })).toEqual({
			bedeutung: 'ok',
			text: 'kein Neustart nötig'
		});
		expect(
			rebootVerdict({ neustart: 'aussteht', gruende: ['windows-update', 'dateien-warten'] })
		).toEqual({
			bedeutung: 'wartet',
			text: 'Neustart steht aus (Windows-Update, Dateien warten auf Austausch)'
		});
	});
	it('Neustart: unbekannt ist neutral mit Grund, nie wie in Ordnung', () => {
		expect(rebootVerdict({ neustart: 'unbekannt', gruende: [] })).toEqual({
			bedeutung: 'neutral',
			text: 'Neustart: unbekannt (nicht lesbar)'
		});
	});
	it('Schlüssel: unbekannt ist neutral mit Grund, nie wie in Ordnung', () => {
		const unbekannt = (grund: SchluesselGrund | null): SchluesselStand => ({
			vollstaendig: 'unbekannt',
			gespeichert: 0,
			fehlen: 0,
			grund
		});
		expect(keysVerdict(unbekannt('bote-fehlt'))).toEqual({
			bedeutung: 'neutral',
			text: 'Schlüssel: Stand unbekannt (Bote läuft nicht)'
		});
		expect(keysVerdict(unbekannt('liste-unlesbar')).text).toBe(
			'Schlüssel: Stand unbekannt (Liste nicht lesbar)'
		);
		expect(keysVerdict(unbekannt(null))).toEqual({
			bedeutung: 'neutral',
			text: 'Schlüssel: Stand unbekannt'
		});
	});
	it('Belegung: Farbe nur aus der Stufe des Servers; unbekannt neutral', () => {
		expect(belegungBedeutung('normal')).toBe('ok');
		expect(belegungBedeutung('hoch')).toBe('wartet');
		expect(belegungBedeutung('kritisch')).toBe('problem');
		expect(belegungBedeutung('unbekannt')).toBe('neutral');
		expect(belegungBedeutung('neu' as Belegung)).toBe('neutral');
	});
	it('zählt Schlüssel und ordnet Schritte', () => {
		expect(keysVerdict({ vollstaendig: 'nein', gespeichert: 0, fehlen: 1, grund: null })).toEqual({
			bedeutung: 'problem',
			text: 'Schlüssel: 0 gespeichert, 1 fehlt'
		});
		expect(keysVerdict({ vollstaendig: 'ja', gespeichert: 2, fehlen: 0, grund: null })).toEqual({
			bedeutung: 'ok',
			text: 'Schlüssel: alle 2 gespeichert'
		});
		const cols = stepsToColumns([
			{ title: 'A', status: 'fertig', when: '', note: '' },
			{ title: 'B', status: 'wartet', when: 'bald', note: 'n' },
			{ title: 'C', status: 'komisch', when: '', note: '' }
		]);
		expect(cols.map((c) => c.title)).toEqual(['wartet auf dich', 'offen', 'fertig']);
		expect(cols[0]?.items).toEqual(['B (bald) · n']);
		expect(cols[1]?.items).toEqual(['C']);
		expect(percent(50, 200)).toBe(25);
		expect(percent(5, 0)).toBe(0);
	});
});

const benutzer: PcBenutzer = {
	inventory: {
		root: '',
		computed_at: null,
		duration_secs: 0,
		entries: [],
		face: [],
		hinweis: null
	},
	sysapps: { autostart: [], tools: [], winget: [], winget_error: null },
	schluessel: { vollstaendig: 'ja', gespeichert: 1, fehlen: 0, grund: null },
	bote: 'da',
	stand: null
};

function antwort(status: number, body: unknown): typeof fetch {
	return () =>
		Promise.resolve(
			new Response(JSON.stringify(body), {
				status,
				headers: { 'content-type': 'application/json' }
			})
		);
}

describe('Benutzer-Daten (7b)', () => {
	it('Stand aus der Ladung; ein unbekannter bote-Wert gilt als da', () => {
		expect(benutzerdaten({ benutzer, grund: null })).toBe('da');
		expect(benutzerdaten({ benutzer: { ...benutzer, bote: 'bote-fehlt' }, grund: null })).toBe(
			'bote-fehlt'
		);
		const unsinn = { ...benutzer, bote: 'unsinn' } as unknown as PcBenutzer;
		expect(benutzerdaten({ benutzer: unsinn, grund: null })).toBe('da');
		expect(benutzerdaten({ benutzer: null, grund: 'anmeldung' })).toBe('anmeldung-noetig');
		expect(benutzerdaten({ benutzer: null, grund: 'fremdes-konto' })).toBe('fremdes-konto');
	});
	it('ladeBenutzer: 200 Daten, 401 und 403 null mit Grund, sonst Fehler', async () => {
		expect(await ladeBenutzer(antwort(200, benutzer))).toEqual({ benutzer, grund: null });
		const ohne = antwort(401, {
			fehler: 'Bitte mit dem Windows-Konto anmelden.',
			art: 'anmeldung'
		});
		expect(await ladeBenutzer(ohne)).toEqual({ benutzer: null, grund: 'anmeldung' });
		const fremd = antwort(403, {
			fehler: 'Dieses Windows-Konto darf hier nicht.',
			art: 'fremdes-konto'
		});
		expect(await ladeBenutzer(fremd)).toEqual({ benutzer: null, grund: 'fremdes-konto' });
		await expect(ladeBenutzer(antwort(500, { fehler: 'Kaputt.' }))).rejects.toBeInstanceOf(
			ApiError
		);
	});
	it('Hinweise und Schlüssel-Kachel', () => {
		expect(benutzerHinweis('da', null)).toBeNull();
		expect(benutzerHinweis('anmeldung-noetig', '2026-10-27T08:00:00Z')).toBe(
			'Diese Werte zeigt das Dashboard nur nach Windows-Anmeldung (über localhost).'
		);
		expect(benutzerHinweis('fremdes-konto', null)).toBe('Dieses Windows-Konto darf hier nicht.');
		expect(keysVerdict(schluesselAusLadung({ benutzer: null, grund: 'fremdes-konto' }))).toEqual({
			bedeutung: 'neutral',
			text: 'Schlüssel: nicht für dieses Windows-Konto'
		});
		expect(benutzerHinweis('bote-fehlt', '2026-10-27T08:00:00Z')).toMatch(
			/^Der Bote läuft nicht; letzter Stand \d\d\.10\.2026, \d\d:\d\d\.$/
		);
		expect(benutzerHinweis('bote-fehlt', null)).toBe(
			'Der Bote läuft nicht; einen letzten Stand gibt es noch nicht.'
		);
		expect(schluesselAusLadung({ benutzer: null, grund: 'anmeldung' })).toEqual({
			vollstaendig: 'unbekannt',
			gespeichert: 0,
			fehlen: 0,
			grund: 'anmeldung'
		});
		expect(keysVerdict(schluesselAusLadung({ benutzer: null, grund: 'anmeldung' }))).toEqual({
			bedeutung: 'neutral',
			text: 'Schlüssel: nur nach Windows-Anmeldung'
		});
		expect(schluesselAusLadung({ benutzer, grund: null })).toBe(benutzer.schluessel);
	});
});

describe('Prozessor-Kurve (Etappe 3a+)', () => {
	it('lässt Messungen ohne Wert aus statt 0 zu zeigen', () => {
		expect(
			cpuVerlauf([{ cpu: null }, { cpu: 12 }, { cpu: null }, { cpu: 0 }, { cpu: 7.5 }])
		).toEqual([12, 0, 7.5]);
		expect(cpuVerlauf([])).toEqual([]);
	});
	it('Schlüssel: Tresor-Fehler ist unbekannt (neutral), nie rot', () => {
		const v = keysVerdict({
			vollstaendig: 'unbekannt',
			gespeichert: 0,
			fehlen: 0,
			grund: 'tresor-fehler'
		});
		expect(v.bedeutung).toBe('neutral');
		expect(v.text).toBe('Schlüssel: Stand unbekannt (Tresor meldet einen Fehler)');
	});
});
