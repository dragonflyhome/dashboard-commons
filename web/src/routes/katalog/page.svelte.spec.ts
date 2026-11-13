import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

vi.mock('$app/state', () => ({
	page: { url: new URL('http://localhost:8787/katalog'), params: {}, route: { id: '/katalog' } }
}));

const { default: Katalog } = await import('./+page.svelte');

describe('/katalog (Etappe 3a-3)', () => {
	it('zeigt alle Abschnitte mit Inhaltsverzeichnis und jeden Baustein mit Beispiel und Code', () => {
		const fetchSpion = vi.fn();
		vi.stubGlobal('fetch', fetchSpion);
		try {
			render(Katalog);
			const nav = screen.getByRole('navigation', { name: 'Inhalt des Katalogs' });
			expect(
				within(nav)
					.getAllByRole('link')
					.map((a) => a.getAttribute('href'))
			).toEqual(['#grundteile', '#text', '#daten', '#zeit', '#als-text', '#widgets']);
			// Rollen einmal ermitteln statt je Baustein (getByRole mit Namen durchsucht
			// jedes Mal die ganze Seite; unter Last waren das über 5 s).
			const artikel = screen.getAllByRole('article');
			const karten = new Map(
				artikel.map((a) => [
					document.getElementById(a.getAttribute('aria-labelledby') ?? '')?.textContent,
					a
				])
			);
			// Wie vorher getByRole: kein Name doppelt (die Map würde Doppelte still überschreiben).
			expect(karten.size).toBe(artikel.length);
			for (const name of [
				'Knopf',
				'Schalter',
				'Wahl',
				'Auswahl',
				'Dialog',
				'Kurzinfo',
				'Schublade',
				'Klappfeld',
				'Minilinie',
				'Reiter',
				'Aufklapper',
				'Tabelle',
				'Abschnitt',
				'Hinweis',
				'Marke',
				'Stand-Hinweis',
				'Tabelle aus Daten',
				'Kennzahlen',
				'Ampel',
				'Gantt',
				'Orte',
				'Verlauf',
				'Dichtestreifen',
				'Zeitachse',
				'Plan',
				'Stand',
				'Stand (Teil der Projektseite)',
				'Frage',
				'Arbeiten',
				'Fristen',
				'Zuletzt'
			]) {
				const karte = karten.get(name);
				if (karte === undefined) throw new Error(`Baustein ${name} fehlt`);
				expect(karte).toHaveAccessibleName(name);
				expect(within(karte).getByText('So benutzt')).toBeInTheDocument();
			}
			// Die Seite selbst fragt den Server nie (den Rahmen +layout.svelte rendert der Test nicht).
			expect(fetchSpion).not.toHaveBeenCalled();
		} finally {
			vi.unstubAllGlobals();
		}
	});

	it('Etappe 3b: jede Textfassung steht offen im Abschnitt „Als Text“', () => {
		render(Katalog);
		const abschnitt = document.getElementById('als-text');
		if (abschnitt === null) throw new Error('als-text fehlt');
		const tabellen = within(abschnitt)
			.getAllByRole('table')
			.map((t) => t.querySelector('caption')?.textContent);
		for (const titel of [
			'Balkenplan als Tabelle',
			'Einträge je Tag, letzte 91 Tage (nur Tage mit Einträgen)',
			'Arbeitsspeicher als Text',
			'Prozessor, letzte 15 Minuten als Text',
			'Fortschritt als Tabelle',
			'Kennzahlen als Tabelle',
			'Flussdiagramm als Tabelle',
			'PC-Verlauf als Tabelle'
		]) {
			expect(tabellen).toContain(titel);
		}
		// Offen heißt sichtbar, nicht nur für Vorlese-Programme.
		for (const t of within(abschnitt).getAllByRole('table')) {
			expect(t.parentElement).not.toHaveClass('nur-vorlesen');
		}
		expect(screen.getByRole('link', { name: 'Logo (drei Entwürfe)' })).toHaveAttribute(
			'href',
			'/katalog/entwuerfe/logo'
		);
	});

	it('Widgets: je drei Entwürfe, genau einer gewählt', () => {
		render(Katalog);
		for (const id of ['stand', 'frage', 'arbeiten', 'fristen', 'zuletzt']) {
			const karte = document.getElementById(`widget-${id}`);
			if (karte === null) throw new Error(`widget-${id} fehlt`);
			expect(karte.querySelectorAll('figure')).toHaveLength(3);
			expect(within(karte).getAllByText('gewählt')).toHaveLength(1);
		}
	});

	it('Grundteile antworten auf Tastatur und Klick (Schalter, Tabelle)', async () => {
		render(Katalog);
		const s = screen.getByRole('switch', { name: 'Nur offene zeigen' });
		expect(s).toHaveAttribute('aria-checked', 'true');
		await fireEvent.click(s);
		expect(s).toHaveAttribute('aria-checked', 'false');
		expect(screen.getByText('Jetzt: alle')).toBeInTheDocument();
		// jsdom misst keine Breiten: die Tabelle rollt hier nicht, also kein
		// Tab-Stopp. Den Bereich bei Überstand prüfen grund.svelte.spec.ts und
		// die Edge-Probe.
		const tabelle = screen.getByRole('table', { name: 'Wo was liegt' });
		expect(tabelle.parentElement).not.toHaveAttribute('tabindex');
	});

	it('Widget „Zuletzt“ im Katalog: „gelesen“ ändert nur die Ansicht', async () => {
		render(Katalog);
		const karte = document.getElementById('widget-zuletzt');
		if (karte === null) throw new Error('fehlt');
		const knopf = within(karte).getByRole('button', { name: 'Gelesen: Prüfung: Tabelle zu breit' });
		await fireEvent.click(knopf);
		expect(within(karte).queryByRole('button', { name: /^Gelesen:/ })).toBeNull();
	});
});
