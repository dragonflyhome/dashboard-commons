/**
 * Etappe B2, Modul Backlog, seit B8 die Aufgabenliste der Projektseite: der
 * Baustein `Backlog` (Gruppen untereinander, Zeilen mit Etappe, wer dran ist,
 * Frist und Schritten), das Laden, die Fragen-Warteschlange in `auswaehlen`
 * und im Baustein `Fragen`. Alle Daten erfunden.
 */
import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { afterEach, describe, expect, it } from 'vitest';
import type { AnstehendEintrag, Backlog as BacklogDaten, Frage } from '$vertrag';
import { Backlog, Fragen } from '$katalog';
import FragenListe from '#lib/components/projekt/FragenListe.svelte';
import { auswaehlen } from '#lib/anstehend.ts';
import { eintrag, json } from '#lib/anstehend.beispiel.ts';
import { ladeBacklog } from '#lib/backlog.ts';
import { etappenNamen } from '#lib/projektteile.ts';
import { backlog, fragen, fragenAbgeleitet, projektPlan } from '../routes/katalog/beispiele.ts';

afterEach(cleanup);

function gruppen(): string[] {
	return screen
		.getAllByRole('heading', { level: 3 })
		.map((h) => h.textContent.replace(/\s+/g, ' ').trim());
}

describe('Baustein Backlog als Aufgabenliste (Etappe B8)', () => {
	it('vier Gruppen untereinander mit Zahl; Fragen und Aufgaben führen zu ihrem Eintrag', () => {
		render(Backlog, { backlog: backlog(), etappen: etappenNamen(projektPlan()) });
		expect(gruppen()).toEqual([
			'Wartet auf dich (2)',
			'Läuft (1)',
			'Als Nächstes (1)',
			'Wartet auf andere (2)'
		]);
		const dich = screen.getByRole('region', { name: /Wartet auf dich/ });
		const link = within(dich).getByRole('link', { name: 'Beim Beispielportal anmelden' });
		expect(link.getAttribute('href')).toMatch(/\/projects\/beispiel#aufgabe-a-20261103-0001$/);
		// Frist heute ist hervorgehoben (Farbe nur zusätzlich zum Wort „bis …“).
		expect(within(dich).getByText(/^bis /)).toHaveClass('achtung');
		expect(within(dich).getByText('noch 1 Frage wartet')).toBeInTheDocument();
		expect(screen.queryByText('Wie soll die Unterseite heißen?')).toBeNull();
		expect(screen.getByText(/eine steht oben; noch 1 Frage wartet/)).toBeInTheDocument();
		// Wer dran ist, in Worten.
		const andere = screen.getByRole('region', { name: /Wartet auf andere/ });
		expect(within(andere).getByText('wartet auf Chat nachbar')).toBeInTheDocument();
		expect(within(andere).getByText('wartet auf jemand anderen')).toBeInTheDocument();
		// Keine Knöpfe: geantwortet wird beim Eintrag.
		expect(screen.queryByRole('button')).toBeNull();
	});

	it('eine Arbeit mit Schritten: Anker, Etappe, Kurzzeile, per Tastatur aufklappbar zu den Schritten', async () => {
		render(Backlog, { backlog: backlog(), etappen: etappenNamen(projektPlan()) });
		const zeile = document.getElementById('arbeit-ar-20261102-0001');
		expect(zeile?.tagName).toBe('DETAILS');
		expect(zeile).toHaveTextContent('Etappe: Einladung');
		expect(zeile).toHaveTextContent('1 von 2 fertig · dran: Chat beispiel');
		expect(zeile).not.toHaveAttribute('open');
		const summary = zeile?.querySelector('summary');
		if (!summary) throw new Error('keine Zeile');
		await fireEvent.click(summary);
		expect(zeile).toHaveAttribute('open');
		const liste = screen.getByRole('list', { name: 'Schritte von „Rundmail entwerfen“' });
		expect(within(liste).getAllByRole('listitem')).toHaveLength(2);
		// Gegenprobe: eine Arbeit ohne Schritte ist eine einfache Zeile mit Anker, ohne Aufklappen.
		const ohne = document.getElementById('arbeit-ar-20261102-0004');
		expect(ohne?.tagName).toBe('DIV');
	});

	it('mit Kasten der Seite: „Wartet auf dich“ trägt ihn und die Zahl des Stands', () => {
		const kasten = createRawSnippet(() => ({ render: () => '<p>Kasten der Seite</p>' }));
		render(Backlog, { backlog: backlog(), fuerDich: kasten, fuerDichZahl: 3 });
		expect(gruppen()[0]).toBe('Wartet auf dich (3)');
		const dich = screen.getByRole('region', { name: /Wartet auf dich/ });
		expect(within(dich).getByText('Kasten der Seite')).toBeInTheDocument();
		// Frage und Aufgabe stehen dann nur im Kasten, nicht noch einmal als Zeile.
		expect(within(dich).queryByRole('link')).toBeNull();
		expect(screen.queryByText(/eine steht oben/)).toBeNull();
	});

	it('Als Nächstes zeigt fünf, der Rest klappt auf; leere Gruppen sagen es in einem Satz', () => {
		const b: BacklogDaten = backlog();
		const vorlage = b.spalten[3]?.karten[0];
		if (!vorlage) throw new Error('Beispiel fehlt');
		const naechstes = b.spalten[3];
		if (naechstes)
			naechstes.karten = Array.from({ length: 7 }, (_, i) => ({
				...vorlage,
				id: `ar-20261102-01${String(i)}`,
				titel: `Schritt ${String(i + 1)}`
			}));
		const laeuft = b.spalten[1];
		if (laeuft) laeuft.karten = [];
		render(Backlog, { backlog: b });
		const region = screen.getByRole('region', { name: /Als Nächstes/ });
		expect(within(region).getByText('und 2 weitere')).toBeInTheDocument();
		expect(within(region).getByText('Schritt 7').closest('details:not(.zeile)')).not.toBeNull();
		expect(screen.getByText('Gerade läuft nichts.')).toBeInTheDocument();
	});

	it('ohne Datenspeicher ein Hinweis', () => {
		const b: BacklogDaten = {
			...backlog(),
			spalten: backlog().spalten.map((s) => ({ ...s, karten: [] })),
			wartend: [],
			fragen_weitere: 0,
			speicher_fehlt: true
		};
		render(Backlog, { backlog: b });
		expect(screen.getByText(/Der Datenspeicher fehlt gerade/)).toBeInTheDocument();
		expect(screen.getByText('Nichts wartet auf dich.')).toBeInTheDocument();
	});

	it('ladeBacklog: ohne Anmeldung, ohne Projekt oder bei Netzfehler null', async () => {
		for (const status of [401, 404]) {
			expect(await ladeBacklog('beispiel', () => Promise.resolve(json({}, status)))).toBeNull();
		}
		expect(await ladeBacklog('beispiel', () => Promise.reject(new Error('Netz')))).toBeNull();
		expect(await ladeBacklog('beispiel', () => Promise.resolve(json(backlog())))).not.toBeNull();
	});
});

describe('Fragen-Warteschlange (Etappe B2)', () => {
	it('auswaehlen nimmt den Platz vom Server, nicht das Alter', () => {
		const alt = eintrag({
			art: 'frage',
			id: 'alt',
			projekt: 'beispiel',
			gestellt: '2026-10-01',
			erstellt: '2026-10-01',
			warteschlange: { platz: 2, offen: 3 }
		});
		const vorn = eintrag({
			art: 'frage',
			id: 'vorn',
			projekt: 'beispiel',
			gestellt: '2026-10-20',
			erstellt: '2026-10-20',
			warteschlange: { platz: 1, offen: 3 }
		});
		const aus = auswaehlen([alt, vorn]);
		expect(aus.map((e) => e.id)).toEqual(['vorn']);
		// Die dritte offene Frage fehlt in der Liste (etwa gefiltert); gezählt wird nach dem Server.
		expect(aus[0]?.vermerk).toBe('noch 2 Fragen warten');
		// Fehlt Platz 1 in der Liste, ist keine Frage sichtbar (nie die zweite statt der ersten).
		expect(auswaehlen([alt])).toEqual([]);
		// Ohne Platz (alte Daten): die alte Regel, die älteste zuerst.
		const ohnePlatz = (e: AnstehendEintrag): AnstehendEintrag => {
			const kopie: Partial<AnstehendEintrag> = { ...e };
			delete kopie.warteschlange;
			return kopie as AnstehendEintrag;
		};
		const ohne = auswaehlen([ohnePlatz(vorn), ohnePlatz(alt)]);
		expect(ohne.map((e) => e.id)).toEqual(['alt']);
	});

	it('Fragen zeigt mit Warteschlange nur die sichtbare Frage und „noch N Fragen warten“', () => {
		render(Fragen, { daten: fragen(), abgeleitet: fragenAbgeleitet() });
		expect(screen.getByText('Soll die Ampel im Stand als Rand oben stehen?')).toBeInTheDocument();
		expect(screen.queryByText('Wie soll die Unterseite heißen?')).toBeNull();
		expect(screen.getByText('Dahinter noch 1 Frage wartet.')).toBeInTheDocument();
	});

	it('Fragen ohne Anmeldung: ein Satz zum Anmelden, kein Fehler, kein Fragentext', () => {
		render(Fragen, {
			daten: {
				zustand: 'fehler',
				inhalt: null,
				abgeleitet: null,
				satz: 'Anmelden, um das zu sehen.',
				ohneAnmeldung: true,
				neuLaden: () => Promise.resolve()
			}
		});
		expect(screen.getByText(/Anmelden, um die Fragen an dich zu sehen/)).toBeInTheDocument();
		expect(screen.queryByRole('alert')).toBeNull();
		expect(screen.queryByRole('button', { name: 'neu laden' })).toBeNull();
	});

	it('FragenListe: ohne Warteschlange alle (alte Daten); alle beantwortet: keine offene Frage', () => {
		const liste: Frage[] = fragen().frage;
		const a = render(FragenListe, { fragen: liste });
		expect(screen.getAllByRole('listitem').filter((li) => li.querySelector('b'))).toHaveLength(2);
		a.unmount();
		render(FragenListe, {
			fragen: liste,
			warteschlange: { sichtbar: null, gestellt: null, weitere: 0 }
		});
		expect(screen.getByText('Keine offene Frage.')).toBeInTheDocument();
	});
});
