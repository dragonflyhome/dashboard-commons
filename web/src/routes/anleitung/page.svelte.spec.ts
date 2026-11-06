import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { AnleitungAntwort, SystemAbschnitt } from '$vertrag';
import { SATZ_ANLEITUNG_ANMELDEN } from '#lib/anmeldung.ts';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: Page } = await import('./+page.svelte');

function zeige(data: {
	anleitung: AnleitungAntwort | null;
	fehler: string | null;
	anmelden?: string | null;
}) {
	return render(Page, { data: { anmelden: null, ...data }, params: {} } as never);
}

function abschnitt(teil: Partial<SystemAbschnitt> & { datei: string }): SystemAbschnitt {
	return {
		titel: null,
		reihenfolge: null,
		text: null,
		punkte: [],
		nummeriert: false,
		bloecke: [],
		hinweise: [],
		weitere: [],
		fehler: null,
		...teil
	};
}

/** Wie die echten Dateien in docs/anleitung: Blöcke mit Titel, Tabelle, Beispiel, Schritte. */
const ANLEITUNG: AnleitungAntwort = {
	abschnitte: [
		abschnitt({
			datei: 'contributing-05-committen.toml',
			titel: '5. Committen',
			reihenfolge: 105,
			bloecke: [
				{
					titel: null,
					nummeriert: false,
					inhalt: { art: 'text', wert: 'Nur die eigenen Pfade stagen.\n\nNie git add -A.' }
				},
				{
					titel: 'Für den Betreuer des Dashboards: der Vertrag aus einer Quelle',
					nummeriert: true,
					inhalt: { art: 'punkte', wert: ['Neue Typen nur in src/vertrag/ anlegen', 'cargo test'] }
				},
				{
					titel: null,
					nummeriert: false,
					inhalt: {
						art: 'tabelle',
						wert: { spalten: ['Datei', 'Regel'], zeilen: [['a.svelte', 'svelte/no-at-html-tags']] }
					}
				},
				{
					titel: null,
					nummeriert: false,
					inhalt: {
						art: 'beispiel',
						wert: 'pwsh -NoProfile -File %USERPROFILE%/dev/dashboard/scripts/Api.ps1 api/vorgaenge'
					}
				}
			]
		}),
		abschnitt({ datei: 'readme-00-dashboard.toml', titel: 'Dashboard', reihenfolge: 200 })
	],
	warnung: null
};

describe('/anleitung (Etappe 6b): die Abschnitte aus docs/anleitung', () => {
	it('zeigt jeden Abschnitt mit seinen Blöcken über SystemAbschnitt', () => {
		const { container } = zeige({ anleitung: ANLEITUNG, fehler: null });
		expect(screen.getByRole('heading', { level: 1, name: 'Anleitung' })).toBeInTheDocument();
		const regionen = screen.getAllByRole('region');
		expect(regionen).toHaveLength(2);
		const c = screen.getByRole('region', { name: '5. Committen' });
		expect(c).toHaveTextContent('Nie git add -A.');
		expect(
			screen.getByRole('heading', {
				level: 3,
				name: 'Für den Betreuer des Dashboards: der Vertrag aus einer Quelle'
			})
		).toBeInTheDocument();
		// Schritte nummeriert, Tabelle als Tabelle, Befehl als Beispiel im eigenen Rahmen.
		expect(screen.getByText('cargo test').parentElement?.tagName).toBe('OL');
		expect(screen.getByRole('table')).toHaveTextContent('svelte/no-at-html-tags');
		const pre = container.querySelector('pre.scroll-x');
		expect(pre?.textContent).toContain('%USERPROFILE%/dev/dashboard/scripts/Api.ps1');
		expect(c).toContainElement(pre as HTMLElement);
		// Reiner Text: kein Code-Element, kein Markdown.
		expect(container.querySelector('code')).toBeNull();
		expect(document.body.textContent).not.toMatch(/\*\*|`/);
		expect(screen.getByRole('region', { name: 'Dashboard' })).toBeInTheDocument();
		expect(screen.queryByText(/lässt sich gerade nicht laden/)).toBeNull();
	});

	it('nennt keinen Benutzerpfad', () => {
		zeige({ anleitung: ANLEITUNG, fehler: null });
		expect(document.body.textContent).not.toMatch(/C:[\\/]Users[\\/]/i);
	});

	it('leerer Ordner: ein Satz statt einer leeren Seite', () => {
		zeige({ anleitung: { abschnitte: [], warnung: null }, fehler: null });
		expect(screen.getByRole('note')).toHaveTextContent(/docs\/anleitung/);
		expect(screen.queryByRole('region')).toBeNull();
	});

	it('kein Leserecht: die Warnung als Problem', () => {
		zeige({
			anleitung: {
				abschnitte: [],
				warnung: 'Das Dashboard darf den Ordner mit der Anleitung nicht lesen.'
			},
			fehler: null
		});
		const note = screen.getByRole('note');
		expect(note).toHaveTextContent('Das Dashboard darf den Ordner mit der Anleitung nicht lesen.');
		expect(note).toHaveClass('b-problem');
	});

	it('bei einem Fehler: nur der Satz, kein Leer-Satz', () => {
		zeige({ anleitung: null, fehler: 'Der Server antwortet nicht.' });
		const notes = screen.getAllByRole('note');
		expect(notes).toHaveLength(1);
		expect(notes[0]).toHaveTextContent(
			'Die Anleitung lässt sich gerade nicht laden: Der Server antwortet nicht.'
		);
	});
});

describe('/anleitung ohne Windows-Anmeldung', () => {
	it('401 vom Server: der Weg über Edge, nicht der Inhalt', async () => {
		const { load } = await import('./+page.ts');
		const fetch401 = (() =>
			Promise.resolve(
				new Response(
					JSON.stringify({ fehler: 'Bitte mit dem Windows-Konto anmelden.', art: 'anmeldung' }),
					{ status: 401, headers: { 'content-type': 'application/json' } }
				)
			)) as typeof fetch;
		const schluessel: string[] = [];
		const data = (await load({
			fetch: fetch401,
			depends: (...k: string[]) => schluessel.push(...k)
		} as never)) as { anleitung: null; fehler: string | null; anmelden: string | null };
		expect(schluessel).toEqual(['api:anleitung']);
		expect(data).toEqual({ anleitung: null, fehler: null, anmelden: SATZ_ANLEITUNG_ANMELDEN });
		zeige(data);
		const link = screen.getByRole('link', { name: 'http://localhost:8787' });
		expect(link.closest('p')?.textContent).toBe(`${SATZ_ANLEITUNG_ANMELDEN}.`);
		expect(screen.queryByRole('region')).toBeNull();
		expect(document.body.textContent).not.toMatch(/lässt sich gerade nicht laden/);
	});

	it('mit Anmeldung: lädt /api/anleitung', async () => {
		const { load } = await import('./+page.ts');
		const wege: string[] = [];
		const fetch200 = ((weg: string) => {
			wege.push(weg);
			return Promise.resolve(
				new Response(JSON.stringify(ANLEITUNG), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				})
			);
		}) as typeof fetch;
		const data = (await load({ fetch: fetch200, depends: () => undefined } as never)) as {
			anleitung: AnleitungAntwort | null;
		};
		expect(wege).toEqual(['/api/anleitung']);
		expect(data.anleitung?.abschnitte).toHaveLength(2);
	});
});
