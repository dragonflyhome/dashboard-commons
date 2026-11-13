/**
 * Etappe B2, Modul Backlog: der Baustein `Backlog` (Spalten nach Stand, Wer
 * ist dran), sein Rahmen auf jeder Projektseite (lädt, ohne Anmeldung
 * nichts), die Fragen-Warteschlange in `auswaehlen` und im Baustein `Fragen`.
 * Alle Daten erfunden.
 */
import { cleanup, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { AnstehendEintrag, Backlog as BacklogDaten, Frage } from '$vertrag';
import { Backlog, Fragen } from '$katalog';
import BacklogRahmen from '#lib/components/projekt/BacklogRahmen.svelte';
import FragenListe from '#lib/components/projekt/FragenListe.svelte';
import { auswaehlen } from '#lib/anstehend.ts';
import { eintrag } from '#lib/anstehend.beispiel.ts';
import { ladeBacklog } from '#lib/backlog.ts';
import { backlog, fragen, fragenAbgeleitet } from '../routes/katalog/beispiele.ts';

afterEach(cleanup);

function antwort(status: number, koerper: unknown): Response {
	return new Response(JSON.stringify(koerper), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

describe('Baustein Backlog', () => {
	it('zeigt vier Spalten mit Zahl (Etappe B8: ohne Fertig), Karten als Links zum Eintrag und Wer ist dran', () => {
		render(Backlog, { backlog: backlog() });
		const spalten = screen
			.getAllByRole('heading', { level: 3 })
			.map((h) => h.textContent.replace(/\s+/g, ' ').trim());
		expect(spalten).toEqual([
			'Für dich 2',
			'Läuft 1',
			'Wartet 2',
			'Als Nächstes 1',
			'Wer ist dran'
		]);
		const fuerDich = screen.getByRole('region', { name: /Für dich/ });
		const link = within(fuerDich).getByRole('link', { name: 'Beim Beispielportal anmelden' });
		expect(link.getAttribute('href')).toMatch(/\/projects\/beispiel#aufgabe-a-20261103-0001$/);
		// Die eine Frage mit dem Satz zu den übrigen, ohne deren Text.
		expect(within(fuerDich).getByText('noch 1 Frage wartet')).toBeInTheDocument();
		expect(screen.queryByText('Wie soll die Unterseite heißen?')).toBeNull();
		// Frist heute ist hervorgehoben.
		expect(within(fuerDich).getByText(/^bis /)).toHaveClass('achtung');
		// Wer ist dran: du, ein Chat, jemand draußen.
		expect(screen.getByText('Wartet auf dich')).toBeInTheDocument();
		expect(screen.getByText('Wartet auf den Chat „nachbar“')).toBeInTheDocument();
		expect(screen.getByText('Wartet auf jemanden draußen')).toBeInTheDocument();
		expect(screen.getByText(/eine steht oben; noch 1 Frage wartet/)).toBeInTheDocument();
		// Keine Klick-Antworten im Backlog: geantwortet wird beim Eintrag.
		expect(screen.queryByRole('button')).toBeNull();
	});

	it('leer: ein Satz statt Spalten; ohne Datenspeicher ein Hinweis', () => {
		const b: BacklogDaten = {
			...backlog(),
			spalten: backlog().spalten.map((s) => ({ ...s, karten: [] })),
			wartend: [],
			fragen_weitere: 0,
			speicher_fehlt: true
		};
		render(Backlog, { backlog: b });
		expect(screen.getByText(/Nichts offen/)).toBeInTheDocument();
		expect(screen.getByText(/Der Datenspeicher fehlt gerade/)).toBeInTheDocument();
		expect(screen.queryByText('Wer ist dran')).toBeNull();
	});
});

describe('BacklogRahmen auf jeder Projektseite', () => {
	it('lädt /api/projekte/<slug>/backlog und zeigt den Abschnitt', async () => {
		const fetchFn = vi.fn(() => Promise.resolve(antwort(200, backlog())));
		render(BacklogRahmen, { slug: 'beispiel', fetchFn });
		expect(await screen.findByRole('heading', { name: 'Backlog', level: 2 })).toBeInTheDocument();
		expect(fetchFn).toHaveBeenCalledWith('/api/projekte/beispiel/backlog', expect.anything());
	});

	it('ohne Anmeldung (401) oder ohne Projekt (404) fehlt der Abschnitt', async () => {
		for (const status of [401, 404]) {
			const fetchFn = vi.fn(() => Promise.resolve(antwort(status, { fehler: 'x' })));
			const r = render(BacklogRahmen, { slug: 'beispiel', fetchFn });
			await waitFor(() => {
				expect(fetchFn).toHaveBeenCalled();
			});
			expect(screen.queryByRole('heading', { name: 'Backlog' })).toBeNull();
			r.unmount();
		}
		expect(await ladeBacklog('beispiel', () => Promise.reject(new Error('Netz')))).toBeNull();
	});
});

describe('Fragen-Warteschlange (Etappe B2)', () => {
	it('auswaehlen nimmt den Platz vom Server, nicht das Alter', () => {
		const alt = eintrag({
			art: 'frage',
			id: 'alt',
			projekt: 'beispiel',
			gestellt: '2026-10-01',
			erstellt: '2026-10-01',
			warteschlange: { platz: 2, offen: 3 }
		});
		const vorn = eintrag({
			art: 'frage',
			id: 'vorn',
			projekt: 'beispiel',
			gestellt: '2026-10-20',
			erstellt: '2026-10-20',
			warteschlange: { platz: 1, offen: 3 }
		});
		const aus = auswaehlen([alt, vorn]);
		expect(aus.map((e) => e.id)).toEqual(['vorn']);
		// Die dritte offene Frage fehlt in der Liste (etwa gefiltert); gezählt wird nach dem Server.
		expect(aus[0]?.vermerk).toBe('noch 2 Fragen warten');
		// Fehlt Platz 1 in der Liste, ist keine Frage sichtbar (nie die zweite statt der ersten).
		expect(auswaehlen([alt])).toEqual([]);
		// Ohne Platz (alte Daten): die alte Regel, die älteste zuerst.
		const ohnePlatz = (e: AnstehendEintrag): AnstehendEintrag => {
			const kopie: Partial<AnstehendEintrag> = { ...e };
			delete kopie.warteschlange;
			return kopie as AnstehendEintrag;
		};
		const ohne = auswaehlen([ohnePlatz(vorn), ohnePlatz(alt)]);
		expect(ohne.map((e) => e.id)).toEqual(['alt']);
	});

	it('Fragen zeigt mit Warteschlange nur die sichtbare Frage und „noch N Fragen warten“', () => {
		render(Fragen, { daten: fragen(), abgeleitet: fragenAbgeleitet() });
		expect(screen.getByText('Soll die Ampel im Stand als Rand oben stehen?')).toBeInTheDocument();
		expect(screen.queryByText('Wie soll die Unterseite heißen?')).toBeNull();
		expect(screen.getByText('Dahinter noch 1 Frage wartet.')).toBeInTheDocument();
	});

	it('Fragen ohne Anmeldung: ein Satz zum Anmelden, kein Fehler, kein Fragentext', () => {
		render(Fragen, {
			daten: {
				zustand: 'fehler',
				inhalt: null,
				abgeleitet: null,
				satz: 'Anmelden, um das zu sehen.',
				ohneAnmeldung: true,
				neuLaden: () => Promise.resolve()
			}
		});
		expect(screen.getByText(/Anmelden, um die Fragen an dich zu sehen/)).toBeInTheDocument();
		expect(screen.queryByRole('alert')).toBeNull();
		expect(screen.queryByRole('button', { name: 'neu laden' })).toBeNull();
	});

	it('FragenListe: ohne Warteschlange alle (alte Daten); alle beantwortet: keine offene Frage', () => {
		const liste: Frage[] = fragen().frage;
		const a = render(FragenListe, { fragen: liste });
		expect(screen.getAllByRole('listitem').filter((li) => li.querySelector('b'))).toHaveLength(2);
		a.unmount();
		render(FragenListe, {
			fragen: liste,
			warteschlange: { sichtbar: null, gestellt: null, weitere: 0 }
		});
		expect(screen.getByText('Keine offene Frage.')).toBeInTheDocument();
	});
});
