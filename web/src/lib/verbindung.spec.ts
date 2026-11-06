import { describe, expect, it } from 'vitest';
import {
	LIVE_TEXT,
	anmeldewort,
	istLive,
	liveFarbe,
	liveName,
	liveZustand,
	schlossName,
	schlossOffen,
	schlossZiel,
	type LiveZustand
} from './verbindung.ts';

// Kopfleiste, Fassung 2: fünf Zustände, Schloss und Live-Link.
describe('fünf Live-Zustände', () => {
	it('Text wie vom Inhaber vorgegeben', () => {
		expect(Object.values(LIVE_TEXT)).toEqual([
			'live, angemeldet',
			'live, nicht angemeldet',
			'live, nicht registriert',
			'Verbindung nicht gefunden',
			'Verbindung unterbrochen'
		]);
	});
	it('aus Kanal und Wort des Servers', () => {
		expect(liveZustand('offen', 'angemeldet')).toBe('angemeldet');
		expect(liveZustand('offen', 'nicht_angemeldet')).toBe('nicht_angemeldet');
		expect(liveZustand('offen', 'nicht_registriert')).toBe('nicht_registriert');
		expect(liveZustand('nie', 'angemeldet')).toBe('nicht_gefunden');
		expect(liveZustand('weg', 'angemeldet')).toBe('unterbrochen');
		// Erster Versuch läuft: keiner der fünf (wäre geraten).
		expect(liveZustand('sucht', 'angemeldet')).toBeNull();
	});
	it('das Wort: eine geschützte 200 oder ein „angemeldet“ gewinnt, dann „nicht registriert“', () => {
		expect(anmeldewort(true, 'nicht_angemeldet')).toBe('angemeldet');
		expect(anmeldewort(false, 'nicht_angemeldet', 'angemeldet')).toBe('angemeldet');
		expect(anmeldewort(false, 'nicht_angemeldet', 'nicht_registriert')).toBe('nicht_registriert');
		expect(anmeldewort(false, 'nicht_registriert', null)).toBe('nicht_registriert');
		expect(anmeldewort(false, 'nicht_angemeldet', null)).toBe('nicht_angemeldet');
		// Noch kein Wort vom Server: nicht angemeldet, nie angemeldet geraten.
		expect(anmeldewort(false, null, undefined)).toBe('nicht_angemeldet');
	});
	it('live nur mit Verbindung; Farbe grün, gelb, rot', () => {
		const alle = Object.keys(LIVE_TEXT) as LiveZustand[];
		expect(alle.filter(istLive)).toEqual(['angemeldet', 'nicht_angemeldet', 'nicht_registriert']);
		expect(alle.map(liveFarbe)).toEqual(['ok', 'ok', 'ok', 'problem', 'wartet']);
	});
	it('Name des Live-Links für Vorlese-Programme', () => {
		expect(liveName('angemeldet')).toBe('Verbindung: live, angemeldet, Stand ansehen');
		expect(liveName('nicht_registriert')).toBe(
			'Verbindung: live, nicht registriert, Stand ansehen'
		);
		expect(liveName('unterbrochen')).toBe('Verbindung unterbrochen, Stand ansehen');
		// Der sichtbare Text steckt im Namen (WCAG 2.5.3).
		for (const z of Object.keys(LIVE_TEXT) as LiveZustand[]) {
			expect(liveName(z)).toContain(LIVE_TEXT[z]);
		}
	});
});

describe('Schloss', () => {
	it('offen nur angemeldet, sonst zu', () => {
		const alle = Object.keys(LIVE_TEXT) as LiveZustand[];
		expect(alle.filter(schlossOffen)).toEqual(['angemeldet']);
	});
	it('Name je Zustand', () => {
		expect(schlossName('angemeldet')).toBe('Angemeldet, Passkey verwalten');
		expect(schlossName('nicht_angemeldet')).toBe('Nicht angemeldet, mit Passkey anmelden');
		expect(schlossName('nicht_registriert')).toBe('Nicht registriert, Passkey beantragen');
		expect(schlossName('unterbrochen')).toBe('Nicht angemeldet, mit Passkey anmelden');
	});
	it('führt immer zur Passkey-Seite: anlegen, beantragen oder anmelden', () => {
		expect(schlossZiel('angemeldet')).toBe('/anmelden#passkey-anlegen');
		expect(schlossZiel('nicht_registriert')).toBe('/anmelden#passkey-anlegen');
		expect(schlossZiel('nicht_angemeldet')).toBe('/anmelden#mit-passkey-anmelden');
		expect(schlossZiel('nicht_gefunden')).toBe('/anmelden#mit-passkey-anmelden');
	});
	it('mit Rückweg nur für Pfade im Dashboard', () => {
		expect(schlossZiel('nicht_angemeldet', '/pc?x=1')).toBe(
			'/anmelden?ziel=%2Fpc%3Fx%3D1#mit-passkey-anmelden'
		);
		for (const fremd of ['//boese.example/', 'https://boese.example/', '/anmelden?ziel=/']) {
			expect(schlossZiel('angemeldet', fremd)).toBe('/anmelden#passkey-anlegen');
		}
	});
});
