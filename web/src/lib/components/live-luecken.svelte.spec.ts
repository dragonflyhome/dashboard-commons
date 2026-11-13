import { render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { api } from '#lib/api/client.ts';
import { freierKanal } from '#lib/freikanal.svelte.ts';
import { live, type InhaltAenderung } from '#lib/live.svelte.ts';
import { NACHLADEN_FENSTER_MS } from '#lib/nachlader.ts';
import type { LiveBereich, PasskeyListe, ProjektAntwort, PruefungProjekt } from '$vertrag';

// Live-Lücken, Punkt 2: kleine Bausteine an vorhandene Ereignisse gehängt.
// Jeder Test: Änderung am Server (ein Ereignis), der Baustein holt neu.

vi.mock('$app/paths', () => ({ resolve: (p: string) => p, asset: (p: string) => `/${p}` }));

const { default: PasskeyVerwaltung } = await import('./passkey/PasskeyVerwaltung.svelte');
const { default: Standardaufbau } = await import('./projekt/Standardaufbau.svelte');

/** Fängt die Hörer einer Methode `on…` ab, damit der Test Ereignisse auslösen kann. */
function abfangen<T>(ziel: object, methode: string): T[] {
	const liste: T[] = [];
	vi.spyOn(ziel as Record<string, (l: T) => () => void>, methode).mockImplementation((l: T) => {
		liste.push(l);
		return () => {
			const i = liste.indexOf(l);
			if (i >= 0) liste.splice(i, 1);
		};
	});
	return liste;
}

afterEach(() => {
	vi.restoreAllMocks();
});

describe('PasskeyVerwaltung', () => {
	const leer: PasskeyListe = { passkeys: [], eigener: null } as unknown as PasskeyListe;

	it('lädt bei nachholen und bei speicher/passkeys neu, nicht bei anderen Bereichen', async () => {
		const nachholen = abfangen<() => void>(live, 'onNachholen');
		const bereich = abfangen<(b: LiveBereich) => void>(live, 'onBereich');
		abfangen<() => void>(live, 'onVorgang');
		const laden = vi.fn(() => Promise.resolve(leer));
		render(PasskeyVerwaltung, { laden });
		await waitFor(() => {
			expect(laden).toHaveBeenCalledTimes(1);
		});
		for (const l of bereich) l('aufgaben');
		await tick();
		expect(laden).toHaveBeenCalledTimes(1);
		for (const l of bereich) l('passkeys');
		await waitFor(() => {
			expect(laden).toHaveBeenCalledTimes(2);
		});
		for (const l of nachholen) l();
		await waitFor(() => {
			expect(laden).toHaveBeenCalledTimes(3);
		});
	});

	it('vorgang und speicher/passkeys innerhalb von 250 ms: eine Ladung', async () => {
		const bereich = abfangen<(b: LiveBereich) => void>(live, 'onBereich');
		const vorgang = abfangen<() => void>(live, 'onVorgang');
		abfangen<() => void>(live, 'onNachholen');
		const laden = vi.fn(() => Promise.resolve(leer));
		render(PasskeyVerwaltung, { laden });
		await waitFor(() => {
			expect(laden).toHaveBeenCalledTimes(1);
		});
		vi.useFakeTimers();
		try {
			for (const l of vorgang) l();
			vi.advanceTimersByTime(250);
			for (const l of bereich) l('passkeys');
			vi.advanceTimersByTime(NACHLADEN_FENSTER_MS * 2);
			expect(laden).toHaveBeenCalledTimes(2);
			// Ein Vorgang ohne passkeys lädt nach dem Fenster trotzdem, einmal.
			for (const l of vorgang) l();
			for (const l of vorgang) l();
			vi.advanceTimersByTime(NACHLADEN_FENSTER_MS);
			expect(laden).toHaveBeenCalledTimes(3);
		} finally {
			vi.useRealTimers();
		}
	});
});

describe('Standardaufbau: Prüfbericht', () => {
	const kopf = {
		slug: 'alpha',
		vorschau: false,
		kopf: { open: [], links: [] },
		daten: []
	} as unknown as ProjektAntwort;
	const bericht = (text: string | null): PruefungProjekt =>
		({
			befunde:
				text === null
					? []
					: [{ schwere: 'fehler', datei: 'Seite.svelte', zeile: 3, text, regel: 'x' }]
		}) as unknown as PruefungProjekt;
	const aenderung = (slug: string): InhaltAenderung => ({
		projekte: [slug],
		daten: [],
		version: 2,
		sammel: false
	});

	it('holt den Bericht bei projekt zum Slug (beide Kanäle) und nach einer Fassung neu', async () => {
		const geschuetzt = abfangen<(a: InhaltAenderung) => void>(live, 'onInhalt');
		const frei = abfangen<(a: InhaltAenderung) => void>(freierKanal, 'onInhalt');
		const fassung = abfangen<() => void>(freierKanal, 'onFassung');
		abfangen<() => void>(live, 'onNachholen');
		const pruefung = vi.spyOn(api, 'pruefung').mockResolvedValue(bericht(null));
		const { container } = render(Standardaufbau, { kopf });
		await waitFor(() => {
			expect(pruefung).toHaveBeenCalledTimes(1);
		});
		expect(screen.getByText(/hat noch keine eigene Seite/)).toBeInTheDocument();
		// Eigenes Merkmal für die Proben (pruefer-edge), nicht die Klasse kopf.
		expect(container.querySelector('[data-standardaufbau]')).toContainElement(
			screen.getByText(/hat noch keine eigene Seite/)
		);
		// Ein anderes Projekt: nichts.
		for (const l of geschuetzt) l(aenderung('beta'));
		await tick();
		expect(pruefung).toHaveBeenCalledTimes(1);
		// Seite.svelte bekommt einen Fehler (Git-Stand neu, also projekt alpha).
		pruefung.mockResolvedValue(bericht('Unbekannter Baustein.'));
		for (const l of geschuetzt) l(aenderung('alpha'));
		expect(await screen.findByText(/Unbekannter Baustein/)).toBeInTheDocument();
		for (const l of frei) l(aenderung('alpha'));
		for (const l of fassung) l();
		await waitFor(() => {
			expect(pruefung).toHaveBeenCalledTimes(4);
		});
	});

	it('Etappe B8: fristen.toml und plan.toml stehen im Rahmen (Zeitleiste, Plan), nicht noch einmal hier', () => {
		abfangen<(a: InhaltAenderung) => void>(live, 'onInhalt');
		abfangen<(a: InhaltAenderung) => void>(freierKanal, 'onInhalt');
		abfangen<() => void>(freierKanal, 'onFassung');
		abfangen<() => void>(live, 'onNachholen');
		vi.spyOn(api, 'pruefung').mockResolvedValue(bericht(null));
		// Ohne Projektseite drumherum lädt StandardDatei nichts; darum nur diese beiden Namen.
		render(Standardaufbau, { kopf: { ...kopf, daten: ['fristen', 'plan'] } });
		expect(screen.getByText(/hat noch keine eigene Seite/)).toBeInTheDocument();
		expect(screen.queryByRole('heading', { name: 'Fristen' })).toBeNull();
		expect(screen.queryByRole('heading', { name: /plan/ })).toBeNull();
	});
});
