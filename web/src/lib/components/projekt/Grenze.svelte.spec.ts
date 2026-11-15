import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type * as Ladefehler from '#lib/ladefehler.ts';
import { zugang } from '#lib/zugang.ts';
import Gut from './probe/Gut.svelte';
import Wirft from './probe/Wirft.svelte';
import WirftWennKaputt from './probe/WirftWennKaputt.svelte';
import { schalter } from './probe/schalter.ts';
import { rollenBeobachten } from './probe/rollen.ts';
import { umgebung, zuruecksetzen } from '#lib/fassung.ts';

// Vitest kennt keine Fassung des Baus; das Neuladen merkt sich, aus welcher es kam.
vi.mock('$app/env', () => ({ version: 'probe', browser: true, building: false, dev: false }));
// Schritt E2b: sonst wie echt; ein Test lässt die Anmeldung ablaufen.
vi.mock('#lib/ladefehler.ts', async (echt) => {
	const m = await echt<typeof Ladefehler>();
	return { ...m, teilFehlt: vi.fn(m.teilFehlt) };
});
const { teilFehlt } = await import('#lib/ladefehler.ts');
const { default: Grenze } = await import('./Grenze.svelte');

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

	it('Schritt E2b: Anmeldung abgelaufen bei offener Eingabe: Hinweis statt Fehlerbild; „neu versuchen“ fragt die Anmeldung neu und lädt noch einmal', async () => {
		vi.mocked(teilFehlt).mockResolvedValueOnce('abgelaufen');
		const auffrischen = vi.spyOn(zugang, 'auffrischen').mockResolvedValue('da');
		const feld = document.body.appendChild(document.createElement('input'));
		feld.focus();
		const { rollen, hinweisDa, ende } = rollenBeobachten();
		let mal = 0;
		render(Grenze, {
			lade: () => {
				mal += 1;
				return mal === 1
					? Promise.reject(new TypeError('Failed to fetch dynamically imported module: x'))
					: Promise.resolve({ default: Gut });
			},
			datei: 'projects/x/Seite.svelte'
		});
		const hinweis = await screen.findByRole('alert');
		// Nach dem Zeichnen des Hinweises das Feld mit dem Fokus ins Bild, der Fokus bleibt.
		await vi.waitFor(() => {
			expect(rollen).toHaveBeenCalledTimes(1);
		});
		ende();
		expect(rollen.mock.contexts[0]).toBe(feld);
		expect(rollen).toHaveBeenCalledWith({ block: 'nearest' });
		expect(hinweisDa).toEqual([true]);
		expect(document.activeElement).toBe(feld);
		feld.remove();
		expect(hinweis).toHaveTextContent('Anmeldung abgelaufen');
		expect(hinweis).not.toHaveTextContent('Diese Seite hat gerade einen Fehler');
		const link = screen.getByRole('link', { name: 'In neuem Tab anmelden' });
		expect(link).toHaveAttribute('target', '_blank');
		expect(link).toHaveAttribute('rel', 'noopener');
		await fireEvent.click(screen.getByRole('button', { name: 'neu versuchen' }));
		expect(auffrischen).toHaveBeenCalledTimes(1);
		expect(await screen.findByText('Seite ist da')).toBeInTheDocument();
		expect(mal).toBe(2);
		auffrischen.mockRestore();
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
