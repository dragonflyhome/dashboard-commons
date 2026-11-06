import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { createRawSnippet, tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import Knopf from './Knopf.svelte';
import Schalter from './Schalter.svelte';
import Auswahl from './Auswahl.svelte';
import Dialog from './Dialog.svelte';
import Kurzinfo from './Kurzinfo.svelte';
import Aufklapper from './Aufklapper.svelte';
import Reiter from '../Reiter.svelte';
import Tabelle from '../Tabelle.svelte';

/** Ein Snippet mit festem Text (für children). */
function text(t: string) {
	return createRawSnippet(() => ({ render: () => `<span>${t}</span>` }));
}

/**
 * Breiten für jsdom vortäuschen (es misst nichts) und die Rückrufe der
 * ResizeObserver fangen, damit ein Test eine neue Größe melden kann.
 */
function breiten(scroll: number, client: number) {
	const rueckrufe: (() => void)[] = [];
	vi.stubGlobal(
		'ResizeObserver',
		class {
			constructor(rueckruf: () => void) {
				rueckrufe.push(rueckruf);
			}
			observe(): void {
				// gemeldet wird von Hand über melden()
			}
			unobserve(): void {
				// nichts
			}
			disconnect(): void {
				// nichts
			}
		}
	);
	const sw = vi.spyOn(Element.prototype, 'scrollWidth', 'get').mockReturnValue(scroll);
	const cw = vi.spyOn(Element.prototype, 'clientWidth', 'get').mockReturnValue(client);
	return {
		/** Neue Breiten melden, wie es der Browser beim Größenwechsel täte. */
		async melden(s: number, c: number): Promise<void> {
			sw.mockReturnValue(s);
			cw.mockReturnValue(c);
			for (const f of rueckrufe) f();
			await tick();
		}
	};
}

const tabelleDaten = {
	beschriftung: 'Wo was liegt',
	mindestbreite: 32,
	spalten: [
		{ schluessel: 'thema', titel: 'Thema' },
		{ schluessel: 'ordner', titel: 'Ordner', art: 'mono' as const }
	],
	zeilen: [{ thema: 'Steuer', ordner: 'D:\\Ablage\\Steuer' }]
};

afterEach(() => {
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

describe('Grundteile (Bits UI)', () => {
	it('Knopf: klickbar; während etwas läuft gesperrt mit Satz und aria-busy', async () => {
		const klick = vi.fn();
		const r = render(Knopf, { art: 'haupt', onclick: klick, children: text('Speichern') });
		const k = screen.getByRole('button', { name: 'Speichern' });
		expect(k).toHaveClass('primary');
		await fireEvent.click(k);
		expect(klick).toHaveBeenCalledOnce();
		await r.rerender({ laeuft: 'wird gespeichert' });
		const k2 = screen.getByRole('button', { name: 'wird gespeichert' });
		expect(k2).toHaveAttribute('aria-disabled', 'true');
		expect(k2).toHaveAttribute('aria-busy', 'true');
	});

	// jsdom nimmt auch einem disabled-Knopf den Fokus nicht; toHaveFocus allein
	// belegt hier also wenig. Was zählt, ist not.toBeDisabled (aria-disabled
	// statt disabled). Den echten Beleg für den Fokus liefert die Edge-Probe
	// (tests/skripte/katalog-edge.mjs).
	it('Knopf: während etwas läuft, bleibt der Fokus auf ihm und der Klick tut nichts', async () => {
		const klick = vi.fn();
		const r = render(Knopf, { onclick: klick, children: text('Speichern') });
		const k = screen.getByRole('button', { name: 'Speichern' });
		k.focus();
		await r.rerender({ laeuft: 'wird gespeichert' });
		expect(k).toHaveFocus();
		expect(k).not.toBeDisabled();
		await fireEvent.click(k);
		expect(klick).not.toHaveBeenCalled();
		await r.rerender({ laeuft: null });
		await fireEvent.click(k);
		expect(klick).toHaveBeenCalledOnce();
	});

	it('Schalter: Rolle switch mit Namen, Leertaste und Klick schalten, Hinweis verbunden', async () => {
		const wechsel = vi.fn();
		render(Schalter, {
			beschriftung: 'Nur offene zeigen',
			hinweis: 'Erledigte bleiben gespeichert.',
			onwechsel: wechsel
		});
		const s = screen.getByRole('switch', { name: 'Nur offene zeigen' });
		expect(s).toHaveAttribute('aria-checked', 'false');
		expect(s).toHaveAccessibleDescription('Erledigte bleiben gespeichert.');
		await fireEvent.click(s);
		expect(s).toHaveAttribute('aria-checked', 'true');
		expect(wechsel).toHaveBeenLastCalledWith(true);
		s.focus();
		await fireEvent.keyDown(s, { key: ' ' });
		await waitFor(() => {
			expect(s).toHaveAttribute('aria-checked', 'false');
		});
	});

	it('Auswahl: Knopf mit Beschriftung und gewähltem Wert, aria-haspopup', () => {
		render(Auswahl, {
			beschriftung: 'Zeitraum',
			wert: '30',
			optionen: [
				{ wert: '7', text: '7 Tage' },
				{ wert: '30', text: '30 Tage' }
			]
		});
		const k = screen.getByRole('combobox', { name: 'Zeitraum 30 Tage' });
		expect(k).toHaveAttribute('aria-expanded', 'false');
	});

	// Dass der Fokus danach wieder auf dem Knopf steht, prüft die Edge-Probe
	// (tests/skripte/katalog-edge.mjs); jsdom gibt ihn nicht zuverlässig zurück.
	it('Dialog: öffnet mit Titel und Beschreibung, Escape schließt', async () => {
		render(Dialog, {
			titel: 'Wirklich löschen?',
			beschreibung: 'Das lässt sich nicht zurückholen.',
			ausloeser: 'Löschen',
			children: text('Inhalt')
		});
		const knopf = screen.getByRole('button', { name: 'Löschen' });
		await fireEvent.click(knopf);
		const d = await screen.findByRole('dialog', { name: 'Wirklich löschen?' });
		expect(d).toHaveAccessibleDescription('Das lässt sich nicht zurückholen.');
		expect(within(d).getByRole('button', { name: 'Schließen' })).toBeInTheDocument();
		await fireEvent.keyDown(document.activeElement ?? d, { key: 'Escape' });
		await waitFor(() => {
			expect(screen.queryByRole('dialog')).toBeNull();
		});
	});

	it('Kurzinfo: der Auslöser ist ein Knopf mit dem Wort', () => {
		render(Kurzinfo, { text: 'Zählt nur Einträge mit Frist', children: text('Fristen') });
		expect(screen.getByRole('button', { name: 'Fristen' })).toBeInTheDocument();
	});

	it('Kurzinfo: Name für ⓘ, offen zeigt aria-describedby auf die Blase mit role=tooltip', async () => {
		render(Kurzinfo, {
			text: 'Zählt nur Einträge mit Frist',
			name: 'Mehr zu Fristen',
			offen: true,
			children: text('ⓘ')
		});
		const k = screen.getByRole('button', { name: 'Mehr zu Fristen' });
		const blase = await screen.findByRole('tooltip');
		expect(blase).toHaveTextContent('Zählt nur Einträge mit Frist');
		expect(k.getAttribute('aria-describedby')).toBe(blase.id);
		expect(k).toHaveAccessibleDescription('Zählt nur Einträge mit Frist');
	});

	it('Aufklapper: aria-expanded, Inhalt erst nach dem Aufklappen im Dokument', async () => {
		render(Aufklapper, { titel: 'Ältere Einträge', children: text('Alt') });
		const kopf = screen.getByRole('button', { name: 'Ältere Einträge' });
		expect(kopf).toHaveAttribute('aria-expanded', 'false');
		expect(screen.queryByText('Alt')).toBeNull();
		await fireEvent.click(kopf);
		expect(kopf).toHaveAttribute('aria-expanded', 'true');
		expect(screen.getByText('Alt')).toBeInTheDocument();
	});

	it('Aufklapper: aria-controls nur, solange der Inhalt im Dokument steht', async () => {
		render(Aufklapper, { titel: 'Ältere Einträge', children: text('Alt') });
		const kopf = screen.getByRole('button', { name: 'Ältere Einträge' });
		expect(kopf).not.toHaveAttribute('aria-controls');
		await fireEvent.click(kopf);
		const ziel = kopf.getAttribute('aria-controls');
		expect(ziel).toBeTruthy();
		expect(document.getElementById(ziel ?? '')).toHaveTextContent('Alt');
		await fireEvent.click(kopf);
		expect(kopf).not.toHaveAttribute('aria-controls');
	});

	it('Reiter: Rollen tab und tabpanel, Pfeiltasten wechseln im Kreis', async () => {
		const inhalt = createRawSnippet((i: () => number) => ({
			render: () => `<p>Inhalt ${String(i())}</p>`
		}));
		render(Reiter, { titel: ['Plan', 'Stand', 'Notizen'], children: inhalt });
		const tabs = screen.getAllByRole('tab');
		expect(tabs.map((t) => t.getAttribute('aria-selected'))).toEqual(['true', 'false', 'false']);
		expect(screen.getByRole('tabpanel')).toHaveTextContent('Inhalt 0');
		tabs[0]?.focus();
		await fireEvent.keyDown(tabs[0] ?? document.body, { key: 'ArrowLeft' });
		await waitFor(() => {
			expect(screen.getByRole('tabpanel')).toHaveTextContent('Inhalt 2');
		});
		expect(tabs[2]).toHaveFocus();
		await fireEvent.keyDown(tabs[2] ?? document.body, { key: 'ArrowRight' });
		await waitFor(() => {
			expect(screen.getByRole('tabpanel')).toHaveTextContent('Inhalt 0');
		});
	});

	it('Reiter: aria-controls nur am gewählten Reiter, und es zeigt auf den Inhalt', async () => {
		const inhalt = createRawSnippet((i: () => number) => ({
			render: () => `<p>Inhalt ${String(i())}</p>`
		}));
		render(Reiter, { titel: ['Plan', 'Stand', 'Notizen'], children: inhalt });
		const pruefen = (gewaehlt: number): void => {
			screen.getAllByRole('tab').forEach((t, i) => {
				if (i === gewaehlt) {
					const ziel = document.getElementById(t.getAttribute('aria-controls') ?? '');
					expect(ziel).toBe(screen.getByRole('tabpanel'));
				} else {
					expect(t).not.toHaveAttribute('aria-controls');
				}
			});
		};
		pruefen(0);
		const tabs = screen.getAllByRole('tab');
		await fireEvent.keyDown(tabs[0] ?? document.body, { key: 'ArrowRight' });
		await waitFor(() => {
			expect(screen.getByRole('tabpanel')).toHaveTextContent('Inhalt 1');
		});
		pruefen(1);
	});

	it('Reiter: wird die Liste kürzer als der gewählte Reiter, gilt der erste', async () => {
		const inhalt = createRawSnippet((i: () => number) => ({
			render: () => `<p>Inhalt ${String(i())}</p>`
		}));
		const r = render(Reiter, { titel: ['Plan', 'Stand', 'Notizen'], children: inhalt });
		const tabs = screen.getAllByRole('tab');
		await fireEvent.keyDown(tabs[0] ?? document.body, { key: 'ArrowLeft' });
		await waitFor(() => {
			expect(screen.getByRole('tabpanel')).toHaveTextContent('Inhalt 2');
		});
		await r.rerender({ titel: ['Plan', 'Stand'] });
		await waitFor(() => {
			expect(screen.getByRole('tabpanel')).toHaveTextContent('Inhalt 0');
		});
		expect(screen.getAllByRole('tab').map((t) => t.getAttribute('aria-selected'))).toEqual([
			'true',
			'false'
		]);
	});

	it('Tabelle: rollt sie, ist der Rahmen ein Bereich mit Namen und Tab-Stopp', () => {
		breiten(800, 300);
		render(Tabelle, tabelleDaten);
		const rahmen = screen.getByRole('region', { name: 'Wo was liegt' });
		expect(rahmen).toHaveAttribute('tabindex', '0');
		expect(rahmen).toHaveClass('scroll-x');
		const tabelle = within(rahmen).getByRole('table', { name: 'Wo was liegt' });
		expect(
			within(tabelle)
				.getAllByRole('columnheader')
				.map((c) => c.textContent)
		).toEqual(['Thema', 'Ordner']);
		expect(within(tabelle).getByText('D:\\Ablage\\Steuer')).toHaveClass('mono');
	});

	it('Tabelle: passt sie, gibt es keinen Tab-Stopp und keinen Bereich', () => {
		breiten(300, 300);
		render(Tabelle, tabelleDaten);
		expect(screen.queryByRole('region')).toBeNull();
		const rahmen = screen.getByRole('table', { name: 'Wo was liegt' }).parentElement;
		expect(rahmen).toHaveClass('scroll-x');
		expect(rahmen).not.toHaveAttribute('tabindex');
		expect(rahmen).not.toHaveAttribute('aria-labelledby');
	});

	it('Tabelle: wird das Fenster schmal oder wieder breit, folgt der Tab-Stopp', async () => {
		const b = breiten(300, 300);
		render(Tabelle, tabelleDaten);
		const rahmen = screen.getByRole('table', { name: 'Wo was liegt' }).parentElement;
		expect(rahmen).not.toHaveAttribute('tabindex');
		await b.melden(800, 300);
		expect(rahmen).toHaveAttribute('tabindex', '0');
		expect(screen.getByRole('region', { name: 'Wo was liegt' })).toBe(rahmen);
		await b.melden(300, 300);
		expect(rahmen).not.toHaveAttribute('tabindex');
		expect(screen.queryByRole('region')).toBeNull();
	});

	it('Tabelle ohne Zeilen: nur der Satz', () => {
		render(Tabelle, { beschriftung: 'Leer', spalten: [], zeilen: [], leer: 'Nichts da.' });
		expect(screen.getByText('Nichts da.')).toBeInTheDocument();
		expect(screen.queryByRole('table')).toBeNull();
	});
});
