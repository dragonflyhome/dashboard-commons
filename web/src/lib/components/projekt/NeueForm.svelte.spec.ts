import { render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { DatenAntwort, ProjektDaten, VerlaufEreignis, Zeitstrahl } from '$vertrag';
import NeueForm from './NeueForm.svelte';

afterEach(() => {
	vi.unstubAllGlobals();
});

const daten: ProjektDaten = {
	kennzahlen: {
		kennzahl: [{ titel: 'Preis', wert: '12', einheit: '€', hinweis: null, farbe: null }]
	},
	ampel: { zeile: [{ titel: 'Saatgut', farbe: 'problem', hinweis: 'fehlt' }] },
	fortschritt: null,
	zeitstrahl: null,
	gantt: null,
	kanban: null,
	fluss: null,
	fragen: {
		frage: [
			{
				id: 'name',
				text: 'Wie soll das Produkt heißen?',
				art: 'wahl',
				optionen: ['Heli', 'Spray'],
				empfehlung: 'Heli',
				gestellt: '2026-10-26'
			}
		]
	},
	orte: null,
	fristen: null,
	plan: null,
	eigen: { rezept: { menge: 3 } }
};

describe('NeueForm', () => {
	it('zeigt je Baustein eine Karte, Fragen, eigene Daten und Unterseiten', () => {
		render(NeueForm, {
			slug: 'probe',
			daten,
			seiten: [
				{ name: 'labor', title: 'Labor', order: 2 },
				{ name: 'plan', title: 'Plan', order: 1 }
			]
		});
		expect(screen.getByRole('heading', { name: 'Kennzahlen' })).toBeInTheDocument();
		// Kachel und Textfassung (Etappe 3b).
		expect(screen.getAllByText('Preis')).toHaveLength(2);
		expect(screen.getByRole('heading', { name: 'Ampel' })).toBeInTheDocument();
		expect(screen.getByRole('img', { name: 'Problem' })).toHaveClass('b-problem');
		expect(screen.getByText('Wie soll das Produkt heißen?')).toBeInTheDocument();
		expect(screen.getByText('Empfehlung: Heli')).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: /Heli/ })).not.toBeInTheDocument();
		expect(screen.getByText(/"menge": 3/)).toBeInTheDocument();
		const titel = screen.getAllByRole('listitem').map((li) => li.textContent);
		expect(titel.indexOf('Plan')).toBeLessThan(titel.indexOf('Labor'));
		expect(screen.getByText(/Etappe 4/)).toBeInTheDocument();
		expect(screen.queryByRole('heading', { name: 'Gantt' })).not.toBeInTheDocument();
	});
	it('sagt, wenn es noch keine Daten gibt', () => {
		render(NeueForm, { slug: 'probe', daten: null, seiten: [] });
		expect(screen.getByText(/Noch keine Daten/)).toBeInTheDocument();
	});
});

/** Ein Ereignis, wie der Server es für einen Eintrag des Zeitstrahls rechnet. */
function hand(kennung: string, datum: string, titel: string): VerlaufEreignis {
	return {
		kennung: `hand:${kennung}`,
		zeit: `${datum}T00:00:00+01:00`,
		genauigkeit: 'tag',
		bis: null,
		titel,
		text: null,
		quelle: 'hand',
		art: 'erledigt',
		verweis: null
	};
}

describe('NeueForm: Zeitstrahl (Weg über ladeDaten, Auflage aus 3a-1)', () => {
	const teile: { teil: Record<string, Zeitstrahl> } = {
		teil: {
			Planung: {
				eintrag: [
					{ kennung: 'plan-a', datum: '2025-03-01', titel: 'Idee', text: null, farbe: null }
				]
			},
			Umsetzung: {
				eintrag: [
					{ kennung: 'bau-a', datum: '2024-06-01', titel: 'Anfang', text: null, farbe: null }
				]
			}
		}
	};

	it('lädt die Datei, zeigt je Teil seine Ereignisse; Teil h3, Jahre h4 unter der Karte h2', async () => {
		const antwort: DatenAntwort = {
			name: 'zeitstrahl',
			inhalt: teile,
			abgeleitet: {
				heute: '2026-10-30',
				listen: {},
				ereignisse: [hand('plan-a', '2025-03-01', 'Idee'), hand('bau-a', '2024-06-01', 'Anfang')]
			}
		};
		const holen = vi.fn(() =>
			Promise.resolve(
				new Response(JSON.stringify(antwort), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				})
			)
		);
		vi.stubGlobal('fetch', holen);
		const { container } = render(NeueForm, {
			slug: 'probe',
			daten: { ...daten, zeitstrahl: teile },
			seiten: []
		});
		await screen.findByText('Idee');
		expect(holen).toHaveBeenCalledWith('/api/projekte/probe/daten/zeitstrahl', expect.anything());
		const karte = screen.getByRole('heading', { level: 2, name: 'Zeitstrahl' }).closest('section');
		if (karte === null) throw new Error('Karte fehlt');
		const ueberschriften = [...karte.querySelectorAll('h2, h3, h4')].map(
			(h) => `${h.tagName.toLowerCase()} ${h.textContent.trim()}`
		);
		expect(ueberschriften).toEqual([
			'h2 Zeitstrahl',
			'h3 Planung',
			'h4 2025',
			'h3 Umsetzung',
			'h4 2024'
		]);
		// Jeder Teil nur mit seinen Ereignissen.
		expect(container.querySelectorAll('[data-kennung]')).toHaveLength(2);
	});
});
