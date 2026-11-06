import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import VorgangZeile from './VorgangZeile.svelte';
import type { Vorgang, VorgangDetail } from '$vertrag';

const jetzt = new Date(2026, 9, 26, 12, 0, 0);
const STD = 3_600_000;
const vor = (ms: number) => new Date(jetzt.getTime() - ms).toISOString();

const basis: Vorgang = {
	id: 'v-20261026-0007',
	art: 'antwort',
	slug: 'beispiel',
	titel: 'Der Inhaber hat geantwortet: Schlusssatz',
	bezug: 'antwort:beispiel/schlusssatz',
	zustand: 'offen',
	erstellt: vor(2 * STD),
	geaendert: vor(2 * STD),
	letzte_nr: 1043,
	von: 'browser',
	notiz: null
};

const detail: VorgangDetail = {
	vorgang: basis,
	ereignisse: [
		{
			nr: 1042,
			zeit: new Date(2026, 9, 26, 9, 14).toISOString(),
			vorgang: basis.id,
			art: 'antwort',
			slug: 'beispiel',
			zustand: 'offen',
			titel: basis.titel,
			bezug: basis.bezug,
			von: 'browser',
			notiz: null
		},
		{
			nr: 1043,
			zeit: new Date(2026, 9, 26, 9, 20).toISOString(),
			vorgang: basis.id,
			art: 'antwort',
			slug: 'beispiel',
			zustand: 'uebernommen',
			titel: basis.titel,
			bezug: basis.bezug,
			von: 'chat:beispiel',
			notiz: 'Ich baue den Satz ein.'
		}
	]
};

describe('VorgangZeile', () => {
	it('färbt den Punkt nach Zustand und verlinkt das Projekt', () => {
		const laden = vi.fn(() => Promise.resolve(detail));
		render(VorgangZeile, { vorgang: { ...basis, zustand: 'uebernommen' }, laden, jetzt });
		expect(screen.getByRole('img', { name: 'übernommen' })).toHaveClass('b-laeuft');
		expect(screen.getByText('Antwort')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'beispiel' })).toHaveAttribute(
			'href',
			'/projects/beispiel'
		);
		expect(screen.getByText('übernommen vor 2 Std.')).toBeInTheDocument();
		expect(screen.queryByText(/nicht reagiert/)).not.toBeInTheDocument();
		expect(laden).not.toHaveBeenCalled();
	});
	it('markiert einen alten zugestellten Vorgang rot', () => {
		const laden = vi.fn(() => Promise.resolve(detail));
		const { container } = render(VorgangZeile, {
			vorgang: { ...basis, zustand: 'zugestellt', geaendert: vor(30 * STD) },
			laden,
			jetzt
		});
		expect(screen.getByRole('img', { name: 'zugestellt' })).toHaveClass('b-neutral');
		expect(container.querySelector('li.zeile')).toHaveClass('rot');
		expect(
			screen.getByText('Der Chat hat seit über einem Tag nicht reagiert.')
		).toBeInTheDocument();
	});
	it('klappt auf, lädt einmal und zeigt die Schritte mit Herkunft', async () => {
		const laden = vi.fn((): Promise<VorgangDetail> => Promise.resolve(detail));
		render(VorgangZeile, { vorgang: basis, laden, jetzt });
		const knopf = screen.getByRole('button', { name: /Schlusssatz/ });
		expect(knopf).toHaveAttribute('aria-expanded', 'false');
		const panel = document.getElementById(knopf.getAttribute('aria-controls') ?? '');
		expect(panel).not.toBeNull();
		expect(panel).not.toBeVisible();
		await fireEvent.click(knopf);
		expect(knopf).toHaveAttribute('aria-expanded', 'true');
		expect(panel).toBeVisible();
		expect(laden).toHaveBeenCalledWith('v-20261026-0007');
		expect(await screen.findByText('von Chat beispiel')).toBeInTheDocument();
		expect(screen.getByText('von dir (Browser)')).toBeInTheDocument();
		expect(screen.getByText('09:20 Uhr')).toBeInTheDocument();
		expect(screen.getByText('„Ich baue den Satz ein.“')).toBeInTheDocument();
		await fireEvent.click(knopf);
		expect(knopf).toHaveAttribute('aria-expanded', 'false');
		await fireEvent.click(knopf);
		expect(await screen.findByText('von Chat beispiel')).toBeInTheDocument();
		expect(laden).toHaveBeenCalledTimes(1);
	});

	it('Live-Lücken: aufgeklappt lädt eine neuere Kopfzeile die Schritte selbst nach', async () => {
		const [, zweiter] = detail.ereignisse;
		if (zweiter === undefined) throw new Error('Beispiel ohne zweiten Schritt');
		const neu: VorgangDetail = {
			vorgang: { ...basis, letzte_nr: 1044 },
			ereignisse: [
				...detail.ereignisse,
				{
					...zweiter,
					nr: 1044,
					zustand: 'erledigt',
					notiz: 'Fertig eingebaut.'
				}
			]
		};
		const laden = vi.fn((): Promise<VorgangDetail> => Promise.resolve(detail));
		const { rerender } = render(VorgangZeile, { vorgang: basis, laden, jetzt });
		await fireEvent.click(screen.getByRole('button', { name: /Schlusssatz/ }));
		expect(await screen.findByText('„Ich baue den Satz ein.“')).toBeInTheDocument();
		expect(laden).toHaveBeenCalledTimes(1);
		laden.mockImplementation(() => Promise.resolve(neu));
		await rerender({ vorgang: { ...basis, letzte_nr: 1044 }, laden, jetzt });
		expect(await screen.findByText('„Fertig eingebaut.“')).toBeInTheDocument();
		expect(laden).toHaveBeenCalledTimes(2);
	});
});
