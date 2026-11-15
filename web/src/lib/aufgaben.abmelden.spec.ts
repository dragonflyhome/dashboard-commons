/**
 * `/api/aufgaben` ist geschützt und liefert Titel und Links im Klartext. Nach
 * dem Abmelden (auch Widerruf, Ablauf) laden die Aufgaben neu; ohne Anmeldung
 * bleibt weder die Liste noch die eines archivierten Projekts stehen.
 */
import { describe, expect, it, vi } from 'vitest';
import type { AufgabeAnsicht } from '$vertrag';

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
const { Aufgaben } = await import('./aufgaben.svelte.ts');

function aufgabe(id: string, projekt: string): AufgabeAnsicht {
	return {
		id,
		projekt,
		von: 'chat:beispiel',
		titel: `Geheim ${id}`,
		knopf: null,
		frist: null,
		ablauf: null,
		zustand: 'offen',
		antwort: null,
		eigene: null,
		erstellt: '2026-10-28T08:00:00+01:00',
		geaendert: '2026-10-28T08:00:00+01:00',
		hat_link: true,
		vorgang: null,
		link: 'https://beispiel.invalid/geheim',
		frist_stufe: null
	};
}

const json = (body: unknown, status = 200): Response =>
	new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

describe('Aufgaben nach dem Abmelden', () => {
	it('lädt neu: keine Liste, kein Archiv, Zahl 0; nach dem Schließen kein Hörer mehr', async () => {
		let angemeldet = true;
		const holen = vi.fn((url: string) => {
			if (!angemeldet) return Promise.resolve(json({ fehler: 'x', art: 'anmeldung' }, 401));
			if (url.endsWith('/anzahl')) return Promise.resolve(json({ offen: 1 }));
			if (url.includes('projekt=stiller-umzug'))
				return Promise.resolve(json({ aufgaben: [aufgabe('a-2', 'stiller-umzug')] }));
			return Promise.resolve(json({ aufgaben: [aufgabe('a-1', 'beispiel')] }));
		}) as unknown as typeof fetch;
		const a = new Aufgaben(holen);
		const ab = a.start();
		const nutzung = a.benutzen();
		try {
			expect(abgemeldet.size).toBe(1);
			await a.ladenFuer('stiller-umzug');
			await vi.waitFor(() => {
				expect(a.stand).toBe('da');
			});
			await vi.waitFor(() => {
				expect(a.anzahl).toBe(1);
			});
			expect(a.fuer('beispiel').map((x) => x.titel)).toEqual(['Geheim a-1']);
			expect(a.fuer('stiller-umzug', true).map((x) => x.titel)).toEqual(['Geheim a-2']);
			angemeldet = false;
			for (const l of abgemeldet) l();
			await vi.waitFor(() => {
				expect(a.stand).toBe('ohne-anmeldung');
			});
			await vi.waitFor(() => {
				expect(a.anzahl).toBe(0);
			});
			expect(a.liste).toEqual([]);
			expect(a.fuer('stiller-umzug', true)).toEqual([]);
			expect(a.archiv).toEqual({});
		} finally {
			nutzung();
			ab();
		}
		expect(abgemeldet.size).toBe(0);
	});

	it('gilt noch eine Anmeldung, kommt das Archiv wieder', async () => {
		const holen = vi.fn((url: string) =>
			Promise.resolve(
				url.endsWith('/anzahl')
					? json({ offen: 0 })
					: json({ aufgaben: [aufgabe('a-2', 'stiller-umzug')] })
			)
		) as unknown as typeof fetch;
		const a = new Aufgaben(holen);
		const ab = a.start();
		try {
			await a.ladenFuer('stiller-umzug');
			for (const l of abgemeldet) l();
			await vi.waitFor(() => {
				expect(a.fuer('stiller-umzug', true).map((x) => x.id)).toEqual(['a-2']);
			});
		} finally {
			ab();
		}
	});
});
