import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	SATZ_NICHT_MEHR_DA,
	SATZ_NOTIZEN_OHNE_ANMELDUNG,
	SATZ_OHNE_ANMELDUNG
} from '#lib/anstehend.ts';
import { SATZ_GESENDET } from '#lib/antworten.ts';
import { ZEIGEN_MS } from '#lib/antworten.svelte.ts';
import {
	beispielProjektStand as beispielStand,
	beispielZahlen,
	eintrag,
	json
} from '#lib/anstehend.beispiel.ts';
import { aufgaben } from '#lib/aufgaben.svelte.ts';
import { rueckfall } from '#lib/rueckfall.ts';
import type {
	Aktualisierung,
	AnstehendEintrag,
	ChangeEvent,
	ProjektStand,
	VorgangEvent
} from '$vertrag';

const { ort } = vi.hoisted(() => ({
	ort: { url: new URL('http://localhost:8787/projects/dashboard') }
}));
vi.mock('$app/state', () => ({ page: ort }));

const { default: UnterDashboard } = await import('./UnterDashboard.svelte');

const arbeit = eintrag({ art: 'arbeit', id: 'ar-1', titel: 'Seite umbauen', zustand: 'laeuft' });
const aufgabe = eintrag({
	id: 'a-1',
	titel: 'Beim Probe-Dienst anmelden',
	frist: '2026-10-31',
	tage_bis: 2,
	frist_stufe: 'bald'
});
const frage = eintrag({
	art: 'frage',
	id: 'schlusssatz',
	titel: 'Welcher Schlusssatz?',
	gestellt: '2026-10-25'
});

/** Was `/stand` ohne Anmeldung schickt (Etappe 3a+): keine Einträge, nur Zahlen. */
function frei(stand: ProjektStand): ProjektStand {
	return {
		...stand,
		sichtbar: false,
		eintraege: [],
		aktualisierungen: [],
		zahlen: stand.zahlen ?? beispielZahlen(),
		// Etappe B2: open[] ohne Anmeldung nur als Zahl.
		notizen: [],
		notizen_zahl: stand.notizen.length
	};
}

/**
 * fetch für `/stand`, `/stand/voll` und `/api/anstehend?projekt=…`;
 * `eintraege: null` heißt ohne Anmeldung (401 auf den geschützten Wegen).
 * Etappe 3a+: `/stand/voll` schickt alle Einträge ohne Zahlen, `/stand`
 * nur die Zahlen (`zahlen`), die Liste alle Einträge ohne Zähler.
 */
function netz(stand: ProjektStand | null, eintraege: AnstehendEintrag[] | null): typeof fetch {
	return ((url: string) => {
		if (url.endsWith('/stand/voll') && eintraege === null)
			return Promise.resolve(json({ fehler: 'Anmeldung', art: 'anmeldung' }, 401));
		if (stand === null && (url.endsWith('/stand') || url.endsWith('/stand/voll')))
			return Promise.resolve(json({ fehler: 'Kein Projekt' }, 404));
		if (stand !== null && url.endsWith('/stand/voll'))
			return Promise.resolve(
				json({ ...stand, sichtbar: true, zahlen: null, eintraege: eintraege ?? [] })
			);
		if (stand !== null && url.endsWith('/stand')) return Promise.resolve(json(frei(stand)));
		if (url.startsWith('/api/anstehend'))
			return Promise.resolve(
				eintraege === null
					? json({ fehler: 'Anmeldung', art: 'anmeldung' }, 401)
					: json({
							heute: '2026-10-29',
							frist_bald_tage: 7,
							eintraege,
							hinweise: [],
							aktualisierungen: []
						})
			);
		return Promise.resolve(json({}, 404));
	}) as typeof fetch;
}

function zeige(fetchFn: typeof fetch, hash = ''): void {
	ort.url = new URL(`http://localhost:8787/projects/dashboard${hash}`);
	render(UnterDashboard, { slug: 'dashboard', fetchFn });
}

/** Der Stand mit Ampel gelb; ohne Anmeldung kommen nur diese Zahlen (Etappe 3a+). */
const gelb = beispielStand({
	gesamt: { ...beispielStand().gesamt, ampel: 'gelb', grund: 'wartet' },
	zahlen: beispielZahlen({
		wartet_auf_dich: 2,
		aufgaben: 1,
		fragen: 1,
		laeuft: 1,
		naechste_frist: '2026-10-31',
		tage_bis: 2,
		frist_stufe: 'bald'
	}),
	notizen: ['Logo fehlt noch']
});

function karten(): string[] {
	return [...document.querySelectorAll('.karte h2')].map((h) =>
		h.textContent.replace(/\s+/g, ' ').trim()
	);
}

/** Viele Ticks und Makro-Aufgaben: Ein Fokuswechsel nach „gelesen“ hätte jetzt stattgefunden. */
async function ruhe(): Promise<void> {
	for (let i = 0; i < 10; i++) {
		await tick();
		await new Promise((fertig) => setTimeout(fertig, 0));
	}
}

afterEach(() => {
	vi.unstubAllGlobals();
	rueckfall.vergessen();
});

describe('Unter-Dashboard', () => {
	it('mit Anmeldung: Stand, wartet auf dich (Link auf den Anker), Arbeiten, Frist, Notizen', async () => {
		zeige(netz(gelb, [aufgabe, frage, arbeit]));
		await screen.findByText('2 Einträge warten auf dich.');
		expect(karten()).toEqual(['Stand', 'Wartet auf dich 2', 'Arbeiten', 'Fristen', 'Notizen 1']);
		expect(screen.getByRole('img', { name: 'Ampel gelb' })).toBeInTheDocument();
		// Aufgaben verlinken auf ihren Anker im Kasten „Aufgaben für dich“ (dort Haken und Öffnen).
		const links = screen.getAllByRole('link', { name: /Beim Probe-Dienst anmelden/ });
		expect(links.map((a) => a.getAttribute('href'))).toEqual(['#aufgabe-a-1', '#aufgabe-a-1']);
		// Fragen und Arbeiten stehen selbst da, mit Anker zum Aufklappen.
		expect(document.getElementById('frage-schlusssatz')?.tagName).toBe('DETAILS');
		expect(document.getElementById('arbeit-ar-1')).toHaveTextContent('Seite umbauen');
		expect(screen.getByText('31.10.')).toBeInTheDocument();
		expect(screen.getByText(', in 2 Tagen')).toBeInTheDocument();
		expect(screen.getByText('Logo fehlt noch')).toBeInTheDocument();
	});

	it('ohne Anmeldung: nur die Zahlen aus /stand, keine Titel von Aufgaben', async () => {
		zeige(netz(gelb, null));
		await screen.findByText('2 Einträge warten auf dich, davon eine Aufgabe.');
		expect(screen.getByText(SATZ_OHNE_ANMELDUNG)).toBeInTheDocument();
		expect(screen.queryByText('Beim Probe-Dienst anmelden')).toBeNull();
		// Etappe 3a+: ohne Anmeldung keine Einträge, auch keine Arbeiten; nur ihre Zahl.
		expect(document.getElementById('arbeit-ar-1')).toBeNull();
		expect(screen.getByText('und 1 weitere')).toBeInTheDocument();
		// Etappe B2: Notizen nur als Zahl, ohne Text.
		expect(karten()).toContain('Notizen 1');
		expect(screen.getByText(SATZ_NOTIZEN_OHNE_ANMELDUNG)).toBeInTheDocument();
		expect(screen.queryByText('Logo fehlt noch')).toBeNull();
	});

	it('Hotfix 3a-1: liest den Stand über /stand/voll, ohne Anmeldung (401, 403) über /stand', async () => {
		const angemeldet = vi.fn(netz(gelb, [aufgabe, frage]));
		zeige(angemeldet);
		await screen.findByText('2 Einträge warten auf dich.');
		const urls = (f: typeof angemeldet): string[] =>
			f.mock.calls.map(([url]) => (typeof url === 'string' ? url : ''));
		expect(urls(angemeldet)).toContain('/api/projekte/dashboard/stand/voll');
		expect(urls(angemeldet)).not.toContain('/api/projekte/dashboard/stand');
		cleanup();
		for (const status of [401, 403]) {
			const ohne = vi.fn(((url: string) =>
				Promise.resolve(
					url.endsWith('/stand')
						? json(frei(gelb))
						: json({ fehler: 'Nein', art: status === 401 ? 'anmeldung' : 'fremdes-konto' }, status)
				)) as typeof fetch);
			zeige(ohne);
			await screen.findByText('2 Einträge warten auf dich, davon eine Aufgabe.');
			expect(urls(ohne).filter((u) => u.includes('/stand'))).toEqual([
				'/api/projekte/dashboard/stand/voll',
				'/api/projekte/dashboard/stand'
			]);
			cleanup();
			rueckfall.vergessen();
		}
	});

	it('3a-4: nach 401 oder 403 merkt sich die Seite den Rückfall, die nächste Ladung geht gleich über /stand', async () => {
		for (const status of [401, 403]) {
			const ohne = vi.fn(((url: string) =>
				Promise.resolve(
					url.endsWith('/stand')
						? json(frei(gelb))
						: json({ fehler: 'Nein', art: status === 401 ? 'anmeldung' : 'fremdes-konto' }, status)
				)) as typeof fetch);
			zeige(ohne);
			await screen.findByText('2 Einträge warten auf dich, davon eine Aufgabe.');
			cleanup();
			zeige(ohne);
			await screen.findByText('2 Einträge warten auf dich, davon eine Aufgabe.');
			const stand = ohne.mock.calls
				.map(([url]) => (typeof url === 'string' ? url : ''))
				.filter((u) => u.includes('/stand'));
			expect(stand).toEqual([
				'/api/projekte/dashboard/stand/voll',
				'/api/projekte/dashboard/stand',
				'/api/projekte/dashboard/stand'
			]);
			cleanup();
			rueckfall.vergessen();
		}
	});

	it('lässt leere Karten weg; ohne Einträge steht nur der Stand da', async () => {
		zeige(netz(beispielStand(), []));
		await screen.findByText('Nichts wartet auf dich.');
		expect(karten()).toEqual(['Stand']);
	});

	it('ohne Datenspeicher: Notizen und die Frage bleiben, dazu ein Satz', async () => {
		const ohneSpeicher = beispielStand({
			gesamt: { ...beispielStand().gesamt, ampel: 'gelb', grund: 'wartet' },
			notizen: ['Logo fehlt noch', 'Impressum prüfen'],
			speicher_fehlt: true
		});
		zeige(netz(ohneSpeicher, [frage]));
		await screen.findByText('Ein Eintrag wartet auf dich.');
		expect(karten()).toEqual(['Stand', 'Wartet auf dich 1', 'Notizen 2']);
		expect(screen.getByText(/Der Datenspeicher ist nicht offen/)).toBeInTheDocument();
		expect(screen.getByText('Impressum prüfen')).toBeInTheDocument();
		expect(document.getElementById('frage-schlusssatz')).toHaveTextContent('Welcher Schlusssatz?');
	});

	it('mit Datenspeicher kein Satz dazu', async () => {
		zeige(netz(gelb, [aufgabe, frage, arbeit]));
		await screen.findByText('2 Einträge warten auf dich.');
		expect(screen.queryByText(/Der Datenspeicher ist nicht offen/)).toBeNull();
	});

	it('lädt um Mitternacht neu (Frist morgen wird heute)', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		vi.setSystemTime(new Date(2026, 9, 29, 23, 59, 0));
		try {
			const f = vi.fn(netz(gelb, []));
			zeige(f);
			await vi.waitFor(() => {
				expect(f).toHaveBeenCalledTimes(2);
			});
			await vi.advanceTimersByTimeAsync(30_000);
			expect(f).toHaveBeenCalledTimes(2);
			await vi.advanceTimersByTimeAsync(31_000);
			expect(f).toHaveBeenCalledTimes(4);
			expect(f.mock.calls.map(([url]) => (typeof url === 'string' ? url : '')).slice(2)).toEqual([
				'/api/projekte/dashboard/stand/voll',
				'/api/anstehend?projekt=dashboard'
			]);
		} finally {
			vi.useRealTimers();
		}
	});

	it('lässt ohne Stand die ganze Reihe weg', async () => {
		const f = vi.fn(netz(null, null));
		zeige(f);
		// /stand/voll (401), /stand (404), /api/anstehend (401).
		await waitFor(() => {
			expect(f).toHaveBeenCalledTimes(3);
		});
		await Promise.resolve();
		expect(screen.queryByRole('region', { name: 'Überblick über das Projekt' })).toBeNull();
	});

	it('Anker: klappt den Eintrag auf, hebt ihn hervor und gibt ihm den Fokus', async () => {
		const zweite = eintrag({ art: 'arbeit', id: 'ar-2', titel: 'Zweite' });
		const dritte = eintrag({ art: 'arbeit', id: 'ar-3', titel: 'Dritte, eingeklappt' });
		zeige(netz(gelb, [arbeit, zweite, dritte]), '#arbeit-ar-3');
		const ziel = await waitFor(() => {
			const el = document.getElementById('arbeit-ar-3');
			expect(el).toHaveFocus();
			if (el === null) throw new Error('Ziel fehlt');
			return el;
		});
		expect(ziel).toHaveClass('ziel-hervorgehoben');
		expect(ziel).toHaveAttribute('tabindex', '-1');
		expect(ziel).toHaveProperty('open', true);
		// „und 1 weitere“ ist mit aufgeklappt.
		expect(ziel.closest('details.mehr')).toHaveProperty('open', true);
		expect(within(ziel).getByText(/Stand: läuft/)).toBeInTheDocument();
		expect(screen.queryByRole('status')).toBeNull();
	});

	it('Etappe 3b-2: derselbe Anker noch einmal klappt einen zugeklappten Eintrag wieder auf; Klick mit Taste oder auf eine andere Seite nicht', async () => {
		const dritte = eintrag({ art: 'arbeit', id: 'ar-3', titel: 'Dritte, eingeklappt' });
		const zweite = eintrag({ art: 'arbeit', id: 'ar-2', titel: 'Zweite' });
		// Die Adresse des Browsers steht schon auf dem Anker (wie nach dem ersten Klick).
		history.replaceState(null, '', '/projects/dashboard#arbeit-ar-3');
		zeige(netz(gelb, [arbeit, zweite, dritte]), '#arbeit-ar-3');
		const ziel = await waitFor(() => {
			const el = document.getElementById('arbeit-ar-3');
			expect(el).toHaveFocus();
			if (!(el instanceof HTMLDetailsElement)) throw new Error('Ziel fehlt');
			return el;
		});
		/** Ein Link wie in der Karte „Fristen“; navigiert im Test nicht wirklich. */
		const link = (href: string): HTMLAnchorElement => {
			const a = document.createElement('a');
			a.href = href;
			a.textContent = 'zum Eintrag';
			a.addEventListener('click', (e) => {
				e.preventDefault();
			});
			document.body.append(a);
			return a;
		};
		const zuklappen = (): void => {
			ziel.open = false;
			ziel.classList.remove('ziel-hervorgehoben');
			ziel.blur();
		};
		const gleich = link('/projects/dashboard#arbeit-ar-3');

		// Von Hand zugeklappt, derselbe Link noch einmal: wieder offen, hervorgehoben, Fokus.
		zuklappen();
		await fireEvent.click(gleich);
		expect(ziel).toHaveProperty('open', true);
		expect(ziel).toHaveClass('ziel-hervorgehoben');
		expect(ziel).toHaveFocus();

		// Mit Strg (neuer Tab) oder Umschalt: nichts.
		zuklappen();
		await fireEvent.click(gleich, { ctrlKey: true });
		await fireEvent.click(gleich, { shiftKey: true });
		expect(ziel).toHaveProperty('open', false);
		expect(ziel).not.toHaveClass('ziel-hervorgehoben');

		// Link auf eine andere Seite mit demselben Anker: nichts.
		await fireEvent.click(link('/projects/andere#arbeit-ar-3'));
		expect(ziel).toHaveProperty('open', false);
		// Ein anderer Anker geht über die Adresse (Effekt), nicht über den Klick.
		await fireEvent.click(link('/projects/dashboard#arbeit-ar-2'));
		expect(ziel).toHaveProperty('open', false);
		expect(document.getElementById('arbeit-ar-2')).not.toHaveClass('ziel-hervorgehoben');
		history.replaceState(null, '', '/');
	});

	it('Widgets aus dem Katalog (3a-3): eine Frage auf einmal (Auswahl in anstehend.ts), Vermerk zu den übrigen, Anker klappt auf', async () => {
		// Etappe 3a+: der Server schickt alle offenen Fragen; die Seite zeigt die ältere
		// (gleiche Stufe), die zweite nur im Vermerk, wie früher die Antwort des Servers.
		const zweite = eintrag({
			art: 'frage',
			id: 'zweite',
			titel: 'Zweite Frage?',
			gestellt: '2026-10-27',
			erstellt: '2026-10-27'
		});
		zeige(netz(gelb, [zweite, { ...frage, erstellt: '2026-10-25' }]), '#frage-schlusssatz');
		const ziel = await waitFor(() => {
			const el = document.getElementById('frage-schlusssatz');
			expect(el).toHaveFocus();
			if (el === null) throw new Error('Ziel fehlt');
			return el;
		});
		expect(ziel).toHaveAttribute('open');
		expect(ziel).toHaveTextContent('noch 1 Frage wartet');
		expect(document.getElementById('frage-zweite')).toBeNull();
		// Stand mit den Zahlen aus anstehend.ts (je Projekt zählt eine Frage, Etappe 3a+).
		expect(document.querySelector('.stand')).toHaveTextContent('1 wartet auf dich');
		expect(document.querySelector('.stand')).toHaveClass('b-wartet');
	});

	it('Anker fehlt (erledigt): kurzer Satz', async () => {
		zeige(netz(gelb, [arbeit]), '#arbeit-ar-9');
		expect(await screen.findByRole('status')).toHaveTextContent(SATZ_NICHT_MEHR_DA);
	});

	it('Anker auf eine erledigte Aufgabe: Satz, sobald die Aufgaben geladen sind', async () => {
		vi.stubGlobal('fetch', () => Promise.resolve(json({ aufgaben: [] })));
		await aufgaben.laden();
		expect(aufgaben.stand).toBe('da');
		zeige(netz(gelb, [arbeit]), '#aufgabe-a-77');
		expect(await screen.findByRole('status')).toHaveTextContent(SATZ_NICHT_MEHR_DA);
	});
	it('die Frage steht offen da und lässt sich direkt beantworten; danach lädt die Reihe neu', async () => {
		const jaNein = eintrag({
			art: 'frage',
			id: 'weiter',
			titel: 'Weitermachen?',
			gestellt: '2026-10-26',
			frage: { art: 'ja-nein', optionen: [], empfehlung: null }
		});
		const gesendet: string[] = [];
		vi.stubGlobal('fetch', (url: string) => {
			gesendet.push(url);
			return Promise.resolve(json({ antwort: {}, geaendert: true }));
		});
		const basis = netz(gelb, [jaNein]);
		const fetchFn = vi.fn(basis);
		zeige(fetchFn);
		const knopf = await screen.findByRole('button', { name: 'Ja' });
		expect(document.getElementById('frage-weiter')).toHaveAttribute('open');
		const vorher = fetchFn.mock.calls.length;
		await fireEvent.click(knopf);
		await waitFor(() => {
			expect(document.querySelector('.antwort-status')).toHaveTextContent(SATZ_GESENDET);
		});
		expect(gesendet).toEqual(['/api/antworten/dashboard/weiter']);
		expect(fetchFn.mock.calls.length).toBeGreaterThan(vorher);
	});

	it('nach der Antwort liegt der Fokus auf der Überschrift der Reihe, außerhalb der Ansage; sie bleibt auch ohne Frage und nach 15 s', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		try {
			const jaNein = eintrag({
				art: 'frage',
				id: 'weiter',
				titel: 'Weitermachen?',
				gestellt: '2026-10-26',
				frage: { art: 'ja-nein', optionen: [], empfehlung: null }
			});
			vi.stubGlobal('fetch', () => Promise.resolve(json({ antwort: {}, geaendert: true })));
			// Nach der Antwort: kein Stand und keine Einträge mehr, die Reihe hätte nichts zu zeigen.
			let beantwortet = false;
			const vorher = netz(gelb, [jaNein]);
			const nachher = netz(null, []);
			zeige(((url: string) => (beantwortet ? nachher(url) : vorher(url))) as typeof fetch);
			const knopf = await screen.findByRole('button', { name: 'Ja' });
			// Fokusziel ist die Überschrift (nicht der ganze Abschnitt, Auflage aus A2).
			const abschnitt = screen.getByRole('region', { name: 'Überblick über das Projekt' });
			const huelle = within(abschnitt).getByRole('heading', { name: 'Überblick über das Projekt' });
			expect(huelle).toHaveAttribute('tabindex', '-1');
			beantwortet = true;
			knopf.focus();
			await fireEvent.click(knopf);
			await vi.waitFor(() => {
				expect(screen.queryByRole('button', { name: 'Ja' })).toBeNull();
			});
			const bereich = document.querySelector('.antwort-status');
			expect(bereich).toHaveTextContent(SATZ_GESENDET);
			expect(document.activeElement).toBe(huelle);
			expect(bereich?.contains(huelle)).toBe(false);
			// Die Ansage steht außerhalb des Abschnitts, immer im Dokument (Auflage aus A2).
			expect(abschnitt.contains(bereich)).toBe(false);
			expect(document.querySelector('.karte')).toBeNull();
			vi.advanceTimersByTime(ZEIGEN_MS);
			await tick();
			expect(bereich?.textContent.trim()).toBe('');
			expect(screen.getByRole('region', { name: 'Überblick über das Projekt' })).toBe(abschnitt);
			expect(document.activeElement).toBe(huelle);
		} finally {
			vi.useRealTimers();
		}
	});
});

describe('Unter-Dashboard: Aktualisierungen und Meldungen (Etappe A3)', () => {
	const akt = (teil: Partial<Aktualisierung>): Aktualisierung => ({
		quelle: 'meldung',
		id: 'm-1',
		art: 'fehler',
		titel: 'Der Bau der Oberfläche ist rot.',
		zeit: '2026-10-30T09:05:00+01:00',
		ziel: '/projects/dashboard#meldung-m-1',
		gelesen: false,
		anzahl: 2,
		behoben: false,
		text: 'npm run build ist gescheitert.',
		...teil
	});
	const neu = akt({});
	const vorgang = akt({
		quelle: 'vorgang',
		id: 'v-20261030-0001',
		art: 'antwort',
		titel: 'Antwort auf „Weiter?“: ja',
		zeit: '2026-10-30T08:00:00+01:00',
		ziel: '/vorgaenge#vorgang-v-20261030-0001',
		gelesen: true,
		anzahl: 1,
		text: null
	});
	const alt = akt({
		id: 'm-0',
		art: 'info',
		titel: 'Bericht liegt bereit',
		zeit: '2026-10-29T08:00:00+01:00',
		ziel: '/projects/dashboard#meldung-m-0',
		gelesen: true,
		anzahl: 1,
		text: null
	});
	const meldungEintrag = (id: string, titel: string): AnstehendEintrag => {
		const e = eintrag({
			art: 'meldung',
			id,
			titel,
			zustand: 'ungelesen',
			wartet_auf: null,
			ziel: `/projects/dashboard#meldung-${id}`,
			meldung: {
				art: 'frist',
				anzahl: 1,
				zuletzt: '2026-10-28T08:00:00+01:00',
				text: null,
				system: false
			}
		});
		e.stufe = { vorschlag: 'jetzt', inhaber: null, wirksam: 'jetzt', grund: 'fest' };
		return e;
	};

	/** fetch mit Aktualisierungen in der Liste; „gelesen“ wird gemerkt und nimmt die Meldung als gelesen. */
	function netzMit(aktualisierungen: Aktualisierung[], eintraege: AnstehendEintrag[] = []) {
		const gelesen: string[] = [];
		let stand = aktualisierungen;
		const fetchFn = ((url: string, init?: RequestInit) => {
			if (init?.method === 'POST') {
				const id = /\/meldungen\/([^/]+)\/gelesen$/.exec(url)?.[1] ?? '';
				gelesen.push(id);
				stand = stand.map((a) => (a.id === id ? { ...a, gelesen: true } : a));
				return Promise.resolve(json({ meldung: {}, geaendert: true }));
			}
			if (url.endsWith('/stand') || url.endsWith('/stand/voll'))
				return Promise.resolve(json(beispielStand()));
			return Promise.resolve(
				json({
					heute: '2026-10-30',
					frist_bald_tage: 7,
					eintraege: eintraege.filter((e) => !gelesen.includes(e.id)),
					hinweise: [],
					aktualisierungen: stand
				})
			);
		}) as typeof fetch;
		return { fetchFn, gelesen };
	}

	it('Abnahme 3a+: „N neu“ zählt alle ungelesenen Meldungen des Projekts, auch über die fünf angezeigten hinaus', async () => {
		const viele = Array.from({ length: 7 }, (_, n) =>
			akt({ id: `m-v${String(n)}`, zeit: `2026-10-30T1${String(n)}:00:00+01:00`, gelesen: false })
		);
		zeige(netzMit([...viele, alt]).fetchFn);
		const titel = await screen.findByRole('heading', { name: /Aktualisierungen/ });
		expect(titel).toHaveTextContent('Aktualisierungen 7 neu');
		const karte = titel.closest('.karte');
		if (!(karte instanceof HTMLElement)) throw new Error('Karte fehlt');
		// Angezeigt sind fünf (je mit „gelesen“), gezählt sind alle sieben.
		expect(within(karte).getAllByRole('button', { name: /^Gelesen:/ })).toHaveLength(5);
	});

	it('Karte „Aktualisierungen“: neueste zuerst, ungelesene hervorgehoben, Vorgänge mit Link', async () => {
		zeige(netzMit([neu, vorgang, alt]).fetchFn);
		const titel = await screen.findByRole('heading', { name: /Aktualisierungen/ });
		expect(titel).toHaveTextContent('Aktualisierungen 1 neu');
		const karte = titel.closest('.karte');
		if (!(karte instanceof HTMLElement)) throw new Error('Karte fehlt');
		const zeilen = within(karte)
			.getAllByRole('listitem')
			.filter((li) => li.parentElement?.classList.contains('liste'));
		expect(zeilen.map((z) => z.classList.contains('ungelesen'))).toEqual([true, false, false]);
		expect(document.getElementById('meldung-m-1')).toHaveTextContent(
			'Fehler · 30.10. 09:05, 2-mal'
		);
		expect(document.getElementById('meldung-m-1')).toHaveTextContent(
			'npm run build ist gescheitert.'
		);
		expect(
			within(karte).getByRole('link', { name: 'Antwort auf „Weiter?“: ja' }).getAttribute('href')
		).toBe('/vorgaenge#vorgang-v-20261030-0001');
		expect(within(karte).getByText(/Antwort, 30\.10\. 08:00/)).toBeInTheDocument();
	});

	it('Anker #meldung-<id>: klappt auf und gibt den Fokus, auch für eine ungelesene außerhalb der fünf', async () => {
		const weiter = meldungEintrag('m-9', 'Frist morgen: Steuer');
		zeige(netzMit([neu, vorgang, alt], [weiter]).fetchFn, '#meldung-m-9');
		await waitFor(() => {
			expect(document.getElementById('meldung-m-9')).toHaveFocus();
		});
		const ziel = document.getElementById('meldung-m-9');
		expect(ziel).toHaveProperty('open', true);
		expect(ziel?.closest('details.mehr')).toHaveProperty('open', true);
		expect(screen.getByText('und 1 weitere ungelesene')).toBeInTheDocument();
	});

	it('„gelesen“ in der Karte: schickt es, lädt neu, Fokus auf die Überschrift der Karte', async () => {
		const n = netzMit([neu, vorgang]);
		zeige(n.fetchFn, '#meldung-m-1');
		const knopf = await screen.findByRole('button', {
			name: 'Gelesen: Der Bau der Oberfläche ist rot.'
		});
		knopf.focus();
		await fireEvent.click(knopf);
		await waitFor(() => {
			expect(n.gelesen).toEqual(['m-1']);
		});
		await waitFor(() => {
			expect(screen.queryByRole('button', { name: /^Gelesen:/ })).toBeNull();
		});
		await waitFor(() => {
			expect(document.activeElement).toBe(
				screen.getByRole('heading', { name: /^Aktualisierungen/ })
			);
		});
	});

	it('Fokus nicht verschoben: wer während „gelesen“ woanders hinklickt, behält seinen Fokus', async () => {
		const n = netzMit([neu, vorgang]);
		let weiter: () => void = () => undefined;
		const halt = new Promise<void>((fertig) => {
			weiter = fertig;
		});
		const fetchFn = (async (url: string, init?: RequestInit) => {
			if (init?.method === 'POST') await halt;
			return n.fetchFn(url, init);
		}) as typeof fetch;
		zeige(fetchFn, '#meldung-m-1');
		const knopf = await screen.findByRole('button', {
			name: 'Gelesen: Der Bau der Oberfläche ist rot.'
		});
		knopf.focus();
		await fireEvent.click(knopf);
		const anders = document.createElement('input');
		document.body.append(anders);
		anders.focus();
		weiter();
		await waitFor(() => {
			expect(n.gelesen).toEqual(['m-1']);
		});
		await waitFor(() => {
			expect(screen.queryByRole('button', { name: /^Gelesen:/ })).toBeNull();
		});
		expect(knopf.isConnected).toBe(false);
		await ruhe();
		await waitFor(() => {
			expect(document.activeElement).toBe(anders);
		});
		anders.remove();
	});

	it('ohne Anmeldung: in „Aktualisierungen“ nur die Zahlen, keine Titel und Texte', async () => {
		const stand = beispielStand({ zahlen: beispielZahlen({ meldungen: { neu: 2, gesamt: 3 } }) });
		zeige(netz(stand, null), '#meldung-m-1');
		const titel = await screen.findByRole('heading', { name: /Aktualisierungen/ });
		expect(titel).toHaveTextContent('Aktualisierungen 2 neu');
		const karte = titel.closest('.karte');
		if (!(karte instanceof HTMLElement)) throw new Error('Karte fehlt');
		expect(karte).toHaveTextContent('3 Meldungen, davon 2 neu.');
		expect(within(karte).getByText(SATZ_OHNE_ANMELDUNG)).toBeInTheDocument();
		expect(within(karte).queryByRole('listitem')).toBeNull();
		expect(screen.queryByRole('button', { name: /^Gelesen:/ })).toBeNull();
		// Ein Anker auf eine Meldung: nicht „nicht mehr da“, sondern der Satz zur Anmeldung.
		await waitFor(() => {
			expect(document.querySelector('.anker-satz')).toHaveTextContent(SATZ_OHNE_ANMELDUNG);
		});
	});

	it('ohne Anmeldung und ohne Meldungen: keine Karte „Aktualisierungen“', async () => {
		zeige(netz(beispielStand(), null));
		await screen.findByText('Nichts wartet auf dich.');
		expect(screen.queryByRole('heading', { name: /Aktualisierungen/ })).toBeNull();
	});

	it('Ampel „unbekannt“ ohne Datenspeicher (Auflage aus A1)', async () => {
		const stand = beispielStand({
			gesamt: {
				...beispielStand().gesamt,
				ampel: 'unbekannt',
				grund: 'speicher-fehlt'
			},
			speicher_fehlt: true
		});
		zeige(netz(stand, null));
		expect(await screen.findByRole('img', { name: 'Ampel unbekannt' })).toBeInTheDocument();
	});

	it('eine späte Antwort überschreibt keine neuere (Auflage aus A1)', async () => {
		let erste: (r: Response) => void = () => undefined;
		let anfragen = 0;
		const spaet = beispielStand({
			notizen: ['Alter Stand.']
		});
		const frisch = beispielStand({
			notizen: ['Neuer Stand.']
		});
		const fetchFn = ((url: string) => {
			if (url.endsWith('/stand')) {
				anfragen += 1;
				if (anfragen === 1)
					return new Promise<Response>((fertig) => {
						erste = fertig;
					});
				return Promise.resolve(json(frisch));
			}
			return Promise.resolve(json({ fehler: 'Anmeldung', art: 'anmeldung' }, 401));
		}) as typeof fetch;
		const { live } = await import('#lib/live.svelte.ts');
		let hoerer: ((c: ChangeEvent) => void) | null = null;
		const spion = vi.spyOn(live, 'onChange').mockImplementation((l) => {
			hoerer = l;
			return () => undefined;
		});
		zeige(fetchFn);
		// Eine Änderung kommt über den Live-Kanal, bevor die erste Antwort da ist.
		const melden = hoerer as ((c: ChangeEvent) => void) | null;
		melden?.({ area: 'anstehend', path: 'anstehend/dashboard', at: '' });
		await waitFor(() => {
			expect(anfragen).toBe(2);
		});
		await screen.findByText('Neuer Stand.');
		erste(json(spaet));
		await new Promise((r) => setTimeout(r, 0));
		await tick();
		expect(screen.getByText('Neuer Stand.')).toBeInTheDocument();
		expect(screen.queryByText('Alter Stand.')).toBeNull();
		spion.mockRestore();
	});

	it('Live-Lücken: ein Vorgang dieses Projekts lädt „Zuletzt“ neu, ein fremder nicht', async () => {
		let anfragen = 0;
		const fetchFn = ((url: string) => {
			if (url.endsWith('/stand')) {
				anfragen += 1;
				return Promise.resolve(json(beispielStand()));
			}
			return Promise.resolve(json({ fehler: 'Anmeldung', art: 'anmeldung' }, 401));
		}) as typeof fetch;
		const { live } = await import('#lib/live.svelte.ts');
		let hoerer: ((v: VorgangEvent) => void) | null = null;
		const spion = vi.spyOn(live, 'onVorgang').mockImplementation((l) => {
			hoerer = l;
			return () => undefined;
		});
		zeige(fetchFn);
		await waitFor(() => {
			expect(anfragen).toBe(1);
		});
		const melden = hoerer as ((v: VorgangEvent) => void) | null;
		const v = { id: 'v-1', art: 'abnahme', zustand: 'offen', nr: 7 } as const;
		melden?.({ ...v, slug: 'anderes' });
		await tick();
		expect(anfragen).toBe(1);
		melden?.({ ...v, slug: 'dashboard', nr: 8 });
		await waitFor(() => {
			expect(anfragen).toBe(2);
		});
		spion.mockRestore();
	});
});
