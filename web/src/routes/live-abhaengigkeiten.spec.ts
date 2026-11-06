import { describe, expect, it } from 'vitest';
import { betroffen, bereichBetroffen, vorgangBetroffen } from '#lib/live.svelte.ts';
import { load as start } from './+page.ts';
import { load as pc } from './pc/+page.ts';
import { load as daten } from './daten/+page.ts';
import { load as entscheidungen } from './entscheidungen/+page.ts';
import { load as zielbild } from './zielbild/+page.ts';
import { load as schluessel } from './schluessel/+page.ts';
import { load as vorgaenge } from './vorgaenge/+page.ts';
import { load as hallo } from './hallo/+page.ts';

// Live-Lücken: Für jede Seite mit Daten gibt es ein Ereignis, dessen
// `invalidate` ihre Ladefunktion trifft. Die Kette Server → Ereignis →
// invalidate → load ist damit für jede Seite geschlossen (das Layout ruft
// invalidate mit genau diesen Funktionen, siehe layout.svelte.spec.ts).

type Laden = (e: never) => unknown;

/**
 * Die `depends`-Schlüssel einer Ladefunktion (das Netz antwortet mit Fehlern,
 * außer den Antworten in `antworten`, je Weg).
 */
async function schluesselVon(
	load: Laden,
	antworten: Record<string, unknown> = {}
): Promise<string[]> {
	const keys: string[] = [];
	const event = {
		fetch: (weg: string) =>
			Promise.resolve(
				weg in antworten
					? new Response(JSON.stringify(antworten[weg]), { status: 200 })
					: new Response('{}', { status: 500 })
			),
		depends: (...k: string[]) => keys.push(...k),
		params: {},
		url: new URL('http://localhost:8787/'),
		parent: () => Promise.resolve({})
	};
	try {
		await load(event as never);
	} catch {
		// Fehlerseite: zählt hier nicht, nur die Schlüssel.
	}
	return keys;
}

const trifft = (fn: (u: URL) => boolean, keys: string[]) => keys.some((k) => fn(new URL(k)));

describe('jede Seite hat ein Ereignis, das sie neu lädt', () => {
	it('/daten: Datenspeicher (Aufgaben, PC-Messung, Sicherung …)', async () => {
		const k = await schluesselVon(daten as Laden);
		for (const b of ['aufgaben', 'arbeiten', 'meldungen', 'pc-messung', 'sicherung'] as const) {
			expect(trifft(bereichBetroffen(b), k), b).toBe(true);
		}
	});
	it('/entscheidungen und /zielbild: docs', async () => {
		expect(
			trifft(bereichBetroffen('entscheidungen'), await schluesselVon(entscheidungen as Laden))
		).toBe(true);
		expect(trifft(bereichBetroffen('zielbild'), await schluesselVon(zielbild as Laden))).toBe(true);
		expect(trifft(bereichBetroffen('zielbild'), await schluesselVon(entscheidungen as Laden))).toBe(
			false
		);
	});
	it('/pc und /: Benutzer-Daten und Verlauf mit eigenen Schlüsseln', async () => {
		for (const load of [pc, start]) {
			const k = await schluesselVon(load as Laden);
			expect(k).toContain('api:pc:benutzer');
			expect(k).toContain('api:verlauf');
			for (const b of [
				'inventar',
				'systemprogramme',
				'tresor',
				'bote',
				'windows-ereignisse',
				'meldungen'
			] as const) {
				expect(trifft(bereichBetroffen(b), k), b).toBe(true);
			}
		}
	});
	it('/schluessel: Tresor und Bote', async () => {
		const k = await schluesselVon(schluessel as Laden);
		expect(trifft(bereichBetroffen('tresor'), k)).toBe(true);
		expect(trifft(bereichBetroffen('bote'), k)).toBe(true);
	});
	it('/vorgaenge: das Ereignis vorgang, zur Sicherung auch speicher/vorgaenge', async () => {
		const k = await schluesselVon(vorgaenge as Laden);
		expect(trifft(vorgangBetroffen(), k)).toBe(true);
		expect(trifft(bereichBetroffen('vorgaenge'), k)).toBe(true);
		expect(trifft(bereichBetroffen('aufgaben'), k)).toBe(false);
	});
	it('/hallo: nur ein projekt-Ereignis zu dem Projekt, das der Server nennt', async () => {
		const k = await schluesselVon(hallo as Laden, {
			'/api/hallo': { projekt: 'beispiel-gruss', daten: 'eigen-hallo' }
		});
		const p = (slug: string) =>
			betroffen({ projekte: [slug], daten: [], version: 1, sammel: false });
		expect(trifft(p('beispiel-gruss'), k)).toBe(true);
		expect(trifft(p('anderes-projekt'), k)).toBe(false);
	});
});
