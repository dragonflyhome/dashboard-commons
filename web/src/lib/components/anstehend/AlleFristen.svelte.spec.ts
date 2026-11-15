/**
 * „Alle Fristen“ hält `/api/fristen` (geschützt, Titel aller Projekte). Nach
 * dem Abmelden (auch Widerruf, Ablauf) lädt der Abschnitt neu und verschwindet
 * ohne Anmeldung. Alle Beispiele sind erfunden.
 */
import { render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { FristenListe } from '$vertrag';

// Der freie Kanal als Platzhalter: die Hörer von `onAbgemeldet` stehen in `abgemeldet`.
const { abgemeldet } = vi.hoisted(() => ({ abgemeldet: new Set<() => void>() }));
vi.mock('#lib/freikanal.svelte.ts', () => ({
	freierKanal: {
		onAbgemeldet: (l: () => void) => {
			abgemeldet.add(l);
			return () => abgemeldet.delete(l);
		}
	}
}));
const { default: AlleFristen } = await import('./AlleFristen.svelte');

const LISTE: FristenListe = {
	heute: '2026-11-04',
	frist_bald_tage: 7,
	zeilen: [
		{
			quelle: 'datei',
			projekt: 'antrag',
			projekt_titel: 'Antrag',
			id: 'plakate',
			titel: 'Plakate bestellen',
			frist: '2026-11-03',
			tage_bis: -1,
			frist_stufe: 'ueberfaellig',
			frist_bezug: null,
			ereignis: null,
			hinweis: null,
			ziel: '/projects/antrag#frist-plakate'
		}
	],
	hinweise: [],
	speicher_fehlt: false
};

describe('„Alle Fristen“ nach dem Abmelden', () => {
	it('lädt neu und verschwindet; nach dem Schließen kein Hörer mehr', async () => {
		let angemeldet = true;
		const fetchFn = vi.fn(() =>
			Promise.resolve(
				new Response(JSON.stringify(angemeldet ? LISTE : { fehler: 'x', art: 'anmeldung' }), {
					status: angemeldet ? 200 : 401,
					headers: { 'content-type': 'application/json' }
				})
			)
		);
		const r = render(AlleFristen, { fetchFn });
		expect(await screen.findByRole('heading', { name: /Alle Fristen/ })).toBeInTheDocument();
		expect(screen.getByText(/Plakate bestellen/)).toBeInTheDocument();
		expect(abgemeldet.size).toBe(1);
		angemeldet = false;
		for (const l of abgemeldet) l();
		await waitFor(() => {
			expect(screen.queryByRole('heading', { name: /Alle Fristen/ })).toBeNull();
		});
		expect(screen.queryByText(/Plakate bestellen/)).toBeNull();
		expect(fetchFn).toHaveBeenCalledTimes(2);
		r.unmount();
		expect(abgemeldet.size).toBe(0);
	});
});
