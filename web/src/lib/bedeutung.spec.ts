import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	BEDEUTUNGEN,
	ausFortschrittStufe,
	ausStandStufe,
	ausStatus,
	klasse,
	wort
} from './bedeutung.ts';

describe('Bedeutungen', () => {
	it('haben je eine eigene Klasse', () => {
		const klassen = BEDEUTUNGEN.map(klasse);
		expect(klassen).toEqual(['b-ok', 'b-laeuft', 'b-wartet', 'b-problem', 'b-info', 'b-neutral']);
		expect(new Set(klassen).size).toBe(6);
	});
	it('haben ein Wort für Bildschirmleser', () => {
		expect(BEDEUTUNGEN.map(wort)).toEqual([
			'in Ordnung',
			'läuft',
			'wartet',
			'Problem',
			'Hinweis',
			'neutral'
		]);
	});
	it('haben je eine Klasse in app.css, die den Ton setzt', () => {
		const css = readFileSync(join(import.meta.dirname, '..', 'app.css'), 'utf8');
		for (const k of BEDEUTUNGEN.map(klasse)) {
			expect(css).toMatch(new RegExp(String.raw`\.${k} \{\r?\n\t--ton: var\(--[a-z]+\);`));
		}
		// Die alten Reihen st-… und c-… gibt es nicht mehr.
		expect(css).not.toMatch(/\.(st|c)-(ok|wait|bad|done|plan|info)\b/);
	});
	it('Etappe B1: Stand-Hinweis alt ist gelb, frisch ruhig', () => {
		expect(ausStandStufe('alt')).toBe('wartet');
		expect(ausStandStufe('frisch')).toBe('neutral');
		expect(ausStandStufe(undefined)).toBe('neutral');
	});
	it('Fortschritt: Farbe aus der Stufe des Servers', () => {
		// Etappe 3a+: die Stufe rechnet der Server (ProgressRow.stufe); hier nur die Farbe dazu.
		expect(ausFortschrittStufe('erreicht')).toBe('ok');
		expect(ausFortschrittStufe('ab-haelfte')).toBe('laeuft');
		expect(ausFortschrittStufe('unter-haelfte')).toBe('wartet');
		expect(ausFortschrittStufe(null)).toBe('neutral');
		expect(ausFortschrittStufe(undefined)).toBe('neutral');
	});
	it('leiten sich aus dem Status in projekt.toml ab wie beim Server', () => {
		expect(ausStatus('aktiv')).toBe('laeuft');
		expect(ausStatus('pausiert')).toBe('wartet');
		expect(ausStatus('geplant')).toBe('neutral');
		expect(ausStatus('fertig')).toBe('ok');
		expect(ausStatus('problem')).toBe('problem');
		expect(ausStatus('archiviert')).toBe('neutral');
	});
});
