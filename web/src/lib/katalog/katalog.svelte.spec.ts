import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type {
	Abgeleitet,
	Datei,
	Gantt as GanttDaten,
	Kennzahlen as KennzahlenDaten,
	Zeitstrahl as ZeitstrahlDaten
} from '$vertrag';
import type { Geladen } from '../projekt/laden.svelte.ts';
import { Gantt, Kennzahlen, Liste, Text, Zeitstrahl } from './index.ts';

const kennzahlen: KennzahlenDaten = {
	kennzahl: [
		{ titel: 'Schritte erledigt', wert: '2', einheit: null, hinweis: 'berechnet', farbe: null }
	]
};

function geladen<T>(teil: Partial<Geladen<T>>): Geladen<T> {
	return {
		zustand: 'fertig',
		inhalt: null,
		abgeleitet: null,
		satz: null,
		neuLaden: () => Promise.resolve(),
		...teil
	};
}

describe('$katalog', () => {
	it('Kennzahlen nehmen den Inhalt aus der API direkt', () => {
		render(Kennzahlen, { daten: kennzahlen });
		// Kachel und Textfassung (Etappe 3b).
		expect(screen.getAllByText('Schritte erledigt')).toHaveLength(2);
		expect(screen.getAllByText('berechnet')).toHaveLength(2);
	});

	it('nehmen auch das Objekt aus ladeDaten: erst „wird geladen“, dann der Inhalt', async () => {
		const r = render(Kennzahlen, { daten: geladen<KennzahlenDaten>({ zustand: 'laedt' }) });
		expect(screen.getByText(/Daten werden geladen/)).toBeInTheDocument();
		await r.rerender({ daten: geladen<KennzahlenDaten>({ inhalt: kennzahlen }) });
		// Kachel und Textfassung (Etappe 3b).
		expect(screen.getAllByText('Schritte erledigt')).toHaveLength(2);
	});

	it('Fehler: der Satz des Servers und ein Knopf „neu laden“', async () => {
		const neuLaden = vi.fn(() => Promise.resolve());
		const satz = 'projects/muster/daten/gantt.toml, Zeile 4: Bei `bis` steht kein Datum.';
		render(Gantt, { daten: geladen<GanttDaten>({ zustand: 'fehler', satz, neuLaden }) });
		expect(screen.getByRole('alert')).toHaveTextContent(satz);
		await fireEvent.click(screen.getByRole('button', { name: 'neu laden' }));
		expect(neuLaden).toHaveBeenCalledOnce();
	});

	it('Teile: ohne `teil` alle mit Überschrift, mit `teil` nur dieser, unbekannt ein Satz', () => {
		const datei: Datei<KennzahlenDaten> = {
			teil: {
				vorher: {
					kennzahl: [{ titel: 'A', wert: '1', einheit: null, hinweis: null, farbe: null }]
				},
				nachher: {
					kennzahl: [{ titel: 'B', wert: '2', einheit: null, hinweis: null, farbe: null }]
				}
			}
		};
		const a = render(Kennzahlen, { daten: datei });
		expect(screen.getByRole('heading', { name: 'vorher' })).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'nachher' })).toBeInTheDocument();
		a.unmount();

		const b = render(Kennzahlen, { daten: datei, teil: 'nachher' });
		expect(screen.queryByText('A')).toBeNull();
		expect(screen.getAllByText('B')).toHaveLength(2);
		expect(screen.queryByRole('heading')).toBeNull();
		b.unmount();

		render(Kennzahlen, { daten: datei, teil: 'gibt-es-nicht' });
		expect(screen.getByText(/keinen Teil „gibt-es-nicht“/)).toHaveTextContent(
			'Vorhanden: vorher, nachher.'
		);
	});

	it('Zeitstrahl zeigt Erledigtes und den nächsten Termin aus dem Abgeleiteten', () => {
		const z: ZeitstrahlDaten = {
			eintrag: [
				{ datum: '2026-10-01', titel: 'Start', text: null, farbe: 'ok' },
				{ datum: '2026-11-01', titel: 'Abgabe', text: null, farbe: 'wartet' }
			]
		};
		const abg: Abgeleitet = {
			heute: '2026-10-27',
			listen: {
				eintrag: {
					anzahl: 2,
					erledigt: { feld: 'farbe', anzahl: 1, anteil: 0.5 },
					summe: {},
					datum: {
						datum: { tage_bis: [-26, 5], tage_seit: [26, -5], naechster: 1, letzter: 0 }
					},
					marken: {}
				}
			}
		};
		render(Zeitstrahl, { daten: z, abgeleitet: abg });
		expect(
			screen.getByText('1 von 2 erledigt · Als Nächstes: Abgabe, in 5 Tagen')
		).toBeInTheDocument();
	});

	it('Gantt mit Fehler im Plan zeigt den Satz statt Balken', () => {
		const g: GanttDaten = {
			start: null,
			balken: [{ name: 'Kaputt', von: '2027-01', bis: '2026-11', farbe: null, gruppe: null }]
		};
		render(Gantt, { daten: g });
		expect(screen.getByText(/liegt `bis` vor `von`/)).toBeInTheDocument();
	});

	it('Text: Leerzeile beginnt einen neuen Absatz; Liste nummeriert', () => {
		const { container } = render(Text, { text: 'Erster Absatz\nnoch dabei.\n\nZweiter.' });
		expect(container.querySelectorAll('p')).toHaveLength(2);
		render(Liste, { punkte: ['eins', 'zwei'], nummeriert: true });
		expect(screen.getAllByRole('listitem')).toHaveLength(2);
	});
});
