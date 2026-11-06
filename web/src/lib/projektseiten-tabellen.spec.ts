import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { bereichBetroffen } from './live.svelte.ts';

// Abnahme live-luecken, Auflage „projekt-tabellen“: Das Ereignis
// `speicher`/`projekt-tabellen` trägt keinen Slug und lädt nur /daten
// (`api:daten`). Das reicht, solange keine Projektseite Daten aus einer
// Modul-Tabelle zeigt: Die Seiten lesen nur `daten/*.toml` über `$projekt`
// (ladeDaten, ladeKopf), und der Hook `nach_lesen` liest keine Tabelle. Holt
// eine Seite einmal selbst einen Endpunkt eines Moduls
// (`/api/projekte/<slug>/x/…`), fällt dieser Test auf: Dann bekommt das
// Ereignis den Slug und die Seite lädt gezielt nach (Anleitung, Live-Lücken).

const REPO = fileURLToPath(new URL('../../../', import.meta.url));

/**
 * Die Projektordner: die des Code-Repos (nach dem Umzug nur die Vorlage) und
 * die des Inhalts (`DASHBOARD_INHALT`, sonst `dashboard-inhalt` neben dem
 * Repo), wenn es ihn gibt. Kein Projektname steht hier.
 */
function projektOrdner(): string[] {
	const inhalt = process.env.DASHBOARD_INHALT ?? join(REPO, '..', 'dashboard-inhalt');
	return [join(REPO, 'projects'), join(inhalt, 'projects')].filter((o) => existsSync(o));
}

/** Seite.svelte und seiten/*.svelte aller Projekte, auch der Vorlage. */
function projektseiten(): string[] {
	const seiten: string[] = [];
	for (const ordner of projektOrdner()) {
		for (const slug of readdirSync(ordner)) {
			if (slug.startsWith('_') && slug !== '_vorlage') continue;
			const seite = join(ordner, slug, 'Seite.svelte');
			if (existsSync(seite)) seiten.push(seite);
			const unter = join(ordner, slug, 'seiten');
			if (existsSync(unter)) {
				for (const n of readdirSync(unter)) {
					if (n.endsWith('.svelte')) seiten.push(join(unter, n));
				}
			}
		}
	}
	return seiten;
}

describe('projekt-tabellen ohne Slug', () => {
	it('projektseiten_zeigen_keine_modul_tabellen', () => {
		const seiten = projektseiten();
		expect(seiten.length).toBeGreaterThan(0);
		const fremd = seiten.filter((p) => {
			const text = readFileSync(p, 'utf8');
			return text.includes('/x/') || /\bfetch\s*\(/.test(text);
		});
		expect(fremd, 'Projektseite holt einen Modul-Endpunkt: Ereignis mit Slug nötig').toEqual([]);
	});

	it('lädt nur /daten, keine Projektseite', () => {
		const f = bereichBetroffen('projekt-tabellen');
		expect(f(new URL('api:daten'))).toBe(true);
		expect(f(new URL('api:projekt:bauplan'))).toBe(false);
		expect(f(new URL('api:project:bauplan'))).toBe(false);
	});
});
