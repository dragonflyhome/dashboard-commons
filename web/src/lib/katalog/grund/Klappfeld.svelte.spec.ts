import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { createRawSnippet, tick } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import Klappfeld from './Klappfeld.svelte';

// Kopfleiste, Fassung 3: das Klappfeld an der Glocke (eigene Specs, Auflage aus der Abnahme).
const quelle = readFileSync(join(import.meta.dirname, 'Klappfeld.svelte'), 'utf8');

const knopf = createRawSnippet(() => ({ render: () => '<span aria-hidden="true">🔔</span>' }));

/** Inhalt mit einem Link und einem Knopf, der `schliessen` ruft. */
const inhalt = createRawSnippet((schliessen: () => () => void) => ({
	render: () => '<div><a href="#ziel">Zur Meldung</a><button type="button">Fertig</button></div>',
	setup: (el: Element) => {
		const b = el.querySelector('button');
		const zu = (): void => {
			schliessen()();
		};
		b?.addEventListener('click', zu);
		return () => b?.removeEventListener('click', zu);
	}
}));

async function ruhe(): Promise<void> {
	for (let i = 0; i < 5; i++) {
		await tick();
		await new Promise((f) => setTimeout(f, 0));
	}
}

/** Klick weit neben dem Feld (jsdom kennt kein Layout: alles 0×0). */
async function danebenKlicken(): Promise<void> {
	const lage = { bubbles: true, cancelable: true, button: 0, clientX: 900, clientY: 600 };
	document.body.dispatchEvent(new MouseEvent('pointerdown', lage));
	await new Promise((f) => setTimeout(f, 30));
	document.body.dispatchEvent(new MouseEvent('pointerup', lage));
	document.body.dispatchEvent(new MouseEvent('click', lage));
}

/**
 * jsdom kennt kein Layout: Floating UI kann das Feld nicht ausmessen, Bits UI
 * lässt es darum `visibility: hidden`. Die Abfragen schließen Verstecktes
 * darum ein (`hidden: true`); wie es im Browser steht, prüft die Edge-Probe.
 */
const auchVersteckt = { hidden: true } as const;

async function oeffnen(onoffen = vi.fn()): Promise<HTMLElement> {
	render(Klappfeld, {
		name: 'Meldungen: 2 ungelesen',
		titel: 'Meldungen',
		knopf,
		children: inhalt,
		onoffen
	});
	const k = screen.getByRole('button', { name: 'Meldungen: 2 ungelesen' });
	await fireEvent.click(k);
	await screen.findByRole('dialog', { name: 'Meldungen', ...auchVersteckt });
	// Nicht länger warten: In jsdom kann das versteckte Feld den Fokus nicht halten,
	// Bits UI schließt es dann nach einer Weile (im Browser bleibt es offen, siehe Edge-Probe).
	return k;
}

describe('Klappfeld (Katalog, Kopfleiste Fassung 3)', () => {
	it('Knopf mit Namen öffnet ein Feld mit Überschrift; ohne Seitensperre', async () => {
		const onoffen = vi.fn();
		const k = await oeffnen(onoffen);
		expect(k).toHaveAttribute('aria-expanded', 'true');
		const feld = screen.getByRole('dialog', { name: 'Meldungen', ...auchVersteckt });
		expect(feld).toHaveTextContent('Zur Meldung');
		expect(onoffen).toHaveBeenLastCalledWith(true);
		expect(document.body.style.pointerEvents).not.toBe('none');
		expect(document.body.style.overflow).not.toBe('hidden');
	});

	it('Escape schließt und meldet es', async () => {
		const onoffen = vi.fn();
		await oeffnen(onoffen);
		await fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
		await waitFor(() => {
			expect(screen.queryByRole('dialog', auchVersteckt)).toBeNull();
		});
		expect(onoffen).toHaveBeenLastCalledWith(false);
	});

	it('`schliessen` aus dem Inhalt schließt (etwa ein Link, der die Seite wechselt)', async () => {
		await oeffnen();
		await fireEvent.click(screen.getByRole('button', { name: 'Fertig', ...auchVersteckt }));
		await waitFor(() => {
			expect(screen.queryByRole('dialog', auchVersteckt)).toBeNull();
		});
	});

	it('ein Klick daneben schließt, die Seite bleibt bedienbar', async () => {
		await oeffnen();
		await danebenKlicken();
		await waitFor(() => {
			expect(screen.queryByRole('dialog', auchVersteckt)).toBeNull();
		});
		await ruhe();
		expect(document.body.style.pointerEvents).not.toBe('none');
	});

	it('nur Token: Eckenradius, Farben und Schatten aus app.css', () => {
		expect(quelle).toMatch(/\.feld\s*\{[^}]*border-radius: var\(--radius\)/);
		expect(quelle).toMatch(/\.feld\s*\{[^}]*box-shadow: var\(--shadow-gross\)/);
		expect(quelle).not.toMatch(/#[0-9a-f]{3,6}\b|rgb\(|hsl\(/i);
	});
});
