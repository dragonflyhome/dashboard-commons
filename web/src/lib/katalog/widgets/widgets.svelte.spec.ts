import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import { beispielStand, eintrag } from '#lib/anstehend.beispiel.ts';
import { SATZ_OHNE_ANMELDUNG } from '#lib/anstehend.ts';
import type { AnstehendEintrag, Aktualisierung } from '$vertrag';
import Stand from './Stand.svelte';
import Frage from './Frage.svelte';
import Arbeiten from './Arbeiten.svelte';
import Fristen from './Fristen.svelte';
import Zuletzt from './Zuletzt.svelte';
import { fristBedeutung, nachFrist, wartetAufWort } from './worte.ts';

const frage = (id: string, titel: string): AnstehendEintrag =>
	eintrag({ art: 'frage', id, titel, gestellt: '2026-10-28' });

describe('Widgets (A4) mit Beispieldaten', () => {
	it('Stand: Ampel mit Namen, Satz, nur Zahlen über null, Rand in der Farbe der Ampel', () => {
		const s = beispielStand({
			gesamt: { ...beispielStand().gesamt, ampel: 'gelb', grund: '3 Einträge warten auf dich.' },
			kurz: {
				wartet_auf_dich: 3,
				laeuft: 1,
				als_naechstes: 0,
				naechste_frist: null,
				tage_bis: null,
				frist_stufe: null,
				frist_bald_tage: 7
			}
		});
		const { container } = render(Stand, {
			gesamt: s.gesamt,
			schwerpunkt: s.schwerpunkt,
			kurz: s.kurz
		});
		expect(screen.getByRole('img', { name: 'Ampel gelb' })).toBeInTheDocument();
		expect(screen.getByText('3 Einträge warten auf dich.')).toBeInTheDocument();
		const zahlen = within(screen.getByRole('list')).getAllByRole('listitem');
		expect(zahlen.map((z) => z.textContent.trim())).toEqual(['3 wartet auf dich', '1 läuft']);
		expect(container.querySelector('.karte')).toHaveClass('b-wartet');
		expect(screen.queryByText(/Datenspeicher/)).toBeNull();
	});

	it('Frage: die erste offen mit Antwort, die anderen unter „noch N warten“, dann Aufgaben', () => {
		const antwort = createRawSnippet((e: () => AnstehendEintrag) => ({
			render: () => `<p>Antwort auf ${e().titel}</p>`
		}));
		render(Frage, {
			zahl: 4,
			fragen: [frage('f1', 'Erste?'), frage('f2', 'Zweite?')],
			aufgaben: [eintrag({ id: 'a1', titel: 'Beleg hochladen', frist: '2026-11-02', tage_bis: 3 })],
			antwort
		});
		expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent('Wartet auf dich 4');
		const erste = document.getElementById('frage-f1');
		expect(erste).toHaveAttribute('open');
		expect(erste).toHaveTextContent('Antwort auf Erste?');
		const zweite = document.getElementById('frage-f2');
		expect(zweite).not.toHaveAttribute('open');
		expect(zweite?.closest('details.widget-mehr')).toHaveTextContent('noch 1 Frage wartet');
		expect(screen.getByRole('heading', { name: 'Fragen' })).toBeInTheDocument();
		const link = screen.getByRole('link', { name: /Beleg hochladen/ });
		expect(link).toHaveAttribute('href', '#aufgabe-a1');
		expect(screen.getByText(/bis 02\.11\., in 3 Tagen/)).toBeInTheDocument();
	});

	it('Frage ohne Anmeldung: nur der Satz mit den Zahlen', () => {
		render(Frage, { zahl: 2, aufgabenZahl: 1, angemeldet: false });
		expect(screen.getByText('2 Einträge warten auf dich, davon eine Aufgabe.')).toBeInTheDocument();
		expect(screen.getByText(SATZ_OHNE_ANMELDUNG)).toBeInTheDocument();
	});

	it('Arbeiten: Gruppen mit Zahl, zwei vorn, Rest unter „und N weitere“, leere fallen weg', () => {
		const a = (id: string): AnstehendEintrag =>
			eintrag({ art: 'arbeit', id, titel: `Arbeit ${id}` });
		render(Arbeiten, {
			gruppen: [
				{ wort: 'läuft', eintraege: [a('1'), a('2'), a('3')], weitere: 0 },
				{ wort: 'wartet', eintraege: [], weitere: 0 },
				{ wort: 'als Nächstes', eintraege: [], weitere: 4 }
			]
		});
		expect(screen.getAllByRole('heading', { level: 3 }).map((h) => h.textContent)).toEqual([
			'läuft 3',
			'als Nächstes 4'
		]);
		expect(document.getElementById('arbeit-3')?.closest('details.mehr')).toHaveTextContent(
			'und 1 weitere'
		);
		expect(screen.getByText('und 4 weitere')).toBeInTheDocument();
	});

	it('Fristen: nächste zuerst, höchstens drei, Rest aufklappbar; heute gelb, vorbei rot', async () => {
		const f = (id: string, frist: string, tage: number): AnstehendEintrag =>
			eintrag({ id, titel: `Sache ${id}`, frist, tage_bis: tage });
		const { container, rerender } = render(Fristen, {
			naechste: '2026-10-30',
			tageBis: 0,
			fristStufe: 'heute',
			eintraege: [
				f('c', '2026-11-05', 6),
				f('a', '2026-10-30', 0),
				eintrag({ id: 'ohne', titel: 'ohne Frist' }),
				f('b', '2026-11-01', 2),
				f('d', '2026-11-09', 10)
			]
		});
		const vorn = container.querySelectorAll('.karte > ul > li');
		expect([...vorn].map((li) => li.querySelector('a')?.textContent)).toEqual([
			'Sache a',
			'Sache b',
			'Sache c'
		]);
		expect(screen.getByText('und 1 weitere')).toBeInTheDocument();
		expect(container.querySelector('.karte')).toHaveClass('b-wartet');
		await rerender({
			naechste: '2026-10-28',
			tageBis: -2,
			fristStufe: 'ueberfaellig',
			eintraege: []
		});
		expect(container.querySelector('.karte')).toHaveClass('b-problem');
		expect(screen.getByText('28.10.')).toBeInTheDocument();
		// Etappe 3a-4: Der Rand folgt nur der Frist-Stufe des Servers, nicht den Tagen.
		await rerender({ naechste: '2026-10-28', tageBis: -2, fristStufe: 'bald', eintraege: [] });
		expect(container.querySelector('.karte')).not.toHaveClass('b-problem');
		expect(container.querySelector('.karte')).toHaveAttribute('data-frist-stufe', 'bald');
	});

	it('Zuletzt: ungelesene hervorgehoben, „gelesen“ ruft den Aufrufer mit id und Knopf', async () => {
		const gelesen = vi.fn();
		const a = (teil: Partial<Aktualisierung>): Aktualisierung => ({
			quelle: 'meldung',
			id: 'm1',
			art: 'fehler',
			titel: 'Bau rot',
			zeit: '2026-10-30T09:05:00+01:00',
			ziel: '/projects/x#meldung-m1',
			gelesen: false,
			anzahl: 1,
			behoben: false,
			text: null,
			...teil
		});
		render(Zuletzt, {
			aktualisierungen: [a({}), a({ id: 'm2', titel: 'Alt', gelesen: true })],
			ungelesen: 1,
			angemeldet: true,
			ongelesen: gelesen
		});
		expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent('Aktualisierungen 1 neu');
		// Ein Leerzeichen vor „neu“: der Abstand allein (margin) ergab in Edges innerText „rotneu“.
		expect(document.querySelector('#meldung-m1 > summary')?.textContent).toBe('• Bau rot neu');
		const knopf = screen.getByRole('button', { name: 'Gelesen: Bau rot' });
		await fireEvent.click(knopf);
		expect(gelesen).toHaveBeenCalledWith('m1', knopf);
		expect(screen.queryByRole('button', { name: 'Gelesen: Alt' })).toBeNull();
	});

	it('Zuletzt: gleiche Vorgänge als eine Zeile mit „2-mal“, Teilschritt als Wort', () => {
		const v = (teil: Partial<Aktualisierung>): Aktualisierung => ({
			quelle: 'vorgang',
			id: 'v-20261031-0010',
			art: 'bau',
			titel: 'Seite neu bauen',
			zeit: '2026-11-13T00:04:34+01:00',
			ziel: '/vorgaenge#vorgang-v-20261031-0010',
			gelesen: true,
			anzahl: 2,
			behoben: false,
			text: null,
			...teil
		});
		render(Zuletzt, {
			aktualisierungen: [
				v({}),
				v({ id: 'v-2', art: 'schritt', titel: 'Dir angeboten', anzahl: 1 })
			],
			ungelesen: 0,
			angemeldet: true
		});
		const zeilen = screen.getAllByRole('listitem').map((li) => li.textContent);
		expect(zeilen).toEqual([
			'↻ Seite neu bauen · Seite erneuern, 13.11. 00:04, 2-mal',
			'↻ Dir angeboten · Teilschritt, 13.11. 00:04'
		]);
	});

	it('Wörter: wartet auf, Frist-Bedeutung, Sortierung nach Frist', () => {
		expect(wartetAufWort('inhaber')).toBe('wartet auf dich');
		expect(wartetAufWort('chat:postfach')).toBe('wartet auf Chat postfach');
		expect([
			fristBedeutung(null),
			fristBedeutung('spaeter'),
			fristBedeutung('bald'),
			fristBedeutung('heute'),
			fristBedeutung('ueberfaellig')
		]).toEqual([null, null, null, 'wartet', 'problem']);
		const l = nachFrist([
			eintrag({ id: '2', titel: 'B', frist: '2026-11-02' }),
			eintrag({ id: '1', titel: 'A', frist: '2026-11-01' })
		]);
		expect(l.map((e) => e.id)).toEqual(['1', '2']);
	});
});
