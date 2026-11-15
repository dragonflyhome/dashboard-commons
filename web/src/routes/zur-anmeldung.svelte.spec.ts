/**
 * Schritt E2: die eine Stelle für die Anmeldung (`+layout.ts`). Ohne
 * Anmeldung (`zugang` „fehlt“ oder „fremd“) leitet jede Seite außer
 * `OEFFENTLICHE_SEITEN` nach `/anmelden?ziel=<pfad>`, bevor sie lädt. Die
 * Seiten liest der Test aus `src/routes` (jede `+page.svelte`): Eine neue
 * Seite ist damit von selbst geschützt und hier geprüft.
 */
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { isRedirect } from '@sveltejs/kit';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { OEFFENTLICHE_SEITEN, anmeldenAdresse, zugang, type Zugangsstand } from '#lib/zugang.ts';
import { zielPruefen } from '#lib/passkey.ts';
import { load } from './+layout.ts';

const ROUTEN = import.meta.dirname;

/** Jede Seite als Pfad; Parameter wie `[slug]` werden zu `beispiel`. */
function seiten(ordner = ROUTEN): string[] {
	const aus: string[] = [];
	for (const e of readdirSync(ordner, { withFileTypes: true })) {
		const voll = join(ordner, e.name);
		if (e.isDirectory()) aus.push(...seiten(voll));
		else if (e.name === '+page.svelte') {
			const rel = relative(ROUTEN, ordner).split(sep).join('/');
			aus.push(`/${rel.replace(/\[[^\]]+\]/g, 'beispiel')}`);
		}
	}
	return aus.sort();
}

const LISTE = { projects: [], gruppen: [] };

/** `fetch` für `/api/projects` mit `status`; andere Adressen sind ein Fehler im Test. */
function projekte(status: number, body: unknown = LISTE) {
	return vi.fn<typeof fetch>((pfad) => {
		if (pfad !== '/api/projects') throw new Error('unerwartete Adresse');
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status,
				headers: { 'content-type': 'application/json' }
			})
		);
	});
}

/** Ruft `load` des Layouts für `adresse`; gibt die Daten oder das Ziel der Umleitung. */
async function lade(
	adresse: string,
	f: typeof fetch = projekte(200)
): Promise<{ ziel: string } | { daten: unknown }> {
	try {
		const daten = await load({
			url: new URL(adresse, 'http://localhost:8787'),
			fetch: f,
			depends: () => undefined
		} as never);
		return { daten };
	} catch (e) {
		if (isRedirect(e)) {
			expect(e.status).toBe(307);
			return { ziel: e.location };
		}
		throw e;
	}
}

afterEach(() => {
	zugang.vergessen();
});

describe('Ohne Anmeldung: jede Seite leitet auf /anmelden (Schritt E2)', () => {
	const alle = seiten();

	it('findet die Seiten (der Test liest wirklich src/routes)', () => {
		expect(alle).toEqual(expect.arrayContaining(['/', '/anmelden', '/fassungen', '/pc']));
		expect(alle).toContain('/projects/beispiel/beispiel');
		expect(alle.length).toBeGreaterThan(15);
	});

	it.each(['fehlt', 'fremd'] as const)(
		'zugang %s: jede nicht öffentliche Seite leitet weiter, ohne /api/projects zu fragen',
		async (stand: Zugangsstand) => {
			for (const pfad of alle) {
				zugang.setzen(stand);
				const f = projekte(200);
				const r = await lade(`${pfad}?filter=a%20b`, f);
				if (OEFFENTLICHE_SEITEN.has(pfad)) {
					expect(r, pfad).toEqual({ daten: { leisteProjekte: null, projektListe: null } });
				} else {
					const ziel = `/anmelden?ziel=${encodeURIComponent(`${pfad}?filter=a%20b`)}&umgeleitet=1`;
					expect(r, pfad).toEqual({ ziel });
					// Das Ziel kommt auf /anmelden unverändert an (zielPruefen lässt es durch).
					const zurueck = new URL(ziel, 'http://localhost:8787').searchParams.get('ziel');
					expect(zielPruefen(zurueck)).toBe(`${pfad}?filter=a%20b`);
				}
				expect(f, pfad).not.toHaveBeenCalled();
			}
		}
	);

	it('unbekannte Wege (auch ein erfundenes Projekt) leiten genauso weiter', async () => {
		zugang.setzen('fehlt');
		for (const pfad of ['/gibt-es-nicht', '/projects/gibt-es-nicht', '/projects/dashboard']) {
			expect(await lade(pfad)).toEqual({ ziel: anmeldenAdresse(new URL(pfad, 'http://x')) });
		}
	});

	it('angemeldet (da): keine Umleitung, die Projektliste für die Leiste', async () => {
		zugang.setzen('da');
		for (const pfad of ['/', '/pc', '/anmelden']) {
			expect(await lade(pfad)).toEqual({
				daten: { leisteProjekte: [], projektListe: LISTE }
			});
		}
	});

	it('Anmeldestand unbekannt, /api/projects sagt 401: gilt als „fehlt“ und leitet weiter', async () => {
		const r = await lade(
			'/pc',
			projekte(401, { fehler: 'Bitte mit dem Windows-Konto anmelden.', art: 'anmeldung' })
		);
		expect(r).toEqual({ ziel: '/anmelden?ziel=%2Fpc&umgeleitet=1' });
		expect(zugang.stand).toBe('fehlt');
	});

	it('Anmeldestand unbekannt, /api/projects sagt „fremdes Konto“: gilt als „fremd“ und leitet weiter', async () => {
		const r = await lade('/pc', projekte(403, { fehler: 'x', art: 'fremdes-konto' }));
		expect(r).toEqual({ ziel: '/anmelden?ziel=%2Fpc&umgeleitet=1' });
		expect(zugang.stand).toBe('fremd');
	});

	it('401 auf einer öffentlichen Seite: keine Umleitung (keine Schleife auf /anmelden)', async () => {
		const r = await lade('/anmelden', projekte(401, { fehler: 'x', art: 'anmeldung' }));
		expect(r).toEqual({ daten: { leisteProjekte: null, projektListe: null } });
	});

	it('andere Fehler (Server startet neu): keine Umleitung, die Leiste ohne Projekte', async () => {
		expect(await lade('/pc', projekte(503, { fehler: 'x' }))).toEqual({
			daten: { leisteProjekte: null, projektListe: null }
		});
		expect(zugang.stand).toBe('unbekannt');
	});
});

describe('Öffentliche Seiten: Oberfläche und Server gleich', () => {
	const SICHERHEIT = readFileSync(join(ROUTEN, '..', '..', '..', 'src', 'sicherheit.rs'), 'utf8');

	it('jede öffentliche Seite der Oberfläche ist im Server öffentlich (sicherheit::oeffentlich)', () => {
		// Die Zeile mit den Seiten in `oeffentlich_roh` (GET und HEAD).
		const zeile = /^\s*("\/healthz"[^=]*)=> lesen,$/m.exec(SICHERHEIT)?.[1] ?? '';
		const server = [...zeile.matchAll(/"([^"]+)"/g)].map((m) => m[1] ?? '');
		expect(server).toContain('/');
		for (const s of OEFFENTLICHE_SEITEN) expect(server).toContain(s);
		// Umgekehrt: der Server lässt außer der Startseite (sie lädt die Oberfläche) und den
		// Umleitungen keine Seite frei, die hier umgeleitet würde.
		const seitenDesServers = server.filter((s) => !['/', '/healthz', '/neu', '/neu/'].includes(s));
		expect([...seitenDesServers].sort()).toEqual([...OEFFENTLICHE_SEITEN].sort());
	});
});
