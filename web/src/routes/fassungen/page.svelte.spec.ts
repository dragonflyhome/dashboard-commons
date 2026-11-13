import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { FassungenAntwort, FassungsZeilen } from '$vertrag';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: Page } = await import('./+page.svelte');

const LEER: FassungsZeilen = {
	hinzugefuegt: [],
	geaendert: [],
	entfernt: [],
	behoben: [],
	brueche: []
};

const ANTWORT: FassungenAntwort = {
	laufend: '0.2.0',
	unveroeffentlicht: { ...LEER, hinzugefuegt: ['Seite /fassungen.'] },
	fassungen: [
		{
			version: '0.2.0',
			tag: 'v0.2.0',
			datum: '2026-11-13',
			...LEER,
			geaendert: ['Bruch: Alte Form weg.', 'Anders.'],
			behoben: ['Fehler B.'],
			brueche: ['Bruch: Alte Form weg.']
		},
		{ version: '0.1.0', tag: 'v0.1.0', datum: '2026-11-06', ...LEER }
	],
	hinweise: []
};

function zeige(data: { fassungen: FassungenAntwort | null; fehler: string | null }) {
	return render(Page, { data, params: {} } as never);
}

describe('Seite /fassungen', () => {
	it('zeigt laufende Fassung, Unveröffentlichtes und jede Fassung mit Datum und Tag', () => {
		zeige({ fassungen: ANTWORT, fehler: null });
		expect(screen.getByRole('heading', { level: 1, name: 'Fassungen' })).toBeInTheDocument();
		expect(
			screen.getByText('Es läuft die Fassung 0.2.0 vom 13.11.2026, dazu Unveröffentlichtes.')
		).toBeInTheDocument();
		const titel = screen.getAllByRole('heading', { level: 2 }).map((h) => h.textContent);
		expect(titel).toEqual(['Unveröffentlicht', '0.2.0 vom 13.11.2026', '0.1.0 vom 06.11.2026']);
		expect(screen.getByText('Tag v0.2.0')).toBeInTheDocument();
		expect(screen.getByText('Seite /fassungen.')).toBeInTheDocument();
		// Der Bruch trägt das Wort hervorgehoben, die übrige Zeile bleibt Text.
		const bruch = screen.getByText('Bruch:');
		expect(bruch.tagName).toBe('STRONG');
		expect(bruch.closest('li')?.textContent).toBe('Bruch: Alte Form weg.');
		expect(screen.getByText('Anders.').tagName).toBe('LI');
		// Eine Fassung ohne Einträge sagt das, statt zu verschwinden.
		expect(screen.getByText('Keine Einträge.')).toBeInTheDocument();
	});

	it('ohne Unveröffentlichtes kein Abschnitt dafür; Hinweise des Lesers stehen da', () => {
		zeige({
			fassungen: {
				...ANTWORT,
				unveroeffentlicht: LEER,
				hinweise: ['Zeile 9: Die Überschrift „x“ hat nicht die Form x.y.z - JJJJ-MM-TT.']
			},
			fehler: null
		});
		expect(screen.queryByRole('heading', { name: 'Unveröffentlicht' })).toBeNull();
		expect(screen.getByText('Es läuft die Fassung 0.2.0 vom 13.11.2026.')).toBeInTheDocument();
		expect(screen.getByRole('note')).toHaveTextContent('Zeile 9');
	});

	it('ein Fehler der API wird ein Satz', () => {
		zeige({ fassungen: null, fehler: 'Das gibt es nicht (mehr).' });
		expect(screen.getByRole('note')).toHaveTextContent(
			'Die Fassungen lassen sich gerade nicht laden: Das gibt es nicht (mehr).'
		);
	});
});
