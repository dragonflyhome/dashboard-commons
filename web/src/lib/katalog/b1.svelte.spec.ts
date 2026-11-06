/**
 * Etappe B1 (gemeinsame Bausteine): Marke, Stand-Hinweis, Tabelle aus Daten
 * (Summenzeile vom Server, Link je Zeile, Marke in der Zelle), Kennzahl und
 * Ampel mit „Stand vom …“, Fortschritt mit Kommazahl. Alle Daten erfunden.
 */
import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type {
	Ampel as AmpelDaten,
	Fortschritt as FortschrittDaten,
	Kennzahlen as KennzahlenDaten,
	ListeAbgeleitet
} from '$vertrag';
import type { Geladen } from '../projekt/laden.svelte.ts';
import { Ampel, Fortschritt, Kennzahlen, Marke, StandHinweis, Tabelle } from './index.ts';
import type { Spalte } from './index.ts';

const KOSTEN = {
	posten: [
		{
			name: 'Druckerei',
			betrag: 120.5,
			zustand: 'fertig',
			adresse: 'https://example.org/rechnung'
		},
		{ name: 'Versand', betrag: 18.9, zustand: 'offen', adresse: '/projekte/beispiel-a' },
		{ name: 'Muster', betrag: 42, zustand: 'fehlt', adresse: 'javascript:alert(1)' },
		{ name: 'Porto', adresse: '//fremd.example/x' }
	]
};

const ABGELEITET: ListeAbgeleitet = {
	anzahl: 4,
	erledigt: null,
	summe: { betrag: 181.4 },
	datum: {},
	marken: { zustand: ['ok', 'wartet', 'problem', null] }
};

const SPALTEN: Spalte[] = [
	{ schluessel: 'name', titel: 'Posten', link: 'adresse' },
	{ schluessel: 'zustand', titel: 'Stand', art: 'marke' },
	{ schluessel: 'betrag', titel: 'Betrag', art: 'zahl', summe: true }
];

function geladen(teil: Partial<Geladen<unknown>>): Geladen<unknown> {
	return {
		zustand: 'fertig',
		inhalt: null,
		abgeleitet: null,
		satz: null,
		neuLaden: () => Promise.resolve(),
		...teil
	};
}

describe('Etappe B1: Marke', () => {
	it('zeigt das Wort mit der Klasse der Bedeutung', () => {
		render(Marke, { bedeutung: 'wartet', wort: 'offen' });
		const m = screen.getByText('offen');
		expect(m).toHaveClass('marke', 'b-wartet');
		expect(m).toHaveAttribute('data-bedeutung', 'wartet');
	});
	it('ohne Wort: das Wort der Bedeutung', () => {
		render(Marke, { bedeutung: 'ok' });
		expect(screen.getByText('in Ordnung')).toHaveClass('b-ok');
	});
});

describe('Etappe B1: Stand-Hinweis', () => {
	it('frisch: nur das Datum, ruhig', () => {
		render(StandHinweis, { stand: '2026-10-30', alter: { tage: 2, stufe: 'frisch' } });
		const h = screen.getByText('Stand vom 30.10.2026');
		expect(h).toHaveClass('b-neutral');
		expect(h).not.toHaveClass('alt');
	});
	it('alt (Stufe vom Server): gelb und in Worten', () => {
		render(StandHinweis, { stand: '2026-10-20', alter: { tage: 14, stufe: 'alt' } });
		const h = screen.getByText(/Stand vom 20\.10\.2026/);
		expect(h).toHaveTextContent('Stand vom 20.10.2026, 14 Tage alt');
		expect(h).toHaveClass('b-wartet', 'alt');
	});
	it('ohne Alter vom Server: nur das Datum', () => {
		render(StandHinweis, { stand: '2026-10-20' });
		expect(screen.getByText('Stand vom 20.10.2026')).not.toHaveClass('alt');
	});
});

describe('Etappe B1: Tabelle aus Daten', () => {
	it('Summenzeile mit der Summe des Servers, Marken, Links nur sichere', () => {
		render(Tabelle, {
			beschriftung: 'Kosten',
			daten: KOSTEN,
			liste: 'posten',
			abgeleitet: ABGELEITET,
			spalten: SPALTEN
		});
		const t = screen.getByRole('table', { name: 'Kosten' });
		const zeilen = within(t).getAllByRole('row');
		expect(zeilen).toHaveLength(6);
		// Summenzeile: Wort vorn, Summe aus `abgeleitet`, deutsch geschrieben.
		const summe = zeilen[5];
		if (summe === undefined) throw new Error('Summenzeile fehlt');
		expect(within(summe).getByRole('rowheader')).toHaveTextContent('Summe');
		expect(summe).toHaveTextContent('181,4');
		// Marken mit der Bedeutung des Servers; ohne Wort keine Marke.
		expect(within(t).getByText('fertig')).toHaveClass('marke', 'b-ok');
		expect(within(t).getByText('offen')).toHaveClass('b-wartet');
		expect(within(t).getByText('fehlt')).toHaveClass('b-problem');
		// https: neuer Tab; Pfad der App: gleicher Tab; javascript: und //: kein Link.
		const extern = within(t).getByRole('link', { name: /Druckerei/ });
		expect(extern).toHaveAttribute('href', 'https://example.org/rechnung');
		expect(extern).toHaveAttribute('target', '_blank');
		expect(extern).toHaveAttribute('rel', 'noopener noreferrer');
		const intern = within(t).getByRole('link', { name: 'Versand' });
		expect(intern).toHaveAttribute('href', '/projekte/beispiel-a');
		expect(intern).not.toHaveAttribute('target');
		expect(within(t).getAllByRole('link')).toHaveLength(2);
		expect(within(t).getByText('Muster').closest('a')).toBeNull();
		expect(within(t).getByText('Porto').closest('a')).toBeNull();
	});

	it('nimmt Zeilen und Abgeleitetes aus dem Objekt von ladeDaten, auch in Teilen', () => {
		render(Tabelle, {
			beschriftung: 'Kosten',
			daten: geladen({
				inhalt: { teil: { herbst: KOSTEN } },
				abgeleitet: { heute: '2026-11-03', listen: { 'teil.herbst.posten': ABGELEITET } }
			}),
			liste: 'teil.herbst.posten',
			spalten: SPALTEN
		});
		expect(screen.getByText('Druckerei')).toBeInTheDocument();
		expect(screen.getByText('181,4')).toBeInTheDocument();
		expect(screen.getByText('fertig')).toHaveClass('b-ok');
	});

	it('rechnet nie selbst: ohne Abgeleitetes bleibt die Summe leer, Marken neutral', () => {
		render(Tabelle, { beschriftung: 'Kosten', daten: KOSTEN, liste: 'posten', spalten: SPALTEN });
		expect(screen.getByRole('rowheader')).toHaveTextContent('Summe');
		expect(screen.queryByText('181,4')).toBeNull();
		expect(screen.getByText('fertig')).toHaveClass('b-neutral');
	});

	it('Abnahme B1: nan/inf (im JSON null) brechen die Seite nicht', () => {
		// So käme es an, wenn der Server eine Summe aus nan oder inf schickte.
		const kaputt = { ...ABGELEITET, summe: { betrag: null } } as unknown as ListeAbgeleitet;
		render(Tabelle, {
			beschriftung: 'Kosten',
			daten: { posten: [{ name: 'A', betrag: null }] },
			liste: 'posten',
			abgeleitet: kaputt,
			spalten: SPALTEN
		});
		const summe = screen.getByRole('rowheader').closest('tr');
		if (summe === null) throw new Error('Summenzeile fehlt');
		expect(summe.querySelectorAll('td')[1]).toHaveTextContent(/^$/);
		expect(screen.getByText('A')).toBeInTheDocument();
	});

	it('Laden, Fehler, leere Liste und keine Liste', () => {
		const { unmount } = render(Tabelle, {
			beschriftung: 'K',
			daten: geladen({ zustand: 'laedt' }),
			liste: 'posten',
			spalten: SPALTEN
		});
		expect(screen.getByText('Daten werden geladen …')).toBeInTheDocument();
		unmount();
		const f = render(Tabelle, {
			beschriftung: 'K',
			daten: geladen({ zustand: 'fehler', satz: 'eigen-kosten.toml, Zeile 3: kaputt' }),
			liste: 'posten',
			spalten: SPALTEN
		});
		expect(screen.getByRole('alert')).toHaveTextContent('eigen-kosten.toml, Zeile 3: kaputt');
		f.unmount();
		const l = render(Tabelle, {
			beschriftung: 'K',
			daten: { posten: [] },
			liste: 'posten',
			spalten: SPALTEN,
			leer: 'Noch keine Kosten.'
		});
		expect(screen.getByText('Noch keine Kosten.')).toBeInTheDocument();
		l.unmount();
		render(Tabelle, {
			beschriftung: 'K',
			daten: { posten: 'Text' },
			liste: 'posten',
			spalten: SPALTEN
		});
		expect(
			screen.getByText('In den Daten steht unter „posten“ keine Liste von Einträgen.')
		).toBeInTheDocument();
	});

	it('mit eigenen Zeilen: wahr/falsch als ja/nein, Summe aus `abgeleitet`', () => {
		render(Tabelle, {
			beschriftung: 'Zeilen',
			spalten: [
				{ schluessel: 'name', titel: 'Name' },
				{ schluessel: 'fertig', titel: 'Fertig' },
				{ schluessel: 'betrag', titel: 'Betrag', art: 'zahl', summe: true }
			],
			zeilen: [
				{ name: 'A', fertig: true, betrag: 1000 },
				{ name: 'B', fertig: false, betrag: 234.5 }
			],
			abgeleitet: { ...ABGELEITET, summe: { betrag: 1234.5 } },
			summeWort: 'Gesamt'
		});
		expect(screen.getByText('ja')).toBeInTheDocument();
		expect(screen.getByText('nein')).toBeInTheDocument();
		expect(screen.getByRole('rowheader')).toHaveTextContent('Gesamt');
		expect(screen.getByText('1.234,5')).toHaveClass('zahl');
	});
});

describe('Etappe B1: Stand an Kennzahl und Ampel, Fortschritt mit Kommazahl', () => {
	it('Kennzahl mit stand: „Stand vom …“ unter der Kachel', () => {
		const k: KennzahlenDaten = {
			kennzahl: [
				{
					titel: 'Ungelesene Briefe',
					wert: '14',
					einheit: null,
					hinweis: null,
					farbe: null,
					stand: '2026-10-20',
					stand_alter: { tage: 14, stufe: 'alt' }
				},
				{ titel: 'Ohne Stand', wert: '0', einheit: null, hinweis: null, farbe: null }
			]
		};
		render(Kennzahlen, { daten: k });
		expect(screen.getAllByText(/^Stand vom/)).toHaveLength(1);
		expect(screen.getByText(/Stand vom 20\.10\.2026/)).toHaveClass('b-wartet');
	});

	it('Ampel-Zeile mit stand', () => {
		const a: AmpelDaten = {
			zeile: [
				{
					titel: 'Postfach',
					farbe: 'ok',
					hinweis: null,
					stand: '2026-11-01',
					stand_alter: { tage: 2, stufe: 'frisch' }
				}
			]
		};
		render(Ampel, { daten: a });
		expect(screen.getByText('Stand vom 01.11.2026')).toHaveClass('b-neutral');
	});

	it('Fortschritt: 0,2 von 1,5 GB', () => {
		const f: FortschrittDaten = {
			zeile: [
				{
					titel: 'Ablage gesichert',
					prozent: null,
					erreicht: 0.2,
					gesamt: 1.5,
					einheit: 'GB',
					hinweis: null,
					stufe: 'unter-haelfte'
				}
			]
		};
		render(Fortschritt, { daten: f });
		expect(screen.getAllByText('0,2 von 1,5 GB').length).toBeGreaterThan(0);
		expect(screen.getByRole('progressbar', { name: 'Ablage gesichert' })).toHaveAttribute(
			'aria-valuenow',
			'13'
		);
	});
});
