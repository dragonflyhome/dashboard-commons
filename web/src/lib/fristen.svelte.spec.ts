/**
 * Etappe B3, Modul Fristen: Worte der Fristenliste, die nächste Frist aus
 * Einträgen und fristen.toml (wie der Server in `anstehend::stand` mit
 * `fristen::datei_fristen_mit_datum`), der Baustein `Fristenliste` und das
 * Widget „Fristen“ mit Fristen aus der Datei.
 * Alle Beispiele sind erfunden.
 */
import { render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { FristenListe, FristZeile } from '$vertrag';
import { eintrag } from '#lib/anstehend.beispiel.ts';
import { ampelSatz, standZahlen } from '#lib/anstehend.ts';
import { bezugSatz, haengtSatz, ladeFristen, nameKurz } from '#lib/fristen.ts';
import AlleFristen from '#lib/components/anstehend/AlleFristen.svelte';
import Fristenliste from '#lib/katalog/Fristenliste.svelte';
import Fristen from '#lib/katalog/widgets/Fristen.svelte';

function zeile(teil: Partial<FristZeile> & Pick<FristZeile, 'id' | 'titel'>): FristZeile {
	return {
		quelle: 'datei',
		projekt: 'antrag',
		projekt_titel: 'Antrag',
		frist: null,
		tage_bis: null,
		frist_stufe: null,
		frist_bezug: null,
		ereignis: null,
		hinweis: null,
		ziel: `/projects/antrag#frist-${teil.id}`,
		...teil
	};
}

const haengt = zeile({
	id: 'helferliste',
	titel: 'Helferliste verschicken',
	frist_bezug: { ereignis: 'aufbau', abstand: '-3 Tage' },
	ereignis: {
		kennung: 'aufbau',
		titel: 'Termin für den Aufbau',
		datum: null,
		herkunft: null,
		zeitraum: null
	}
});
const aufgeloest = zeile({
	id: 'standanmeldung',
	titel: 'Stand anmelden',
	frist: '2026-11-30',
	tage_bis: 26,
	frist_stufe: 'spaeter',
	frist_bezug: { ereignis: 'zusage-da', abstand: '+1 Monat' },
	ereignis: {
		kennung: 'zusage-da',
		titel: 'Zusage ist da',
		datum: '2026-10-31',
		herkunft: null,
		zeitraum: null
	}
});
const fest = zeile({
	id: 'plakate',
	titel: 'Plakate bestellen',
	frist: '2026-11-03',
	tage_bis: -1,
	frist_stufe: 'ueberfaellig'
});

describe('Fristen: Worte', () => {
	it('hängt an … nur ohne Datum; ein fehlendes Ereignis mit Kennung', () => {
		expect(haengtSatz(haengt)).toBe('hängt an „Termin für den Aufbau“ (-3 Tage)');
		expect(haengtSatz(aufgeloest)).toBeNull();
		expect(haengtSatz(fest)).toBeNull();
		expect(
			haengtSatz({
				...haengt,
				ereignis: { kennung: 'aufbau', titel: null, datum: null, herkunft: null, zeitraum: null }
			})
		).toBe('hängt an „aufbau“ (-3 Tage; dieses Ereignis fehlt in fristen.toml)');
		// Eine Aufgabe ohne die Felder (ältere Antwort): kein Satz.
		expect(haengtSatz({ frist: null })).toBeNull();
	});

	it('aufgelöst: woraus das Datum kommt', () => {
		expect(bezugSatz(aufgeloest)).toBe('aus „Zusage ist da“ (31.10.) +1 Monat');
		expect(bezugSatz(haengt)).toBeNull();
		expect(bezugSatz(fest)).toBeNull();
	});

	it('Ampel: eine Frist aus fristen.toml hat keine Art', () => {
		expect(ampelSatz({ grund: 'frist-vorbei', bezug: null, bezug_art: null }, 0)).toBe(
			'Eine Frist ist vorbei.'
		);
		expect(
			ampelSatz({ grund: 'frist-vorbei', bezug: 'Plakate bestellen', bezug_art: null }, 0)
		).toBe('Die Frist von „Plakate bestellen“ ist vorbei.');
	});

	it('Ampel (Etappe B8): wartet lange und ohne Bewegung mit den Tagen des Servers', () => {
		expect(
			ampelSatz({ grund: 'wartet-lange', bezug: 'Logo wählen', bezug_art: 'frage', tage: 9 }, 1)
		).toBe('„Logo wählen“ wartet seit 9 Tagen auf dich.');
		expect(ampelSatz({ grund: 'wartet-lange', bezug: null, bezug_art: 'frage', tage: 4 }, 1)).toBe(
			'Ein Eintrag wartet seit 4 Tagen auf dich.'
		);
		expect(ampelSatz({ grund: 'ohne-bewegung', bezug: null, bezug_art: null, tage: 7 }, 0)).toBe(
			'Seit 7 Tagen hat sich an keiner offenen Arbeit etwas bewegt.'
		);
	});
});

describe('Fristen: nächste Frist (wie der Server)', () => {
	const aufgabe = eintrag({ id: 'a-1', frist: '2026-11-10', tage_bis: 6, frist_stufe: 'bald' });

	it('die frühere gewinnt; bei gleichem Datum der Eintrag; ohne Datum zählt nicht', () => {
		expect(standZahlen([aufgabe], [], [fest]).naechste_frist).toBe('2026-11-03');
		expect(standZahlen([aufgabe], [], [aufgeloest, haengt]).naechste_frist).toBe('2026-11-10');
		expect(standZahlen([], [], [haengt]).naechste_frist).toBeNull();
		const gleich = { ...fest, frist: '2026-11-10', tage_bis: 6, frist_stufe: 'spaeter' as const };
		expect(standZahlen([aufgabe], [], [gleich]).frist_stufe).toBe('bald');
		const z = standZahlen([], [], [aufgeloest, fest]);
		expect([z.naechste_frist, z.tage_bis, z.frist_stufe]).toEqual([
			'2026-11-03',
			-1,
			'ueberfaellig'
		]);
	});
});

describe('Baustein Fristenliste', () => {
	const liste = (zeilen: FristZeile[]): FristenListe => ({
		heute: '2026-11-04',
		frist_bald_tage: 7,
		zeilen,
		hinweise: [],
		speicher_fehlt: false,
		mond: null
	});

	it('nach Datum vom Server, bewegliche ohne Datum am Ende mit „hängt an“, Anker je Datei-Frist', () => {
		const aufgabeZeile = zeile({
			quelle: 'aufgabe',
			id: 'a-20261104-0001',
			titel: 'Getränkeliste prüfen',
			frist: '2026-11-07',
			tage_bis: 3,
			frist_stufe: 'bald',
			ziel: '/projects/antrag#aufgabe-a-20261104-0001'
		});
		const { container } = render(Fristenliste, {
			liste: liste([fest, aufgabeZeile, aufgeloest, haengt])
		});
		const zeilen = [...container.querySelectorAll('li')];
		const nr = (i: number): HTMLElement => {
			const li = zeilen[i];
			if (li === undefined) throw new Error(`Zeile ${String(i)} fehlt`);
			return li;
		};
		expect(zeilen.map((li) => li.querySelector('.was')?.firstChild?.textContent?.trim())).toEqual([
			'Plakate bestellen',
			'Getränkeliste prüfen',
			'Stand anmelden',
			'Helferliste verschicken'
		]);
		expect(zeilen[0]).toHaveAttribute('id', 'frist-plakate');
		expect(within(nr(0)).getByText('03.11.2026')).toBeInTheDocument();
		expect(within(nr(0)).getByText(/überfällig/)).toBeInTheDocument();
		expect(within(nr(1)).getByRole('link', { name: 'Getränkeliste prüfen' })).toBeInTheDocument();
		expect(
			within(nr(2)).getByText(/aus „Zusage ist da“ \(31\.10\.\) \+1 Monat/)
		).toBeInTheDocument();
		expect(within(nr(3)).getByText('ohne Datum')).toBeInTheDocument();
		expect(within(nr(3)).getByText(/hängt an „Termin für den Aufbau“/)).toBeInTheDocument();
	});

	it('übergreifend mit Projekt, ohne Anker; leer mit Satz', () => {
		const { container, unmount } = render(Fristenliste, {
			liste: liste([fest]),
			mitProjekt: true
		});
		expect(screen.getByText(/· Antrag/)).toBeInTheDocument();
		expect(container.querySelector('li')).not.toHaveAttribute('id');
		unmount();
		render(Fristenliste, { liste: liste([]), leerSatz: 'Gerade läuft keine Frist.' });
		expect(screen.getByText('Gerade läuft keine Frist.')).toBeInTheDocument();
	});

	it('Kalender: Fest mit Herkunft, ohne Stufe und ohne Link auf dieselbe Seite; Frist mit Herkunft', () => {
		const pfingsten = zeile({
			quelle: 'kalender',
			id: 'kirche.pfingsten',
			titel: 'Pfingstsonntag',
			frist: '2027-05-16',
			tage_bis: 177,
			ziel: '/projects/antrag',
			ereignis: {
				kennung: 'kirche.pfingsten',
				titel: 'Kirchenjahr',
				datum: '2027-05-16',
				herkunft: 'Kirchenjahr · Pfingstsonntag · 49 Tage nach Ostersonntag (28.3.2027)',
				zeitraum: null
			}
		});
		const einkauf = zeile({
			id: 'einkauf',
			titel: 'Einkauf fürs Neujahrsfest',
			frist: '2027-02-02',
			tage_bis: 74,
			frist_stufe: 'spaeter',
			frist_bezug: { ereignis: 'neujahr', abstand: '-3 Tage' },
			ereignis: {
				kennung: 'neujahr',
				titel: 'Neujahrsfest',
				datum: '2027-02-06',
				herkunft: 'Chinesischer Kalender · Chinesisches Neujahr',
				zeitraum: null
			}
		});
		const { container, unmount } = render(Fristenliste, { liste: liste([einkauf, pfingsten]) });
		const [frist, festZeile] = [...container.querySelectorAll('li')];
		expect(festZeile).toHaveAttribute('data-frist-stufe', 'keine');
		expect(festZeile?.textContent).toMatch(/Fest\s*· Kirchenjahr · Pfingstsonntag · 49 Tage nach/);
		expect(within(festZeile as HTMLElement).queryByRole('link')).toBeNull();
		expect(frist?.textContent).toMatch(/· Chinesischer Kalender · Chinesisches Neujahr/);
		unmount();
		render(Fristenliste, { liste: liste([pfingsten]), mitProjekt: true });
		expect(screen.getByRole('link', { name: 'Pfingstsonntag' })).toBeInTheDocument();
	});

	it('Kalender, Stufe 2: Mondmonat mit Zeitraum, laufender Mond mit überlieferten Namen', () => {
		const samhain = zeile({
			quelle: 'kalender',
			id: 'jahreskreis.samhain',
			titel: 'Samhain',
			frist: '2027-09-30',
			tage_bis: 330,
			ziel: '/projects/kalender',
			ereignis: {
				kennung: 'jahreskreis.samhain',
				titel: 'Jahreskreis',
				datum: '2027-09-30',
				herkunft:
					'Jahreskreis · Samhain · Monat um den 11. Vollmond nach der Wintersonnenwende (21.12.2026) · überliefert: Winterfylleþ (altenglisch, Beda)',
				zeitraum: { anfang: '2027-09-30', vollmond: '2027-10-15', ende: '2027-10-29' }
			}
		});
		const mond = {
			kalender: 'Jahreskreis',
			nummer: 12,
			name: '1. Wintervollmond',
			namen: [
				{
					name: 'Blōtmōnaþ',
					sprache: 'altenglisch',
					quelle: 'Beda, De temporum ratione 15, um 725',
					bedeutung: 'Opfermonat',
					stufe: 'belegt' as const,
					ungeprueft: false
				},
				{
					name: 'herbistmanoth',
					sprache: 'althochdeutsch',
					quelle: 'Einhard, Vita Karoli Magni 29',
					bedeutung: null,
					stufe: 'belegt' as const,
					ungeprueft: true
				}
			],
			zeitraum: { anfang: '2026-11-09', vollmond: '2026-11-24', ende: '2026-12-09' },
			hinweis: null,
			phase: 'zunehmend'
		};
		const { container } = render(Fristenliste, { liste: { ...liste([samhain]), mond } });
		const zeitraum = container.querySelector('li [data-zeitraum]');
		expect(zeitraum?.textContent).toMatch(
			/Neumond 30\.09\.2027 bis 29\.10\.2027, Vollmond 15\.10\.2027/
		);
		expect(zeitraum?.querySelector('strong')?.textContent).toBe('Vollmond 15.10.2027');
		const kasten = container.querySelector('[data-mond]');
		expect(kasten?.textContent).toMatch(
			/Laufender Mond:\s*1\. Wintervollmond\s*· Blōtmōnaþ \(altenglisch, Beda\)/
		);
		expect(kasten?.textContent).toMatch(/12\. Monat im Festjahr, Jahreskreis/);
		expect(kasten?.querySelector('[data-phase]')?.textContent).toBe('zunehmend');
		expect(kasten?.querySelector('details')).not.toHaveAttribute('open');
		expect(kasten?.querySelector('details li:last-child')?.textContent).toMatch(
			/herbistmanoth \(althochdeutsch\).*belegt, nicht gegen eine Fachausgabe geprüft/s
		);
	});

	it('ohne Anmeldung nur der Satz zum Anmelden, kein Titel', () => {
		const geladen = {
			zustand: 'fehler' as const,
			satz: 'Bitte anmelden.',
			ohneAnmeldung: true,
			inhalt: null,
			abgeleitet: null,
			neuLaden: () => Promise.resolve()
		};
		render(Fristenliste, { daten: geladen });
		expect(screen.getByText(/Anmelden, um die Fristen zu sehen/)).toBeInTheDocument();
		expect(screen.queryByRole('list')).toBeNull();
	});
});

describe('Widget „Fristen“ mit Fristen aus fristen.toml', () => {
	it('in derselben Liste nach Datum, Link auf den Anker der Fristenliste', () => {
		const { container } = render(Fristen, {
			naechste: '2026-11-03',
			tageBis: -1,
			fristStufe: 'ueberfaellig',
			eintraege: [
				eintrag({ id: 'a-1', titel: 'Getränkeliste prüfen', frist: '2026-11-07', tage_bis: 3 })
			],
			dateiFristen: [aufgeloest, fest]
		});
		const links = [...container.querySelectorAll('.karte > ul > li a')];
		expect(links.map((a) => [a.textContent, a.getAttribute('href')])).toEqual([
			['Plakate bestellen', '#frist-plakate'],
			['Getränkeliste prüfen', '#aufgabe-a-1'],
			['Stand anmelden', '#frist-standanmeldung']
		]);
	});
});

describe('„Alle Fristen“ in Anstehend', () => {
	const antwort = (status: number, body: unknown): Response =>
		new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
	const uebergreifend: FristenListe = {
		heute: '2026-11-04',
		frist_bald_tage: 7,
		zeilen: [fest, haengt],
		hinweise: [],
		speicher_fehlt: false,
		mond: null
	};

	it('lädt /api/fristen und zeigt die Liste mit Projekt, zugeklappt', async () => {
		const fetchFn = vi.fn(() => Promise.resolve(antwort(200, uebergreifend)));
		const { container } = render(AlleFristen, { fetchFn });
		expect(await screen.findByRole('heading', { name: /Alle Fristen/ })).toBeInTheDocument();
		expect(fetchFn).toHaveBeenCalledWith('/api/fristen', expect.anything());
		expect(container.querySelector('details')).not.toHaveAttribute('open');
		expect(within(container).getAllByText(/· Antrag/).length).toBe(2);
	});

	it('ohne Anmeldung oder mit anderer Antwort fehlt der Abschnitt', async () => {
		for (const res of [antwort(401, { fehler: 'x' }), antwort(200, { eintraege: [] })]) {
			const fetchFn = vi.fn(() => Promise.resolve(res));
			const r = render(AlleFristen, { fetchFn });
			await waitFor(() => {
				expect(fetchFn).toHaveBeenCalled();
			});
			expect(screen.queryByRole('heading', { name: /Alle Fristen/ })).toBeNull();
			r.unmount();
		}
		expect(await ladeFristen(() => Promise.reject(new Error('Netz')))).toBeNull();
	});
});

describe('nameKurz (wie Name::kurz im Server)', () => {
	const name = (stufe: 'belegt' | 'erschlossen' | 'neuzeitlich') => ({
		name: 'Ostara',
		sprache: 'neuheidnischer Festname',
		quelle: 'Göttinnenname bei Jacob Grimm, Deutsche Mythologie, 1835',
		bedeutung: null,
		stufe,
		ungeprueft: false
	});
	it('Quelle bis zum ersten Komma, eine unsichere Stufe dabei', () => {
		expect(nameKurz(name('belegt'))).toBe(
			'Ostara (neuheidnischer Festname, Göttinnenname bei Jacob Grimm)'
		);
		expect(nameKurz(name('neuzeitlich'))).toBe(
			'Ostara (neuheidnischer Festname, Göttinnenname bei Jacob Grimm, neuzeitlich)'
		);
		expect(nameKurz(name('erschlossen'))).toMatch(/, erschlossen\)$/);
	});
});
