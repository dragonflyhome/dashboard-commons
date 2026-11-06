import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SATZ_OHNE_ANMELDUNG } from '#lib/anstehend.ts';
import { SATZ_GESENDET } from '#lib/antworten.ts';
import { beispielListe, eintrag, json } from '#lib/anstehend.beispiel.ts';

const { ort, goto } = vi.hoisted(() => ({
	ort: { url: new URL('http://localhost:8787/anstehend') },
	goto: vi.fn()
}));
vi.mock('$app/state', () => ({ page: ort }));
vi.mock('$app/navigation', () => ({ goto }));
vi.mock('$app/paths', () => ({
	resolve: (p: string, o?: Record<string, string>) =>
		o ? p.replace(/\[(\w+)\]/g, (_m: string, k: string) => o[k] ?? '') : p
}));

const { default: Page } = await import('./+page.svelte');
const { anstehend } = await import('#lib/anstehend.svelte.ts');

interface Anfrage {
	url: string;
	methode: string;
	koerper: unknown;
}

/** Gestubbtes fetch: GET /api/anstehend liefert die Liste (oder 401), POSTs werden gemerkt. */
function netz(status = 200): Anfrage[] {
	const anfragen: Anfrage[] = [];
	vi.stubGlobal('fetch', (url: string, init?: RequestInit) => {
		const methode = init?.method ?? 'GET';
		const koerper = typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : null;
		anfragen.push({ url, methode, koerper });
		if (methode === 'POST' && url === '/api/einstufung')
			return Promise.resolve(json({ einstufung: koerper, geaendert: true }));
		if (methode === 'POST') return Promise.resolve(json({ aufgabe: {}, geaendert: true }));
		if (url === '/api/anstehend')
			return Promise.resolve(
				status === 200
					? json(beispielListe())
					: json({ fehler: 'Anmeldung', art: 'anmeldung' }, status)
			);
		return Promise.resolve(json({ offen: 0, aufgaben: [] }));
	});
	return anfragen;
}

function zeige(suche = ''): void {
	ort.url = new URL(`http://localhost:8787/anstehend${suche}`);
	render(Page);
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
	goto.mockClear();
});

describe('Seite „Anstehend“', () => {
	it('zeigt die Zahlen und gruppiert nach Priorität, darin nach Frist', async () => {
		netz();
		zeige();
		const zahlen = await screen.findByRole('list', { name: 'Anzahl nach Stufe' });
		expect(zahlen).toHaveTextContent('2 Jetzt');
		expect(zahlen).toHaveTextContent('1 Bald');
		expect(zahlen).toHaveTextContent('1 Irgendwann');
		expect(zahlen).toHaveTextContent('3 wartet auf dich');
		const gruppen = screen.getAllByRole('heading', { level: 2 }).map((h) => h.textContent.trim());
		expect(gruppen).toEqual(['Jetzt 2', 'Bald 1', 'Irgendwann 1']);
		const jetzt = screen.getByRole('region', { name: /Jetzt/ });
		const titel = within(jetzt)
			.getAllByRole('listitem')
			.map((li) => li.querySelector('.titel')?.textContent)
			.filter(Boolean);
		// Frist 30.10. vor Frist 02.11.
		expect(titel).toEqual(['Welcher Schlusssatz passt?', 'Beim Probe-Dienst anmelden']);
		expect(within(jetzt).getByText(/bis 30\.10\., morgen/)).toBeInTheDocument();
		expect(within(jetzt).getByText('wartet auf deine Antwort')).toBeInTheDocument();
		expect(within(jetzt).getAllByText(/Bienen und Honig/).length).toBeGreaterThan(0);
	});

	it('gruppiert nach Schwerpunkt in der Reihenfolge der Antwort, darunter die Projekte', async () => {
		netz();
		zeige('?ansicht=schwerpunkt');
		await screen.findByRole('heading', { level: 2, name: /Chor und Verein/ });
		const h2 = screen.getAllByRole('heading', { level: 2 }).map((h) => h.textContent.trim());
		expect(h2).toEqual(['Chor und Verein 2', 'Bienen und Honig 2']);
		const h3 = screen.getAllByRole('heading', { level: 3 }).map((h) => h.textContent.trim());
		expect(h3).toEqual(['Beispiel', 'Dashboard']);
		expect(screen.getByRole('button', { name: 'nach Schwerpunkt' })).toHaveAttribute(
			'aria-pressed',
			'true'
		);
	});

	it('liest die Filter aus der Adresse und schreibt Änderungen hinein', async () => {
		netz();
		zeige('?art=frage&nur=1');
		await screen.findByText('1 von 4 Einträgen');
		expect(screen.getByText('Welcher Schlusssatz passt?')).toBeInTheDocument();
		expect(screen.queryByText('Seite umbauen')).toBeNull();
		expect(screen.getByRole('button', { name: 'Fragen' })).toHaveAttribute('aria-pressed', 'true');
		expect(screen.getByRole('button', { name: 'nur was auf mich wartet' })).toHaveAttribute(
			'aria-pressed',
			'true'
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Arbeiten' }));
		expect(goto).toHaveBeenLastCalledWith('/anstehend?art=frage,arbeit&nur=1', { reset: false });
		await fireEvent.click(screen.getByRole('button', { name: 'nach Schwerpunkt' }));
		expect(goto).toHaveBeenLastCalledWith('/anstehend?ansicht=schwerpunkt&art=frage&nur=1', {
			reset: false
		});
	});

	it('Umstufen schickt POST /api/einstufung und lädt neu', async () => {
		const anfragen = netz();
		zeige();
		const gruppe = await screen.findByRole('group', { name: 'Wann dran? Seite umbauen' });
		const vorher = anfragen.filter((a) => a.url === '/api/anstehend').length;
		await fireEvent.click(within(gruppe).getByRole('button', { name: 'Jetzt' }));
		await waitFor(() => {
			expect(anfragen.filter((a) => a.url === '/api/anstehend').length).toBe(vorher + 1);
		});
		const post = anfragen.find((a) => a.methode === 'POST');
		expect(post).toEqual({
			url: '/api/einstufung',
			methode: 'POST',
			koerper: { projekt: 'dashboard', art: 'arbeit', id: 'ar-1', stufe: 'jetzt' }
		});
		// Vorschlag des Chats: erst möglich, wenn der Inhaber eingestuft hat.
		expect(within(gruppe).getByRole('button', { name: /^Wie vorgeschlagen/ })).toBeDisabled();
		// Fragen tragen „gestellt“ mit.
		const frage = screen.getByRole('group', { name: 'Wann dran? Welcher Schlusssatz passt?' });
		await fireEvent.click(within(frage).getByRole('button', { name: 'Irgendwann' }));
		await waitFor(() => {
			expect(anfragen.filter((a) => a.methode === 'POST')).toHaveLength(2);
		});
		expect(anfragen.filter((a) => a.methode === 'POST')[1]?.koerper).toEqual({
			projekt: 'beispiel',
			art: 'frage',
			id: 'schlusssatz',
			stufe: 'irgendwann',
			gestellt: '2026-10-25'
		});
	});

	it('hat bei Aufgaben den Haken (Weg aus 5a), aber keine Klick-Links', async () => {
		const anfragen = netz();
		zeige();
		await screen.findByText('Beim Probe-Dienst anmelden');
		// Nur Links auf Projektseiten im Dashboard, mit Anker; kein neuer Tab, nichts Fremdes.
		// (Die Zahl-Kacheln oben sind Links innerhalb von „Anstehend“, eigener Test unten.)
		const links = screen.getAllByRole('link').filter((a) => !a.classList.contains('zahl-kachel'));
		expect(links.length).toBe(4);
		for (const a of links) {
			expect(a.getAttribute('href')).toMatch(/^\/projects\/[a-z-]+#(aufgabe|frage|arbeit)-/);
			expect(a).not.toHaveAttribute('target');
		}
		expect(screen.queryByRole('link', { name: /Öffnen/ })).toBeNull();
		expect(screen.getAllByRole('link', { name: 'Beispiel' })[0]).toHaveAttribute(
			'href',
			'/projects/beispiel#frage-schlusssatz'
		);
		await fireEvent.click(
			screen.getByRole('button', { name: 'Erledigt: Beim Probe-Dienst anmelden' })
		);
		await waitFor(() => {
			expect(anfragen.some((a) => a.url === '/api/aufgaben/a-1/erledigt')).toBe(true);
		});
	});

	it('die Zahl-Kacheln sind Links auf ihr Ziel', async () => {
		netz();
		zeige();
		await screen.findByText('Beim Probe-Dienst anmelden');
		const kacheln = screen.getAllByRole('link').filter((a) => a.classList.contains('zahl-kachel'));
		expect(kacheln.map((a) => a.getAttribute('href'))).toEqual([
			'/anstehend#stufe-jetzt',
			'/anstehend#stufe-bald',
			'/anstehend#stufe-irgendwann',
			'/anstehend?nur=1'
		]);
		expect(kacheln[3]).toHaveAccessibleName(/wartet auf dich/);
		// Die Ziele gibt es: je Stufe eine Überschrift mit dieser Kennung.
		expect(document.getElementById('stufe-jetzt')).not.toBeNull();
	});

	it('mit Filter zählen die Kacheln dasselbe wie die Gruppen darunter', async () => {
		netz();
		zeige('?art=aufgabe');
		const zahlen = await screen.findByRole('list', { name: 'Anzahl nach Stufe' });
		const kacheln = within(zahlen).getAllByRole('link');
		const zahl = (a: HTMLElement | undefined): string =>
			a?.querySelector('.zahl')?.textContent ?? '';
		for (const stufe of ['jetzt', 'bald', 'irgendwann']) {
			const kachel = kacheln.find((a) => a.getAttribute('href')?.endsWith(`#stufe-${stufe}`));
			const kopf = document.getElementById(`stufe-${stufe}`);
			expect(kachel, stufe).toBeDefined();
			expect(kopf?.querySelector('.anzahl')?.textContent, stufe).toBe(zahl(kachel));
		}
		// Der Filter bleibt im Link.
		expect(kacheln[0]).toHaveAttribute('href', '/anstehend?art=aufgabe#stufe-jetzt');
	});

	it('nach einer Antwort liegt der Fokus auf der Überschrift, außerhalb der Ansage', async () => {
		const anfragen = netz();
		zeige();
		const feld = await screen.findByLabelText('Deine Antwort');
		await fireEvent.input(feld, { target: { value: 'Der zweite.' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Absenden' }));
		const titel = screen.getByRole('heading', { level: 1, name: 'Anstehend' });
		expect(titel).toHaveAttribute('tabindex', '-1');
		await waitFor(() => {
			expect(document.activeElement).toBe(titel);
		});
		expect(anfragen.some((a) => a.url === '/api/antworten/beispiel/schlusssatz')).toBe(true);
		const bereich = document.querySelector('.antwort-status');
		expect(bereich).toHaveAttribute('aria-live', 'polite');
		expect(bereich).toHaveTextContent(SATZ_GESENDET);
		expect(bereich?.contains(titel)).toBe(false);
	});

	it('zeigt ohne Anmeldung (401) nur den Satz', async () => {
		netz(401);
		zeige();
		// Etappe 3b: die Adresse ist ein Link (auf dieselbe Seite unter localhost).
		const link = await screen.findByRole('link', { name: 'http://localhost:8787' });
		expect(link.closest('p')?.textContent).toBe(`${SATZ_OHNE_ANMELDUNG}.`);
		expect(screen.getAllByText(/Microsoft Edge/)).toHaveLength(1);
		expect(SATZ_OHNE_ANMELDUNG).toBe(
			'Anstehendes siehst du in Microsoft Edge unter http://localhost:8787'
		);
		expect(screen.queryByRole('list', { name: 'Anzahl nach Stufe' })).toBeNull();
		expect(screen.queryAllByRole('button')).toHaveLength(0);
	});
});

describe('Meldungen, Tab-Titel und Zahl (Etappe A3)', () => {
	/** Drei Einträge unter Jetzt (zwei aus dem Beispiel und die Meldung); gezählt wird im Web (Etappe 3a+). */
	function mitMeldung(): ReturnType<typeof beispielListe> {
		const l = beispielListe();
		const m = eintrag({
			art: 'meldung' as const,
			id: 'm-20261030-0001',
			titel: 'Der Bau der Oberfläche ist rot.',
			zustand: 'ungelesen',
			wartet_auf: null,
			frist: null,
			tage_bis: null,
			ziel: '/projects/dashboard#meldung-m-20261030-0001',
			meldung: {
				art: 'fehler' as const,
				anzahl: 2,
				zuletzt: '2026-10-30T09:00:00+01:00',
				text: 'npm run build ist gescheitert.',
				system: false
			}
		});
		m.stufe = { vorschlag: 'jetzt', inhaber: null, wirksam: 'jetzt', grund: 'fest' };
		return { ...l, eintraege: [m, ...l.eintraege] };
	}

	it('der Tab-Titel trägt die Zahl der Stufe Jetzt und folgt ohne Neuladen', async () => {
		let liste = mitMeldung();
		vi.stubGlobal('fetch', () => Promise.resolve(json(liste)));
		zeige();
		await screen.findByRole('list', { name: 'Anzahl nach Stufe' });
		await waitFor(() => {
			expect(document.title).toBe('(3) Anstehend · Dashboard');
		});
		// Eine Änderung kommt über den Live-Kanal: Die Liste lädt neu, der Titel folgt.
		liste = {
			...beispielListe(),
			eintraege: beispielListe().eintraege.filter((e) => e.stufe.wirksam !== 'jetzt')
		};
		await anstehend.laden();
		await waitFor(() => {
			expect(document.title).toBe('Anstehend · Dashboard');
		});
	});

	it('Meldung mit „gelesen“ statt Umstufen; danach Fokus auf der Überschrift', async () => {
		let liste = mitMeldung();
		const posts: string[] = [];
		vi.stubGlobal('fetch', (url: string, init?: RequestInit) => {
			if (init?.method === 'POST') {
				posts.push(url);
				liste = beispielListe();
				return Promise.resolve(json({ meldung: {}, geaendert: true }));
			}
			return Promise.resolve(json(liste));
		});
		zeige('?art=meldung');
		const zeile = (await screen.findByText('Der Bau der Oberfläche ist rot.')).closest('li');
		if (zeile === null) throw new Error('Zeile fehlt');
		expect(zeile).toHaveTextContent('Meldung: Fehler');
		expect(zeile).toHaveTextContent('npm run build ist gescheitert.');
		expect(within(zeile).queryByRole('group', { name: /Stufe/ })).toBeNull();
		expect(within(zeile).getByRole('link', { name: 'Dashboard' }).getAttribute('href')).toBe(
			'/projects/dashboard#meldung-m-20261030-0001'
		);
		const knopf = within(zeile).getByRole('button', { name: /^Gelesen:/ });
		knopf.focus();
		await fireEvent.click(knopf);
		await waitFor(() => {
			expect(screen.queryByText('Der Bau der Oberfläche ist rot.')).toBeNull();
		});
		expect(posts).toEqual(['/api/meldungen/m-20261030-0001/gelesen']);
		await waitFor(() => {
			expect(document.activeElement).toBe(
				screen.getByRole('heading', { level: 1, name: 'Anstehend' })
			);
		});
	});

	it('Fokus nicht verschoben: wer während „gelesen“ woanders hinklickt, behält seinen Fokus', async () => {
		let liste = mitMeldung();
		let weiter: () => void = () => undefined;
		const halt = new Promise<void>((fertig) => {
			weiter = fertig;
		});
		vi.stubGlobal('fetch', async (_url: string, init?: RequestInit) => {
			if (init?.method === 'POST') {
				await halt;
				liste = beispielListe();
				return json({ meldung: {}, geaendert: true });
			}
			return json(liste);
		});
		zeige('?art=meldung');
		const zeile = (await screen.findByText('Der Bau der Oberfläche ist rot.')).closest('li');
		if (zeile === null) throw new Error('Zeile fehlt');
		const knopf = within(zeile).getByRole('button', { name: /^Gelesen:/ });
		knopf.focus();
		await fireEvent.click(knopf);
		const anders = document.createElement('input');
		document.body.append(anders);
		anders.focus();
		weiter();
		await waitFor(() => {
			expect(screen.queryByText('Der Bau der Oberfläche ist rot.')).toBeNull();
		});
		expect(knopf.isConnected).toBe(false);
		await ruhe();
		await waitFor(() => {
			expect(document.activeElement).toBe(anders);
		});
		anders.remove();
	});
});

describe('Seite „Anstehend“: Fristen im Archiv (Entscheidung archiv-status)', () => {
	it('zeigt den neutralen Satz mit Link zur Projektseite, ohne Titel und ohne Zählen', async () => {
		const satz = 'Eine Frist in einem archivierten Projekt läuft am 08.11.2026 ab.';
		vi.stubGlobal('fetch', (url: string) => {
			if (url === '/api/anstehend')
				return Promise.resolve(
					json({
						...beispielListe(),
						archiv_fristen: [
							{ satz, frist: '2026-11-08', tage_bis: 7, ziel: '/projects/stiller-umzug' }
						]
					})
				);
			return Promise.resolve(json({ offen: 0, aufgaben: [] }));
		});
		zeige();
		const fristen = await screen.findByRole('list', { name: 'Fristen im Archiv' });
		const zeile = within(fristen).getByRole('listitem');
		expect(zeile).toHaveTextContent(satz);
		expect(within(zeile).getByRole('link', { name: 'Zur Projektseite' })).toHaveAttribute(
			'href',
			'/projects/stiller-umzug'
		);
		// Die Zahlen bleiben die der übrigen Einträge.
		const zahlen = screen.getByRole('list', { name: 'Anzahl nach Stufe' });
		expect(zahlen).toHaveTextContent('2 Jetzt');
		expect(zahlen).toHaveTextContent('3 wartet auf dich');
	});

	it('Auflage B3: Zeilen ohne Datum („hängt an“, „nicht lesbar“) stehen hier, je Satz eine', async () => {
		const ziel = '/projects/stiller-umzug';
		const haengt = 'Eine Frist in einem archivierten Projekt hängt an einem Ereignis.';
		const unlesbar = 'Fristen eines archivierten Projekts lassen sich nicht lesen.';
		vi.stubGlobal('fetch', (url: string) => {
			if (url === '/api/anstehend')
				return Promise.resolve(
					json({
						...beispielListe(),
						archiv_fristen: [
							{ satz: haengt, frist: null, tage_bis: null, ziel },
							{ satz: unlesbar, frist: null, tage_bis: null, ziel }
						]
					})
				);
			return Promise.resolve(json({ offen: 0, aufgaben: [] }));
		});
		zeige();
		// Der gemeinsame Speicher lädt neu; danach stehen genau die zwei Zeilen da.
		await waitFor(() => {
			const liste = screen.getByRole('list', { name: 'Fristen im Archiv' });
			expect(
				within(liste)
					.getAllByRole('listitem')
					.map((z) => z.textContent)
			).toEqual([expect.stringContaining(haengt), expect.stringContaining(unlesbar)]);
		});
		const fristen = screen.getByRole('list', { name: 'Fristen im Archiv' });
		expect(fristen).not.toHaveTextContent('stiller-umzug');
	});

	it('ohne Fristen im Archiv steht nichts davon da', async () => {
		netz();
		zeige();
		await screen.findByRole('list', { name: 'Anzahl nach Stufe' });
		// Der gemeinsame Speicher lädt neu; danach ist der Teil weg.
		await waitFor(() => {
			expect(screen.queryByRole('list', { name: 'Fristen im Archiv' })).toBeNull();
		});
	});
});
