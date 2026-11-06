/**
 * Prüfbericht, Funde der Oberfläche (Komponenten): Stand
 * eines archivierten Projekts, Aufgaben der Startseite gekürzt, leerer
 * Abschnitt „Keine Fragen“, Inhaltsverzeichnis langer Seiten der neuen Form.
 * Erfundene Beispiele.
 */
import { render, screen } from '@testing-library/svelte';
import { createRawSnippet, flushSync, tick } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import Stand from '$katalog/widgets/Stand.svelte';
import Abschnitt from '$katalog/Abschnitt.svelte';
import { nurLeeres } from '$katalog/leer.ts';
import AufgabenKasten from '#lib/components/aufgaben/AufgabenKasten.svelte';
import FragenListe from '#lib/components/projekt/FragenListe.svelte';
import AbschnittVerzeichnis from '#lib/components/projekt/AbschnittVerzeichnis.svelte';
import { abschnitteLesen } from '#lib/components/projekt/abschnitte.ts';
import { SATZ_ARCHIV_RUHIG } from '#lib/anstehend.ts';
import { beispielStand } from '#lib/anstehend.beispiel.ts';
import type { AufgabeAnsicht } from '$vertrag';

describe('Stand eines archivierten Projekts', () => {
	it('ohne Ampel und ohne Farbe, mit dem ruhigen Satz', () => {
		const s = beispielStand({
			gesamt: {
				...beispielStand().gesamt,
				status: 'archiviert',
				ampel: 'unbekannt',
				grund: SATZ_ARCHIV_RUHIG,
				ruhig: true
			}
		});
		const { container } = render(Stand, { gesamt: s.gesamt, schwerpunkt: s.schwerpunkt });
		expect(screen.getByText(SATZ_ARCHIV_RUHIG)).toBeInTheDocument();
		expect(screen.queryByLabelText(/^Ampel/)).toBeNull();
		const karte = container.querySelector('.stand');
		expect(karte?.className).toContain('ruhig');
		expect(karte?.className).not.toMatch(/problem|wartet|ok\b/);
	});
	it('aktiv und rot: Ampel wie bisher', () => {
		const s = beispielStand({
			gesamt: { ...beispielStand().gesamt, ampel: 'rot', grund: 'Ein Fehler ist offen.' }
		});
		render(Stand, { gesamt: s.gesamt, schwerpunkt: s.schwerpunkt });
		expect(screen.getByLabelText('Ampel rot')).toBeInTheDocument();
	});
});

function aufgabe(n: number): AufgabeAnsicht {
	return {
		id: `a-20261102-000${String(n)}`,
		projekt: 'beispiel',
		von: 'chat:beispiel',
		titel: `Aufgabe ${String(n)}`,
		knopf: null,
		frist: null,
		ablauf: null,
		zustand: 'offen',
		antwort: null,
		eigene: null,
		erstellt: '2026-11-02T08:00:00+01:00',
		geaendert: '2026-11-02T08:00:00+01:00',
		hat_link: false,
		vorgang: null,
		link: null,
		frist_stufe: null
	};
}

describe('Aufgaben auf der Startseite: die ersten offen, der Rest zum Aufklappen', () => {
	it('mit `hoechstens` 5 von 8 sichtbar, drei unter „und 3 weitere Aufgaben“; die Zahl bleibt 8', () => {
		const { container } = render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [1, 2, 3, 4, 5, 6, 7, 8].map(aufgabe),
			fehler: null,
			hoechstens: 5,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(screen.getByRole('heading', { name: 'Für dich 8 offene Aufgaben' })).toBeInTheDocument();
		const mehr = container.querySelector('details.mehr');
		expect(mehr).not.toBeNull();
		expect(mehr?.querySelector('summary')?.textContent.trim()).toBe('und 3 weitere Aufgaben');
		expect(mehr?.querySelectorAll('li.aufgabe')).toHaveLength(3);
		// Jeder Anker bleibt da (Ziele aus „Anstehend“).
		expect(container.querySelectorAll('li.aufgabe[id^="aufgabe-"]')).toHaveLength(8);
	});
	it('ohne `hoechstens` alle offen, kein Aufklapper', () => {
		const { container } = render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [1, 2, 3, 4, 5, 6, 7, 8].map(aufgabe),
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(container.querySelector('details.mehr')).toBeNull();
	});
});

describe('Leerer Abschnitt „Keine Fragen“', () => {
	it('nurLeeres: nur Elemente mit data-leer', () => {
		const div = document.createElement('div');
		expect(nurLeeres(div)).toBe(false);
		div.innerHTML = '<!-- Anker --><p data-leer>Keine Fragen.</p> ';
		expect(nurLeeres(div)).toBe(true);
		div.innerHTML = '<p data-leer>Keine Fragen.</p><p>Etwas anderes</p>';
		expect(nurLeeres(div)).toBe(false);
		div.innerHTML = '<p data-leer>Keine Fragen.</p>Text';
		expect(nurLeeres(div)).toBe(false);
	});

	it('Abschnitt mit nur „Keine Fragen.“ ist versteckt, mit einer Frage sichtbar', async () => {
		const leer = render(Abschnitt, {
			titel: 'Frage an dich',
			children: createRawSnippet(() => ({ render: () => '<p data-leer>Keine Fragen.</p>' }))
		});
		await tick();
		expect(leer.container.querySelector('section.abschnitt')).toHaveAttribute('hidden');
		const voll = render(Abschnitt, {
			titel: 'Etwas',
			children: createRawSnippet(() => ({ render: () => '<p>Inhalt</p>' }))
		});
		await tick();
		expect(voll.container.querySelector('section.abschnitt')).not.toHaveAttribute('hidden');
	});

	it('FragenListe ohne Fragen markiert ihren Satz als leer', () => {
		const { container } = render(FragenListe, { fragen: [] });
		expect(container.querySelector('[data-leer]')?.textContent).toBe('Keine Fragen.');
	});
});

describe('Inhaltsverzeichnis langer Seiten der neuen Form', () => {
	function seite(n: number, versteckt: number[] = []): HTMLElement {
		const el = document.createElement('div');
		el.innerHTML = Array.from(
			{ length: n },
			(_, i) =>
				`<section class="card abschnitt"${versteckt.includes(i) ? ' hidden' : ''}><h2 id="s${String(i)}-titel">Teil ${String(i + 1)}</h2><p>x</p></section>`
		).join('');
		document.body.append(el);
		return el;
	}

	it('abschnitteLesen: sichtbare Abschnitte mit Kennung und Titel', () => {
		const el = seite(3, [1]);
		expect(abschnitteLesen(el)).toEqual([
			{ id: 's0-titel', titel: 'Teil 1' },
			{ id: 's2-titel', titel: 'Teil 3' }
		]);
		el.remove();
	});

	it('ab sechs Abschnitten ein Verzeichnis mit Sprunglinks, darunter keins', async () => {
		const lang = seite(7);
		render(AbschnittVerzeichnis, { ziel: lang });
		flushSync();
		const nav = screen.getByRole('navigation', { name: 'Inhalt dieser Seite' });
		const links = nav.querySelectorAll('a');
		expect(links).toHaveLength(7);
		expect(links[0]?.getAttribute('href')).toBe('#s0-titel');
		expect(screen.getByText('Inhalt (7 Abschnitte)')).toBeInTheDocument();
		lang.remove();

		const kurz = seite(3);
		const r = render(AbschnittVerzeichnis, { ziel: kurz });
		flushSync();
		expect(r.container.querySelector('nav')).toBeNull();
		// Kommen Abschnitte nach dem Laden dazu, erscheint es.
		for (let i = 3; i < 6; i++) {
			kurz.insertAdjacentHTML(
				'beforeend',
				`<section class="card abschnitt"><h2 id="n${String(i)}-titel">Neu ${String(i)}</h2></section>`
			);
		}
		await vi.waitFor(() => {
			expect(r.container.querySelectorAll('nav a')).toHaveLength(6);
		});
		kurz.remove();
	});
});
