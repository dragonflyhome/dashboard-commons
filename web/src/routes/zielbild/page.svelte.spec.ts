import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { SystemAbschnitt, ZielbildAntwort } from '$vertrag';
import { SATZ_ZIELBILD_ANMELDEN } from '#lib/anmeldung.ts';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: Page } = await import('./+page.svelte');

const PLATZHALTER = 'Im Zielbild steht noch kein Abschnitt.';

function zeige(data: {
	zielbild: ZielbildAntwort | null;
	fehler: string | null;
	anmelden?: string | null;
}) {
	return render(Page, { data: { anmelden: null, ...data }, params: {} } as never);
}

describe('/zielbild (Etappe 4d)', () => {
	it('ohne Daten: der Platzhalter, sonst nichts', () => {
		zeige({ zielbild: { abschnitte: [], warnung: null }, fehler: null });
		expect(screen.getByRole('heading', { level: 1, name: 'Zielbild' })).toBeInTheDocument();
		expect(screen.getByRole('note')).toHaveTextContent(PLATZHALTER);
		expect(screen.queryByRole('region')).toBeNull();
	});

	it('bei einem Fehler: Satz und Platzhalter', () => {
		zeige({ zielbild: null, fehler: 'Der Server antwortet nicht.' });
		const notes = screen.getAllByRole('note');
		expect(notes[0]).toHaveTextContent('Der Server antwortet nicht.');
		expect(notes[1]).toHaveTextContent(PLATZHALTER);
	});

	it('kein Leserecht (Hotfix 4d): die Warnung als Problem statt des Platzhalters', () => {
		zeige({
			zielbild: {
				abschnitte: [],
				warnung: 'Das Dashboard darf den Ordner mit dem Zielbild nicht lesen.'
			},
			fehler: null
		});
		const note = screen.getByRole('note');
		expect(note).toHaveTextContent('Das Dashboard darf den Ordner mit dem Zielbild nicht lesen.');
		expect(note).toHaveClass('b-problem');
		expect(screen.queryByText(PLATZHALTER)).toBeNull();
	});

	it('mit Daten: Abschnitte mit Text und Punkten, kaputte Datei mit Satz', () => {
		zeige({
			zielbild: {
				abschnitte: [
					{
						datei: 'grundsatz.toml',
						titel: 'Grundsatz',
						reihenfolge: 1,
						text: 'Erster Absatz.\n\nZweiter Absatz.',
						punkte: ['Erstens', 'Zweitens'],
						nummeriert: false,
						bloecke: [],
						hinweise: [],
						weitere: [],
						fehler: null
					},
					{
						datei: 'kaputt.toml',
						titel: null,
						reihenfolge: null,
						text: null,
						punkte: [],
						nummeriert: false,
						bloecke: [],
						hinweise: [],
						weitere: [],
						fehler: 'Diese Datei lässt sich nicht lesen: In Zeile 2 ist die Schreibweise falsch.'
					}
				],
				warnung: null
			},
			fehler: null
		});
		expect(screen.queryByText(PLATZHALTER)).toBeNull();
		const g = screen.getByRole('region', { name: 'Grundsatz' });
		expect(g).toHaveTextContent('Zweiter Absatz.');
		expect(screen.getByText('Zweitens').tagName).toBe('LI');
		expect(screen.getByText('Zweitens').parentElement?.tagName).toBe('UL');
		expect(screen.getByRole('region', { name: 'kaputt.toml' })).toHaveTextContent('Zeile 2');
	});
});

describe('/zielbild mit Blöcken', () => {
	const BEISPIEL = 'erste Zeile\n  eingerückt <b>fett?</b> <script>alert(1)</script>\nletzte';

	function mitBloecken() {
		const abschnitt: SystemAbschnitt = {
			datei: 'etappen.toml',
			titel: 'Etappen',
			reihenfolge: 1,
			text: 'Vorweg.',
			punkte: ['Vorher'],
			nummeriert: true,
			bloecke: [
				{
					titel: 'Warum',
					nummeriert: false,
					inhalt: { art: 'text', wert: 'Absatz eins.\n\nAbsatz **zwei**.' }
				},
				{ titel: null, nummeriert: true, inhalt: { art: 'punkte', wert: ['Eins', 'Zwei'] } },
				{
					titel: 'Plan',
					nummeriert: false,
					inhalt: {
						art: 'tabelle',
						wert: {
							spalten: ['Etappe', 'Was', 'Warum'],
							zeilen: [
								['1a', 'Lesen', 'Weil'],
								['1b', 'Zeigen', 'Darum']
							]
						}
					}
				},
				{ titel: 'Aufruf', nummeriert: false, inhalt: { art: 'beispiel', wert: BEISPIEL } }
			],
			hinweise: ['Block 5: Er hat keinen Inhalt und wird nicht gezeigt.'],
			weitere: [{ name: 'notiz', wert: { art: 'text', inhalt: 'Unbekanntes Feld' } }],
			fehler: null
		};
		return zeige({ zielbild: { abschnitte: [abschnitt], warnung: null }, fehler: null });
	}

	it('alle vier Arten in Reihenfolge, mit kleinen Überschriften', () => {
		mitBloecken();
		const r = screen.getByRole('region', { name: 'Etappen' });
		const ueber = [...r.querySelectorAll('h3')].map((h) => h.textContent);
		expect(ueber).toEqual(['Warum', 'Plan', 'Aufruf', 'Notiz']);
		// Text: Absätze, Markdown bleibt Text.
		expect(screen.getByText('Absatz **zwei**.').tagName).toBe('P');
		expect(screen.getByText('Eins').tagName).toBe('LI');
		// nummeriert = true auf Abschnitts- und Blockebene: <ol>.
		expect(screen.getByText('Vorher').parentElement?.tagName).toBe('OL');
		expect(screen.getByText('Eins').parentElement?.tagName).toBe('OL');
		// Tabelle: Kopf und Zeilen aus dem Block.
		const tabelle = screen.getByRole('table', { name: 'Plan' });
		expect([...tabelle.querySelectorAll('th')].map((th) => th.textContent)).toEqual([
			'Etappe',
			'Was',
			'Warum'
		]);
		expect([...tabelle.querySelectorAll('tbody tr')].map((tr) => tr.textContent)).toEqual([
			'1aLesenWeil',
			'1bZeigenDarum'
		]);
		// Reihenfolge im Dokument: Text, Punkte, Blöcke, übrige Felder.
		const text = r.textContent;
		const stellen = ['Vorweg.', 'Vorher', 'Absatz eins.', 'Eins', '1a', 'erste Zeile', 'Unbekannt'];
		const pos = stellen.map((s) => text.indexOf(s));
		expect(pos.every((p) => p >= 0)).toBe(true);
		expect([...pos].sort((x, y) => x - y)).toEqual(pos);
	});

	it('der Hinweis „Block n“ steht im Abschnitt, der Rest bleibt', () => {
		mitBloecken();
		const r = screen.getByRole('region', { name: 'Etappen' });
		const note = screen.getByRole('note');
		expect(r).toContainElement(note);
		expect(note).toHaveClass('b-problem');
		expect(note).toHaveTextContent(/^Block 5: Er hat keinen Inhalt/);
		expect(r).toHaveTextContent('Vorweg.');
		expect(screen.getByRole('table', { name: 'Plan' })).toBeInTheDocument();
	});

	it('beispiel: vorformatiert, Zeilen bleiben, kein HTML', () => {
		const { container } = mitBloecken();
		const pre = container.querySelector('pre');
		expect(pre).not.toBeNull();
		expect(pre?.textContent).toBe(BEISPIEL);
		expect(pre?.children.length).toBe(0);
		expect(container.querySelector('b, script')).toBeNull();
	});

	it('Überlauf: Tabelle und Beispiel rollen je im eigenen Rahmen', () => {
		const { container } = mitBloecken();
		const rahmen = [...container.querySelectorAll('.scroll-x')];
		expect(rahmen.map((r) => r.tagName)).toEqual(['DIV', 'PRE']);
		expect(rahmen[0]).toContainElement(screen.getByRole('table', { name: 'Plan' }));
		// jsdom lädt app.css nicht: dort rollt .scroll-x seitlich, nie die Seite.
		const css = readFileSync(join(import.meta.dirname, '..', '..', 'app.css'), 'utf8');
		expect(css).toMatch(
			/\.scroll-x,\s*\.table-scroll\s*\{[^}]*overflow-x: auto;[^}]*max-width: 100%;/
		);
		expect(rahmen[1]).toHaveClass('beispiel');
	});
});

describe('/zielbild ohne Windows-Anmeldung (Abnahme 4d)', () => {
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
			zielbild: null;
			fehler: string | null;
			anmelden: string | null;
		};
		expect(data).toEqual({ zielbild: null, fehler: null, anmelden: SATZ_ZIELBILD_ANMELDEN });
		zeige(data);
		const link = screen.getByRole('link', { name: 'http://localhost:8787' });
		expect(link.closest('p')?.textContent).toBe(`${SATZ_ZIELBILD_ANMELDEN}.`);
		expect(document.body.textContent).not.toMatch(/lässt sich gerade nicht laden/);
		expect(screen.queryByText(PLATZHALTER)).toBeNull();
	});
});
