import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import Grenze from './Grenze.svelte';
import Gut from './probe/Gut.svelte';
import Wirft from './probe/Wirft.svelte';
import WirftWennKaputt from './probe/WirftWennKaputt.svelte';
import { schalter } from './probe/schalter.ts';
import { umgebung, zuruecksetzen } from '#lib/fassung.ts';

// Vitest kennt keine Fassung des Baus; das Neuladen merkt sich, aus welcher es kam.
vi.mock('$app/env', () => ({ version: 'probe', browser: true, building: false, dev: false }));

describe('Grenze um eine Projektseite', () => {
	it('zeigt die Seite, wenn sie geht', async () => {
		render(Grenze, {
			lade: () => Promise.resolve({ default: Gut }),
			datei: 'projects/x/Seite.svelte'
		});
		expect(await screen.findByText('Seite ist da')).toBeInTheDocument();
	});

	it('fängt einen Fehler beim Zeichnen ab: nur das Fehlerbild mit Satz, Knopf und Meldung', async () => {
		const fehler = vi.spyOn(console, 'error').mockImplementation(() => undefined);
		render(Grenze, {
			lade: () => Promise.resolve({ default: Wirft }),
			datei: 'projects/x/Seite.svelte'
		});
		expect(
			await screen.findByRole('heading', { name: 'Diese Seite hat gerade einen Fehler' })
		).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'neu versuchen' })).toBeInTheDocument();
		expect(screen.getByText('absichtlich kaputt')).toBeInTheDocument();
		expect(screen.getByText('projects/x/Seite.svelte')).toBeInTheDocument();
		expect(fehler).toHaveBeenCalled();
		fehler.mockRestore();
	});

	it('lässt sich die Seite nicht laden, zeigt sie dasselbe Fehlerbild', async () => {
		render(Grenze, {
			lade: () => Promise.reject(new Error('Teil des Baus fehlt')),
			datei: 'projects/x/Seite.svelte'
		});
		expect(await screen.findByText('Teil des Baus fehlt')).toBeInTheDocument();
		expect(screen.getByText(/ließ sich nicht laden/)).toBeInTheDocument();
	});

	it('Etappe 4b: „neu versuchen“ zeichnet die Seite noch einmal (Fehler beim Zeichnen)', async () => {
		const fehler = vi.spyOn(console, 'error').mockImplementation(() => undefined);
		schalter.kaputt = true;
		render(Grenze, {
			lade: () => Promise.resolve({ default: WirftWennKaputt }),
			datei: 'projects/x/Seite.svelte'
		});
		const knopf = await screen.findByRole('button', { name: 'neu versuchen' });
		expect(screen.getByRole('alert')).toHaveTextContent('Diese Seite hat gerade einen Fehler');
		schalter.kaputt = false;
		await fireEvent.click(knopf);
		expect(await screen.findByText('wieder heil')).toBeInTheDocument();
		expect(screen.queryByRole('alert')).toBeNull();
		fehler.mockRestore();
	});

	it('Etappe 4b: nach einem Ladefehler lädt „neu versuchen“ noch einmal', async () => {
		let mal = 0;
		render(Grenze, {
			lade: () => {
				mal += 1;
				return mal === 1
					? Promise.reject(new Error('Teil des Baus fehlt'))
					: Promise.resolve({ default: Gut });
			},
			datei: 'projects/x/Seite.svelte'
		});
		await fireEvent.click(await screen.findByRole('button', { name: 'neu versuchen' }));
		expect(await screen.findByText('Seite ist da')).toBeInTheDocument();
		expect(mal).toBe(2);
	});

	it('Nachtrag: fehlt nur ein Teil eines alten Baus, lädt die Seite einmal neu statt des Fehlerbilds', async () => {
		const ursprung = { ...umgebung };
		const neuLaden = vi.fn<() => void>();
		umgebung.neuLaden = neuLaden;
		sessionStorage.clear();
		zuruecksetzen();
		const veraltet = new TypeError(
			'Failed to fetch dynamically imported module: http://localhost:8787/_app/immutable/chunks/alt.js'
		);
		try {
			render(Grenze, { lade: () => Promise.reject(veraltet), datei: 'projects/x/Seite.svelte' });
			await vi.waitFor(() => {
				expect(neuLaden).toHaveBeenCalledTimes(1);
			});
			expect(screen.getByText('Seite wird geladen …')).toBeInTheDocument();
			expect(screen.queryByText('Diese Seite hat gerade einen Fehler')).toBeNull();
			// Gleich danach noch einmal (die neue Seite hat denselben Fehler): kein zweites
			// Neuladen, jetzt das Fehlerbild.
			zuruecksetzen();
			render(Grenze, { lade: () => Promise.reject(veraltet), datei: 'projects/y/Seite.svelte' });
			expect(await screen.findByText(/ließ sich nicht laden/)).toBeInTheDocument();
			expect(neuLaden).toHaveBeenCalledTimes(1);
		} finally {
			Object.assign(umgebung, ursprung);
			sessionStorage.clear();
			zuruecksetzen();
		}
	});
});
