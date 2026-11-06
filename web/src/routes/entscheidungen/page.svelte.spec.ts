import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Entscheidung, EntscheidungsListe } from '$vertrag';
import { einzeilig, feldName, filtern, zeilen } from '#lib/entscheidungen.ts';
import { SATZ_ENTSCHEIDUNGEN_ANMELDEN } from '#lib/anmeldung.ts';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: Page } = await import('./+page.svelte');

const leer: Omit<Entscheidung, 'datei'> = {
	datum: null,
	thema: null,
	status: null,
	von: null,
	etappe: null,
	urteil: null,
	was: null,
	weitere: [],
	fehler: null
};

const liste: EntscheidungsListe = {
	eintraege: [
		{
			...leer,
			datei: '2026-10-31-plan-4.toml',
			datum: '2026-10-31',
			thema: 'Plan Etappe 4',
			status: 'geprüft',
			von: 'Architektur-Rolle',
			etappe: '4',
			urteil: 'Ja mit Ergänzungen',
			was: 'Erster Absatz.\n\nZweiter Absatz.',
			weitere: [
				{ name: 'warum', wert: { art: 'text', inhalt: 'Weil es einfacher ist.' } },
				{
					name: 'auflagen_spaeter',
					wert: {
						art: 'liste',
						inhalt: [
							{ art: 'text', inhalt: 'Erste Auflage' },
							{ art: 'text', inhalt: 'Zweite Auflage' }
						]
					}
				},
				{
					name: 'nachbesserung_1',
					wert: {
						art: 'gruppe',
						inhalt: [{ name: 'datum', wert: { art: 'text', inhalt: '2026-11-01' } }]
					}
				}
			]
		},
		{
			...leer,
			datei: '2026-10-30-kaputt.toml',
			datum: '2026-10-30',
			fehler: 'Diese Datei lässt sich nicht lesen: In Zeile 3 ist die Schreibweise falsch.'
		},
		{ ...leer, datei: '2026-10-25-alt.toml', datum: '2026-10-25', thema: 'Alt', etappe: '2a' }
	],
	warnung: null
};

function zeige(data: {
	liste: EntscheidungsListe | null;
	fehler: string | null;
	anmelden?: string | null;
}) {
	return render(Page, { data: { anmelden: null, ...data }, params: {} } as never);
}

describe('/entscheidungen (Etappe 4d)', () => {
	it('zeigt die Einträge mit Thema, Eckdaten, Urteil und was, ohne Markdown', () => {
		zeige({ liste, fehler: null });
		expect(screen.getByRole('heading', { level: 1, name: 'Entscheidungen' })).toBeInTheDocument();
		const plan = screen.getByRole('region', { name: 'Plan Etappe 4' });
		expect(plan).toHaveTextContent('2026-10-31 · Etappe 4 · geprüft · von Architektur-Rolle');
		expect(plan).toHaveTextContent('Urteil: Ja mit Ergänzungen');
		// `was` als Absätze
		expect(within(plan).getByText('Zweiter Absatz.').tagName).toBe('P');
		// Nur der Dateiname, kein Pfad
		expect(within(plan).getByText('2026-10-31-plan-4.toml')).toBeInTheDocument();
		expect(document.body.textContent).not.toMatch(/docs.entscheidungen/);
	});

	it('eine kaputte Datei ist ein Eintrag mit Satz, der Rest bleibt', () => {
		zeige({ liste, fehler: null });
		const kaputt = screen.getByRole('region', { name: '2026-10-30-kaputt.toml' });
		expect(within(kaputt).getByRole('note')).toHaveTextContent('In Zeile 3');
		expect(screen.getByRole('region', { name: 'Alt' })).toBeInTheDocument();
	});

	it('weitere Felder klappen auf: Text, Liste und Gruppe', async () => {
		zeige({ liste, fehler: null });
		const plan = screen.getByRole('region', { name: 'Plan Etappe 4' });
		expect(within(plan).queryByText('Weil es einfacher ist.')).toBeNull();
		await fireEvent.click(
			within(plan).getByRole('button', { name: /Alle weiteren Angaben \(3\)/ })
		);
		await waitFor(() => {
			expect(within(plan).getByText('Weil es einfacher ist.')).toBeInTheDocument();
		});
		expect(within(plan).getByRole('heading', { name: 'Auflagen spaeter' })).toBeInTheDocument();
		expect(within(plan).getByText('Zweite Auflage').tagName).toBe('LI');
		expect(
			within(plan).getByRole('heading', { name: 'Nachbesserung 1 · Datum' })
		).toBeInTheDocument();
		// Einträge ohne weitere Felder haben keinen Aufklapper.
		const alt = screen.getByRole('region', { name: 'Alt' });
		expect(within(alt).queryByRole('button')).toBeNull();
	});

	it('Suche nach Text und Etappe', async () => {
		zeige({ liste, fehler: null });
		const feld = screen.getByRole('searchbox', { name: 'Entscheidungen durchsuchen' });
		await fireEvent.input(feld, { target: { value: 'etappe 2a' } });
		await waitFor(() => {
			expect(screen.queryByRole('region', { name: 'Plan Etappe 4' })).toBeNull();
		});
		expect(screen.getByRole('region', { name: 'Alt' })).toBeInTheDocument();
		expect(screen.getByText('1 von 3')).toBeInTheDocument();
		await fireEvent.input(feld, { target: { value: 'gibt es nicht' } });
		expect(await screen.findByText(/Keine Entscheidung passt/)).toBeInTheDocument();
	});

	it('Fehler beim Laden und leere Liste: je ein Satz', () => {
		const { unmount } = zeige({ liste: null, fehler: 'Der Server antwortet nicht.' });
		expect(screen.getByRole('note')).toHaveTextContent('Der Server antwortet nicht.');
		unmount();
		zeige({ liste: { eintraege: [], warnung: null }, fehler: null });
		expect(screen.getByText('Hier stehen noch keine Entscheidungen.')).toBeInTheDocument();
	});

	it('kein Leserecht (Hotfix 4d): die Warnung als Problem, nicht „noch keine Entscheidungen“', () => {
		const satz =
			'Das Dashboard darf den Ordner mit den Entscheidungen nicht lesen. Der Ordner liegt im privaten Inhalt; dort hilft kein Chat. Einmal als Administrator scripts\\Dienst-Einrichten.ps1 -NurLeserecht (Anleitung, „Das Dashboard als Dienst“). Danach steht hier wieder alles.';
		zeige({ liste: { eintraege: [], warnung: satz }, fehler: null });
		const note = screen.getByRole('note');
		expect(note).toHaveTextContent('darf den Ordner mit den Entscheidungen nicht lesen');
		expect(note).toHaveClass('b-problem');
		expect(screen.queryByText('Hier stehen noch keine Entscheidungen.')).toBeNull();
		expect(screen.queryByRole('searchbox')).toBeNull();
	});

	it('ohne Feld warnung (älterer Server): wie bisher', () => {
		zeige({ liste: { eintraege: [] } as unknown as EntscheidungsListe, fehler: null });
		expect(screen.getByText('Hier stehen noch keine Entscheidungen.')).toBeInTheDocument();
	});
});

describe('Felder als Zeilen', () => {
	it('Namen lesbar, Gruppen mit Trenner, Listen einzeilig', () => {
		expect(feldName('auflagen_spaeter')).toBe('Auflagen spaeter');
		expect(
			einzeilig({
				art: 'gruppe',
				inhalt: [
					{ name: 'a', wert: { art: 'text', inhalt: 'x' } },
					{ name: 'b_c', wert: { art: 'liste', inhalt: [{ art: 'text', inhalt: 'y' }] } }
				]
			})
		).toBe('A: x; B c: y');
		const z = zeilen(liste.eintraege[0]?.weitere ?? []);
		expect(z.map((x) => x.name)).toEqual(['Warum', 'Auflagen spaeter', 'Nachbesserung 1 · Datum']);
		expect(z[1]?.punkte).toEqual(['Erste Auflage', 'Zweite Auflage']);
	});

	it('Filter: leer heißt alles, Groß/Klein egal', () => {
		expect(filtern(liste.eintraege, '')).toHaveLength(3);
		expect(filtern(liste.eintraege, 'PLAN').map((e) => e.datei)).toEqual([
			'2026-10-31-plan-4.toml'
		]);
	});
});

describe('/entscheidungen ohne Windows-Anmeldung (Abnahme 4d)', () => {
	it('401 vom Server: der Weg über Edge, wie bei /anstehend, nicht „lässt sich nicht laden“', async () => {
		const { load } = await import('./+page.ts');
		const fetch401 = (() =>
			Promise.resolve(
				new Response(
					JSON.stringify({ fehler: 'Bitte mit dem Windows-Konto anmelden.', art: 'anmeldung' }),
					{ status: 401, headers: { 'content-type': 'application/json' } }
				)
			)) as typeof fetch;
		const data = (await load({ fetch: fetch401, depends: () => undefined } as never)) as {
			liste: null;
			fehler: string | null;
			anmelden: string | null;
		};
		expect(data).toEqual({ liste: null, fehler: null, anmelden: SATZ_ENTSCHEIDUNGEN_ANMELDEN });
		zeige(data);
		const link = screen.getByRole('link', { name: 'http://localhost:8787' });
		expect(link.closest('p')?.textContent).toBe(`${SATZ_ENTSCHEIDUNGEN_ANMELDEN}.`);
		expect(document.body.textContent).not.toMatch(/lassen sich gerade nicht laden/);
		expect(screen.queryByRole('searchbox')).toBeNull();
	});
});
