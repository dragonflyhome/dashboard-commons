import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { ArchivListe } from '$vertrag';

vi.mock('$app/paths', () => ({
	resolve: (p: string, params?: Record<string, string>) => p.replace('[slug]', params?.slug ?? '')
}));
const { default: Page } = await import('./+page.svelte');

function zeige(data: { liste: ArchivListe | null; fehler: string | null }) {
	return render(Page, { data, params: {} } as never);
}

describe('/projekte/archiviert (Entscheidung archiv-status)', () => {
	it('listet archivierte Projekte mit Titel, Link zur Seite und seit wann', () => {
		zeige({
			liste: {
				projekte: [
					{ slug: 'alter-umzug', titel: 'Alter Umzug', seit: '2026-11-01' },
					{ slug: 'ruhepause', titel: 'Ruhepause', seit: null }
				]
			},
			fehler: null
		});
		expect(screen.getByRole('heading', { level: 1, name: 'Archiv' })).toBeInTheDocument();
		const links = screen.getAllByRole('link');
		expect(links.map((a) => [a.textContent, a.getAttribute('href')])).toEqual([
			['Alter Umzug', '/projects/alter-umzug'],
			['Ruhepause', '/projects/ruhepause']
		]);
		expect(screen.getByText('archiviert seit 01.11.2026')).toBeInTheDocument();
		expect(screen.getAllByText(/archiviert seit/)).toHaveLength(1);
	});

	it('leer: ein ruhiger Satz', () => {
		zeige({ liste: { projekte: [] }, fehler: null });
		expect(screen.getByText('Im Archiv liegt nichts.')).toBeInTheDocument();
		expect(screen.queryByRole('link')).toBeNull();
	});

	it('bei einem Fehler: der Satz als Problem', () => {
		zeige({ liste: null, fehler: 'Der Server antwortet nicht.' });
		expect(screen.getByRole('note')).toHaveTextContent('Der Server antwortet nicht.');
	});
});
