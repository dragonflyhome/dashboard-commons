/**
 * Schritt E1: `/api/anstehend` und `/api/meldungen` kommen angemeldet im
 * Klartext. Nach dem Abmelden (auch Widerruf, Ablauf) lädt „Anstehend“ neu;
 * ohne Anmeldung bleibt weder die Liste noch der Teil „behoben“ der Glocke.
 */
import { describe, expect, it, vi } from 'vitest';
import type { Meldung } from '$vertrag';
import { beispielListe, eintrag, json } from './anstehend.beispiel.ts';

// Der freie Kanal als Platzhalter: die Hörer von `onAbgemeldet` stehen in `abgemeldet`.
const { abgemeldet } = vi.hoisted(() => ({ abgemeldet: new Set<() => void>() }));
vi.mock('./freikanal.svelte.ts', () => ({
	freierKanal: {
		onAbgemeldet: (l: () => void) => {
			abgemeldet.add(l);
			return () => abgemeldet.delete(l);
		}
	}
}));
const { Anstehend } = await import('./anstehend.svelte.ts');

const BEHOBEN = {
	id: 'm-1',
	titel: 'C:\\Users\\beispiel\\dev ist nicht lesbar',
	behoben: '2026-10-30T09:00:00+01:00',
	gelesen: null,
	archiviert: null,
	zuletzt: '2026-10-30T08:00:00+01:00'
} as Meldung;

describe('Anstehend nach dem Abmelden', () => {
	it('lädt neu: keine Liste, keine behobenen Meldungen; nach dem Schließen kein Hörer mehr', async () => {
		let angemeldet = true;
		const holen = vi.fn((url: string) => {
			if (!angemeldet) return Promise.resolve(json({ fehler: 'x', art: 'anmeldung' }, 401));
			return Promise.resolve(
				url.startsWith('/api/meldungen')
					? json({ meldungen: [BEHOBEN] })
					: json({ ...beispielListe(), eintraege: [eintrag({ id: 'a-1', titel: 'Geheim' })] })
			);
		}) as unknown as typeof fetch;
		const a = new Anstehend(holen);
		const ab = a.start();
		try {
			expect(abgemeldet.size).toBe(1);
			await vi.waitFor(() => {
				expect(a.stand).toBe('da');
			});
			await a.meldungenLaden();
			expect(a.liste?.eintraege.map((e) => e.titel)).toEqual(['Geheim']);
			expect(a.behoben.map((m) => m.titel)).toEqual([BEHOBEN.titel]);
			angemeldet = false;
			for (const l of abgemeldet) l();
			await vi.waitFor(() => {
				expect(a.stand).toBe('ohne-anmeldung');
			});
			await vi.waitFor(() => {
				expect(a.meldungenAlle).toEqual([]);
			});
			expect(a.liste).toBeNull();
			expect(a.behoben).toEqual([]);
		} finally {
			ab();
		}
		expect(abgemeldet.size).toBe(0);
	});
});
