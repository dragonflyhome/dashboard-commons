import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { createRawSnippet, tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import Schublade from './Schublade.svelte';

// Kopfleiste, Fassung 3: die Seitenleiste am Handy. Lehre aus dem Menü-Hotfix:
// keine Sperre an <body>, Escape schließt und gibt den Fokus zurück.
const inhalt = createRawSnippet(() => ({ render: () => '<a href="#x">Ein Link</a>' }));

/**
 * Ein Klick auf den Schleier neben der Schublade. Bits UI prüft die Lage des
 * Zeigers gegen die Schublade; jsdom kennt kein Layout (alles 0×0), darum ein
 * Punkt weit rechts davon.
 */
async function danebenKlicken(ziel: Element): Promise<void> {
	const lage = { bubbles: true, cancelable: true, button: 0, clientX: 900, clientY: 300 };
	ziel.dispatchEvent(new MouseEvent('pointerdown', lage));
	// Bits UI wertet „daneben“ nach 10 ms aus.
	await new Promise((f) => setTimeout(f, 30));
	ziel.dispatchEvent(new MouseEvent('pointerup', lage));
	ziel.dispatchEvent(new MouseEvent('click', lage));
}

async function ruhe(): Promise<void> {
	for (let i = 0; i < 5; i++) {
		await tick();
		await new Promise((f) => setTimeout(f, 0));
	}
}

describe('Schublade (Katalog, Kopfleiste Fassung 3)', () => {
	it('Knopf mit Namen öffnet; offen keine Seitensperre; Escape schließt', async () => {
		render(Schublade, { beschriftung: 'Seiten und Projekte', titel: 'Seiten', children: inhalt });
		const knopf = screen.getByRole('button', { name: 'Seiten und Projekte' });
		await fireEvent.click(knopf);
		const fenster = await screen.findByRole('dialog');
		expect(fenster).toHaveTextContent('Ein Link');
		await ruhe();
		expect(document.body.style.pointerEvents).not.toBe('none');
		expect(document.body.style.overflow).not.toBe('hidden');
		await fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
		await waitFor(() => {
			expect(screen.queryByRole('dialog')).toBeNull();
		});
		await ruhe();
		expect(document.body.style.pointerEvents).not.toBe('none');
		// Den Fokus zurück auf den Knopf prüft die Edge-Probe (jsdom setzt ihn nach Bits UI nicht).
	});

	it('von außen geschlossen (Seitenwechsel): weg, ohne Sperre', async () => {
		const { rerender } = render(Schublade, {
			beschriftung: 'Seiten und Projekte',
			titel: 'Seiten',
			offen: true,
			children: inhalt
		});
		await screen.findByRole('dialog');
		await rerender({ offen: false });
		await waitFor(() => {
			expect(screen.queryByRole('dialog')).toBeNull();
		});
		await ruhe();
		expect(document.body.style.pointerEvents).not.toBe('none');
		expect(document.body.style.overflow).not.toBe('hidden');
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it('beim Vergrößern geschlossen (Knopf ausgeblendet): Fokus auf den Inhalt, nie auf den versteckten Knopf', async () => {
		const main = document.createElement('main');
		main.tabIndex = -1;
		document.body.append(main);
		const { rerender } = render(Schublade, {
			beschriftung: 'Seiten und Projekte',
			titel: 'Seiten',
			ersatz: () => main,
			children: inhalt
		});
		const knopf = screen.getByRole('button', { name: 'Seiten und Projekte' });
		await fireEvent.click(knopf);
		await screen.findByRole('dialog');
		await ruhe();
		// jsdom kennt kein Layout: getClientRects ist leer wie bei einem ausgeblendeten Knopf.
		expect(knopf.getClientRects()).toHaveLength(0);
		await rerender({ offen: false });
		await waitFor(() => {
			expect(screen.queryByRole('dialog')).toBeNull();
		});
		await ruhe();
		expect(document.activeElement).toBe(main);
		main.remove();
	});

	it('Knopf sichtbar: der Fokus geht wie bisher zurück (nicht auf den Inhalt)', async () => {
		const main = document.createElement('main');
		main.tabIndex = -1;
		document.body.append(main);
		const ersatz = vi.fn(() => main);
		const { rerender } = render(Schublade, {
			beschriftung: 'Seiten und Projekte',
			titel: 'Seiten',
			ersatz,
			children: inhalt
		});
		const knopf = screen.getByRole('button', { name: 'Seiten und Projekte' });
		vi.spyOn(knopf, 'getClientRects').mockReturnValue([new DOMRect(0, 0, 44, 44)] as never);
		await fireEvent.click(knopf);
		await screen.findByRole('dialog');
		await ruhe();
		await rerender({ offen: false });
		await waitFor(() => {
			expect(screen.queryByRole('dialog')).toBeNull();
		});
		await ruhe();
		expect(ersatz).not.toHaveBeenCalled();
		expect(document.activeElement).not.toBe(main);
		main.remove();
	});

	it('Klick daneben (auf den Schleier) schließt, ohne Sperre', async () => {
		render(Schublade, { beschriftung: 'Seiten und Projekte', titel: 'Seiten', children: inhalt });
		await fireEvent.click(screen.getByRole('button', { name: 'Seiten und Projekte' }));
		await screen.findByRole('dialog');
		// Nach der Schonfrist für Doppelklicks.
		await new Promise((f) => setTimeout(f, 450));
		await ruhe();
		const schleier = document.querySelector('.schleier');
		if (schleier === null) throw new Error('kein Schleier');
		await danebenKlicken(schleier);
		await waitFor(() => {
			expect(screen.queryByRole('dialog')).toBeNull();
		});
		await ruhe();
		expect(document.body.style.pointerEvents).not.toBe('none');
		expect(document.body.style.overflow).not.toBe('hidden');
	});

	it('Doppelklick auf den Knopf: öffnet und bleibt offen (der zweite Klick trifft den Schleier)', async () => {
		render(Schublade, { beschriftung: 'Seiten und Projekte', titel: 'Seiten', children: inhalt });
		await fireEvent.click(screen.getByRole('button', { name: 'Seiten und Projekte' }));
		await screen.findByRole('dialog');
		await ruhe();
		const schleier = document.querySelector('.schleier');
		if (schleier === null) throw new Error('kein Schleier');
		await danebenKlicken(schleier);
		await fireEvent.dblClick(schleier);
		await ruhe();
		expect(screen.getByRole('dialog')).toBeInTheDocument();
		expect(document.body.style.pointerEvents).not.toBe('none');
	});
});
