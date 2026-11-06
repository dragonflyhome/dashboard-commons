import { render, screen, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import type { ProjectSummary } from '$vertrag';
import { ANSICHT_SCHLUESSEL, ansichtLesen, ansichtMerken, auswerten } from '#lib/projekte.ts';
import ProjektSpalten from './ProjektSpalten.svelte';

function projekt(slug: string, bedeutung: ProjectSummary['bedeutung']): ProjectSummary {
	return {
		slug,
		title: `Projekt ${slug}`,
		status: bedeutung === 'ok' ? 'fertig' : 'aktiv',
		bedeutung,
		last_change: '2026-10-30',
		summary: 'Ein Satz.',
		chat: null,
		backup: null,
		backup_bedeutung: null,
		backup_note: '',
		vorschau: null,
		order: 100,
		open: ['eins'],
		repos: [],
		file_modified: null
	};
}

afterEach(() => {
	localStorage.clear();
});

describe('ProjektSpalten (Etappe 3b)', () => {
	it('je Bedeutung eine benannte Spalte mit ihrer Zahl und Links auf die Projekte', () => {
		const a = auswerten([projekt('a', 'laeuft'), projekt('b', 'laeuft'), projekt('c', 'ok')], {
			suche: '',
			status: 'alle',
			sortierung: 'title'
		});
		render(ProjektSpalten, { spalten: a.spalten });
		const aktiv = screen.getByRole('region', { name: /aktiv/ });
		expect(
			within(aktiv)
				.getAllByRole('link')
				.map((l) => l.getAttribute('href'))
		).toEqual(['/projects/a', '/projects/b']);
		expect(aktiv.querySelector('[data-anzahl]')).toHaveAttribute('data-anzahl', '2');
		expect(screen.getByRole('region', { name: /fertig/ })).toBeInTheDocument();
		expect(screen.getAllByRole('region')).toHaveLength(2);
	});

	it('merkt Kacheln oder Spalten je Browser; Standard Kacheln', () => {
		expect(ansichtLesen()).toBe('kacheln');
		ansichtMerken('spalten');
		expect(localStorage.getItem(ANSICHT_SCHLUESSEL)).toBe('spalten');
		expect(ansichtLesen()).toBe('spalten');
		ansichtMerken('kacheln');
		expect(localStorage.getItem(ANSICHT_SCHLUESSEL)).toBeNull();
	});
});
