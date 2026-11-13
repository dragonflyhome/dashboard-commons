/**
 * Etappe B8: die vier Teile der Projektseite (`ProjektTeile`) auf der neuen
 * API: Stand (Satz, vier Zahlen als Sprung, Ampel als Wort mit Grund und
 * Link, Bau nur wenn auffällig, Notizen klein), Aufgabenliste (Frage mit
 * Antwort in „Wartet auf dich“), Plan, Zeitleiste (14 Tage, „ältere
 * anzeigen“, ungelesene Meldungen mit „gelesen“), Anker und die Sätze ohne
 * Anmeldung. Jede Regel mit Gegenprobe. Alle Daten erfunden.
 */
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SATZ_NICHT_MEHR_DA, SATZ_OHNE_ANMELDUNG } from '#lib/anstehend.ts';
import { live, type InhaltAenderung } from '#lib/live.svelte.ts';
import { beispielProjektStand, eintrag, json } from '#lib/anstehend.beispiel.ts';
import { SATZ_TEIL_FEHLT } from '#lib/projektteile.ts';
import type { BauLage } from '#lib/bau.svelte.ts';
import type {
	AnstehendEintrag,
	ChangeEvent,
	ProjektStand,
	Verlauf,
	VerlaufEreignis
} from '$vertrag';
import { backlog, projektPlan, tagIso } from '../../../routes/katalog/beispiele.ts';

const { ort } = vi.hoisted(() => ({
	ort: { url: new URL('http://localhost:8787/projects/beispiel') }
}));
vi.mock('$app/state', () => ({ page: ort }));
// Die Aufgaben lädt der Rahmen; hier zählt nur, dass der Kasten in „Wartet auf dich“ steht.
vi.mock('#lib/aufgaben.svelte.ts', () => ({
	aufgaben: {
		stand: 'da',
		meldung: null,
		beschaeftigt: null,
		fuer: () => [],
		haken: vi.fn(),
		rueckmelden: vi.fn()
	}
}));

const { default: ProjektTeile } = await import('./ProjektTeile.svelte');

afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
	ort.url = new URL('http://localhost:8787/projects/beispiel');
});

const frage = eintrag({
	art: 'frage',
	id: 'farbe',
	projekt: 'beispiel',
	titel: 'Soll die Ampel oben stehen?',
	frage: { art: 'wahl', optionen: ['Ja', 'Nein'], empfehlung: null }
});
const meldung = eintrag({
	art: 'meldung',
	id: 'm-1',
	projekt: 'beispiel',
	titel: 'Prüfung meldet einen Fehler',
	meldung: {
		art: 'fehler',
		anzahl: 1,
		zuletzt: '2026-11-13T09:00:00+01:00',
		text: null,
		system: false
	}
});

const stand: ProjektStand = beispielProjektStand({
	slug: 'beispiel',
	gesamt: {
		...beispielProjektStand().gesamt,
		ampel: 'gelb',
		grund: 'wartet-lange',
		bezug: 'Soll die Ampel oben stehen?',
		bezug_art: 'frage',
		tage: 9
	},
	notizen: ['Logo fehlt noch'],
	fertig_7_tage: 6
});

function ereignis(
	kennung: string,
	tage: number,
	mehr: Partial<VerlaufEreignis> = {}
): VerlaufEreignis {
	return {
		kennung,
		zeit: `${tagIso(tage)}T10:00:00+01:00`,
		genauigkeit: 'minute',
		bis: null,
		titel: kennung,
		text: null,
		quelle: 'arbeit',
		art: 'erledigt',
		verweis: null,
		...mehr
	};
}

const verlauf: Verlauf = {
	ereignisse: [
		ereignis('arbeit:ar-1:fertig', 0, { titel: 'Gästeliste abgeschlossen' }),
		ereignis('meldung:m-1', 0, { quelle: 'meldung', titel: 'Prüfung meldet einen Fehler' }),
		ereignis('frist:datei:fest', 20, { quelle: 'frist', art: 'geplant', titel: 'Fest' })
	],
	seit: `${tagIso(-13)}T00:00:00+01:00`,
	bis: `${tagIso(90)}T00:00:00+01:00`,
	hinweise: []
};

/** fetch für alle Wege; `angemeldet: false` gibt auf den geschützten 401; `liste` ersetzt /backlog. */
function netz(
	angemeldet = true,
	eintraege: AnstehendEintrag[] = [frage, meldung],
	liste: () => Promise<Response> = () => Promise.resolve(json(backlog()))
) {
	return vi.fn((eingabe: RequestInfo | URL) => {
		const url =
			typeof eingabe === 'string' ? eingabe : eingabe instanceof URL ? eingabe.href : eingabe.url;
		const geschuetzt = !url.endsWith('/stand');
		if (!angemeldet && geschuetzt) return Promise.resolve(json({ fehler: 'Anmeldung' }, 401));
		if (url.endsWith('/stand/voll'))
			return Promise.resolve(json({ ...stand, sichtbar: true, eintraege }));
		if (url.endsWith('/stand'))
			return Promise.resolve(
				json({
					...stand,
					sichtbar: false,
					eintraege: [],
					notizen: [],
					notizen_zahl: 1,
					zahlen: {
						wartet_auf_dich: 1,
						aufgaben: 0,
						fragen: 1,
						laeuft: 1,
						wartet: 0,
						als_naechstes: 1,
						fertig_7_tage: 6,
						meldungen: { neu: 1, gesamt: 1 },
						naechste_frist: null,
						tage_bis: null,
						frist_stufe: null
					}
				})
			);
		if (url.startsWith('/api/anstehend'))
			return Promise.resolve(
				json({
					heute: tagIso(0),
					frist_bald_tage: 7,
					eintraege,
					hinweise: [],
					aktualisierungen: []
				})
			);
		if (url.endsWith('/backlog')) return liste();
		if (url.endsWith('/plan')) return Promise.resolve(json(projektPlan()));
		if (url.includes('/verlauf?')) return Promise.resolve(json(verlauf));
		if (url.includes('/gelesen')) return Promise.resolve(json({}));
		return Promise.resolve(json({}, 404));
	});
}

function teile(): string[] {
	return [...document.querySelectorAll('section.abschnitt > h2')].map((h) => h.textContent.trim());
}

describe('Die vier Teile der Projektseite (Etappe B8)', () => {
	it('Stand, Aufgabenliste, Plan, Zeitleiste in dieser Reihenfolge, je ein Abschnitt mit h2', async () => {
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz() });
		expect(teile()).toEqual(['Stand', 'Aufgabenliste', 'Plan', 'Zeitleiste']);
		await screen.findByText(/In den letzten 7 Tagen sind 6 fertig geworden/);
	});

	it('Stand: vier Zahlen als Sprung, die Ampel als Wort mit Grund und Link, Notizen klein', async () => {
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz() });
		const fertig = await screen.findByRole('link', { name: '6 fertig in 7 Tagen' });
		expect(fertig).toHaveAttribute('href', '#zeitleiste-beispiel');
		expect(screen.getByRole('link', { name: '1 wartet auf dich' })).toHaveAttribute(
			'href',
			'#liste-dich-beispiel'
		);
		// Ziel jeder Zahl gibt es auf der Seite.
		await screen.findByRole('heading', { name: /Läuft/, level: 3 });
		for (const a of document.querySelectorAll<HTMLAnchorElement>('.vier a')) {
			expect(
				document.getElementById(a.getAttribute('href')?.slice(1) ?? ''),
				a.href
			).not.toBeNull();
		}
		const ampel = screen.getByText('Wartet:').closest('p');
		expect(ampel).toHaveTextContent(
			'Wartet: „Soll die Ampel oben stehen?“ wartet seit 9 Tagen auf dich.'
		);
		expect(within(ampel as HTMLElement).getByRole('link')).toHaveAttribute(
			'href',
			'#liste-dich-beispiel'
		);
		expect(screen.getByText('Notizen (1)')).toBeInTheDocument();
		// Gegenprobe: Ein grüner Bau steht nicht im Stand-Kopf.
		expect(document.querySelector('[data-bau]')).toBeNull();
	});

	it('Aufgabenliste: die Frage mit Antwort in „Wartet auf dich“, mit derselben Zahl wie im Stand', async () => {
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz() });
		const dich = await screen.findByRole('region', { name: /Wartet auf dich/ });
		await within(dich).findByText('Soll die Ampel oben stehen?');
		expect(within(dich).getByRole('heading', { level: 3 })).toHaveTextContent(
			'Wartet auf dich (1)'
		);
		expect(within(dich).getByRole('button', { name: 'Ja' })).toBeInTheDocument();
		expect(document.getElementById('frage-farbe')).not.toBeNull();
	});

	it('Plan: Etappen mit Stand-Wort und Zählung; offene Arbeiten springen in die Aufgabenliste', async () => {
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz() });
		const einladung = await waitFor(() => {
			const el = document.getElementById('etappe-einladung');
			if (el === null) throw new Error('noch nicht da');
			return el;
		});
		expect(einladung).toHaveTextContent(/läuft seit \d\d\.\d\d\./);
		expect(einladung).toHaveTextContent('0 von 2 Arbeiten fertig');
		expect(within(einladung).getByRole('link', { name: 'Rundmail entwerfen' })).toHaveAttribute(
			'href',
			'#arbeit-ar-20261102-0001'
		);
		// Gegenprobe: eine Arbeit aus einem anderen Projekt hat hier keine Zeile, also keinen Link.
		expect(within(einladung).queryByRole('link', { name: 'Druck prüfen' })).toBeNull();
		expect(document.querySelector('[data-streifen]')).toHaveAttribute('aria-hidden', 'true');
		// Der Termin von „Das Fest“ liegt in der Zukunft: rechts steht sein Datum, nicht „heute“.
		expect(document.querySelector('.achse')).not.toHaveTextContent('heute');
	});

	it('ein Anker auf eine fertige Arbeit (etwa aus der Zeitleiste) klappt ihre Etappe im Plan auf', async () => {
		ort.url = new URL('http://localhost:8787/projects/beispiel#arbeit-ar-20261020-0002');
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz() });
		await waitFor(() => {
			expect(document.activeElement?.id).toBe('arbeit-ar-20261020-0002');
		});
		expect(document.getElementById('etappe-vorbereitung')).toHaveAttribute('open');
		expect(screen.queryByRole('status')).toBeNull();
		// Gegenprobe: eine Arbeit aus einem anderen Projekt hat hier kein Ziel (sie steht bei ihrem Projekt).
		expect(document.getElementById('arbeit-ar-20261102-0007')).toBeNull();
	});

	it('ein Anker auf eine Etappe (etwa aus der Zeitleiste) klappt sie im Plan auf', async () => {
		ort.url = new URL('http://localhost:8787/projects/beispiel#etappe-vorbereitung');
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz() });
		await waitFor(() => {
			expect(document.activeElement?.id).toBe('etappe-vorbereitung');
		});
		expect(document.getElementById('etappe-vorbereitung')).toHaveAttribute('open');
		expect(document.querySelector('.anker-satz')).toBeNull();
	});

	it('Zeitleiste: Kommendes oben, ungelesene Meldung eigens mit „gelesen“, nicht doppelt; „Ältere anzeigen“ lädt 400 Tage', async () => {
		const f = netz();
		render(ProjektTeile, { slug: 'beispiel', fetchFn: f });
		await screen.findByText('Gästeliste abgeschlossen');
		expect(f).toHaveBeenCalledWith(
			`/api/projekte/beispiel/verlauf?seit=${tagIso(-13)}`,
			expect.anything()
		);
		expect(screen.getByText('Demnächst')).toBeInTheDocument();
		expect(screen.getAllByText('Prüfung meldet einen Fehler')).toHaveLength(1);
		expect(document.getElementById('meldung-m-1')).not.toBeNull();
		await fireEvent.click(
			screen.getByRole('button', { name: 'Gelesen: Prüfung meldet einen Fehler' })
		);
		await waitFor(() => {
			expect(f).toHaveBeenCalledWith(expect.stringContaining('/gelesen'), expect.anything());
		});
		// Fokus nach „gelesen“: die Zeile „Zeitleiste“ nur zum Vorlesen, nicht der Kasten mit allen Meldungen.
		await waitFor(() => {
			expect(document.activeElement).toHaveTextContent(/^Zeitleiste$/);
		});
		expect(document.activeElement).toHaveClass('nur-vorlesen');
		(document.activeElement as HTMLElement).blur();
		const aelter = screen.getByRole('button', { name: 'Ältere anzeigen' });
		aelter.focus();
		await fireEvent.click(aelter);
		// Der Knopf verschwindet; der Fokus bleibt in der Zeitleiste statt auf body.
		expect(document.activeElement).toHaveTextContent(/^Zeitleiste$/);
		await waitFor(() => {
			expect(f).toHaveBeenCalledWith(
				`/api/projekte/beispiel/verlauf?seit=${tagIso(-399)}`,
				expect.anything()
			);
		});
		expect(screen.queryByRole('button', { name: 'Ältere anzeigen' })).toBeNull();
	});

	it('ein Anker auf eine Arbeit klappt ihre Zeile in der Aufgabenliste auf', async () => {
		ort.url = new URL('http://localhost:8787/projects/beispiel#arbeit-ar-20261102-0001');
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz() });
		await waitFor(() => {
			expect(document.getElementById('arbeit-ar-20261102-0001')).toHaveAttribute('open');
		});
		expect(document.activeElement?.id).toBe('arbeit-ar-20261102-0001');
	});

	it('ohne Anmeldung: Stand mit Zahlen als Text (ohne Ziel kein Link), sonst je Teil der Satz, keine Titel', async () => {
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz(false) });
		await waitFor(() => {
			expect(screen.getAllByText(SATZ_OHNE_ANMELDUNG)).toHaveLength(3);
		});
		await screen.findByText(/In den letzten 7 Tagen/);
		expect(document.querySelector('.vier')).toHaveTextContent('1 wartet auf dich');
		expect(document.querySelectorAll('.vier a, .ampel a')).toHaveLength(0);
		expect(screen.queryByText('Soll die Ampel oben stehen?')).toBeNull();
		expect(screen.getByText('Notizen (1)')).toBeInTheDocument();
	});
});

describe('Bau im Stand-Kopf nur, wenn auffällig', () => {
	it('rot steht da, grün nicht', async () => {
		const { default: BauKopf } = await import('./BauKopf.svelte');
		const lage = (zustand: string, zeile: string): BauLage =>
			({
				kopf: () => ({ zustand, bedeutung: 'ok', zeile, satz: null }),
				schritt: null
			}) as unknown as BauLage;
		const r = render(BauKopf, {
			slug: 'beispiel',
			lage: lage('rot', 'Bau rot'),
			nurAuffaellig: true
		});
		expect(screen.getByText('Bau rot')).toBeInTheDocument();
		r.unmount();
		render(BauKopf, {
			slug: 'beispiel',
			lage: lage('gruen', 'Seite in Ordnung'),
			nurAuffaellig: true
		});
		expect(screen.queryByText('Seite in Ordnung')).toBeNull();
	});
});

/** Wie oft fetch einen Weg gerufen hat, dessen Adresse `teil` enthält. */
function aufrufe(f: ReturnType<typeof netz>, teil: string): number {
	return f.mock.calls.filter(([u]) => typeof u === 'string' && u.includes(teil)).length;
}

const SATZ_LISTE_FEHLT = 'Die Aufgabenliste lässt sich gerade nicht laden.';

/** Eine Antwort, wie eine ältere Fassung des Dienstes sie schickt: ohne `felder`. */
function ohneFelder(x: object, ...felder: string[]): Record<string, unknown> {
	return Object.fromEntries(Object.entries(x).filter(([n]) => !felder.includes(n)));
}

/** Wie `netz`, aber `ersetzt` beantwortet die Wege, deren Adresse einen der Schlüssel enthält. */
function netzMit(ersetzt: Record<string, () => Promise<Response>>) {
	const basis = netz();
	return vi.fn((eingabe: RequestInfo | URL) => {
		const url = typeof eingabe === 'string' ? eingabe : '';
		const weg = Object.keys(ersetzt).find((t) => url.includes(t));
		const antwort = weg === undefined ? undefined : ersetzt[weg];
		return antwort === undefined ? basis(eingabe) : antwort();
	});
}

describe('Ein älterer Dienst: fehlende Wege und Felder (je Teil ein Satz)', () => {
	it('404 auf /plan und 405 auf /verlauf: Plan und Zeitleiste sagen es, Stand und Aufgabenliste bleiben', async () => {
		render(ProjektTeile, {
			slug: 'beispiel',
			fetchFn: netzMit({
				'/plan': () => Promise.resolve(json({ fehler: 'x' }, 404)),
				'/verlauf?': () => Promise.resolve(json({ fehler: 'x' }, 405))
			})
		});
		await waitFor(() => {
			expect(screen.getAllByText(SATZ_TEIL_FEHLT)).toHaveLength(2);
		});
		expect(teile()).toEqual(['Stand', 'Aufgabenliste', 'Plan', 'Zeitleiste']);
		await screen.findByText(/In den letzten 7 Tagen sind 6 fertig geworden/);
		const dich = await screen.findByRole('region', { name: /Wartet auf dich/ });
		expect(await within(dich).findByRole('button', { name: 'Ja' })).toBeInTheDocument();
		expect(screen.queryByText('Der Plan lässt sich gerade nicht laden.')).toBeNull();
	});

	it('/backlog ohne die Felder der Aufgabenliste (Karten ohne schritte): ein Satz, die Frage bleibt beantwortbar', async () => {
		const alt = backlog();
		const ohneSchritte = {
			...alt,
			spalten: alt.spalten.map((s) => ({
				...s,
				karten: s.karten.map((k) => ohneFelder(k, 'schritte', 'etappe', 'besitzer'))
			}))
		};
		render(ProjektTeile, {
			slug: 'beispiel',
			fetchFn: netzMit({ '/backlog': () => Promise.resolve(json(ohneSchritte)) })
		});
		expect(await screen.findByText(SATZ_TEIL_FEHLT)).toBeInTheDocument();
		const dich = screen.getByRole('region', { name: /Wartet auf dich/ });
		expect(await within(dich).findByRole('button', { name: 'Ja' })).toBeInTheDocument();
		// Die übrigen Teile stehen ganz da.
		await screen.findByText('Gästeliste abgeschlossen');
		await waitFor(() => {
			expect(document.getElementById('etappe-einladung')).not.toBeNull();
		});
		expect(screen.queryByText(SATZ_LISTE_FEHLT)).toBeNull();
	});

	it('Gegenprobe: mit allen Wegen kein Satz über den Dienst', async () => {
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz() });
		await screen.findByText('Gästeliste abgeschlossen');
		await waitFor(() => {
			expect(document.getElementById('etappe-einladung')).not.toBeNull();
		});
		expect(screen.queryByText(SATZ_TEIL_FEHLT)).toBeNull();
	});
});

describe('Fehlerwege, Anker und Neuladen (aus dem Unter-Dashboard übernommen)', () => {
	it('/backlog scheitert (500): die Frage bleibt beantwortbar, ein eigener Satz statt „ohne Anmeldung“', async () => {
		render(ProjektTeile, {
			slug: 'beispiel',
			fetchFn: netz(true, [frage, meldung], () => Promise.resolve(json({ fehler: 'x' }, 500)))
		});
		expect(await screen.findByText(SATZ_LISTE_FEHLT)).toBeInTheDocument();
		const dich = screen.getByRole('region', { name: /Wartet auf dich/ });
		expect(await within(dich).findByRole('button', { name: 'Ja' })).toBeInTheDocument();
		expect(screen.queryByText(SATZ_OHNE_ANMELDUNG)).toBeNull();
		// Ohne Liste haben die Zahlen kein Ziel: Text statt Link.
		await screen.findByText(/In den letzten 7 Tagen/);
		expect(document.querySelectorAll('.vier a')).toHaveLength(0);
	});

	it('„Fertig: …“ steht nicht als Meldung, wenn die Zeitleiste die Arbeit schon als fertig zeigt', async () => {
		const fertig = eintrag({
			art: 'meldung',
			id: 'm-fertig',
			projekt: 'beispiel',
			titel: 'Fertig: Gästeliste abgeschlossen',
			meldung: {
				art: 'fertig',
				anzahl: 1,
				zuletzt: '2026-11-13T09:00:00+01:00',
				text: null,
				system: false,
				schluessel: 'beispiel/fertig:ar-1'
			}
		});
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz(true, [frage, meldung, fertig]) });
		await screen.findByText('Gästeliste abgeschlossen');
		await screen.findByText('Prüfung meldet einen Fehler');
		expect(screen.queryByText('Fertig: Gästeliste abgeschlossen')).toBeNull();
		expect(screen.getByRole('heading', { name: 'Neue Meldungen (1)' })).toBeInTheDocument();
	});

	it('ein Anker auf eine Meldung wartet auf die Zeitleiste; steht ihre Sache dort, landet der Fokus dort', async () => {
		const fertig = eintrag({
			art: 'meldung',
			id: 'm-fertig',
			projekt: 'beispiel',
			titel: 'Fertig: Gästeliste abgeschlossen',
			meldung: {
				art: 'fertig',
				anzahl: 1,
				zuletzt: '2026-11-13T09:00:00+01:00',
				text: null,
				system: false,
				schluessel: 'beispiel/fertig:ar-1'
			}
		});
		const basis = netz(true, [frage, meldung, fertig]);
		let verlaufFrei: () => void = () => undefined;
		const f = vi.fn((eingabe: RequestInfo | URL) =>
			typeof eingabe === 'string' && eingabe.includes('/verlauf?')
				? new Promise<Response>((r) => {
						verlaufFrei = () => {
							r(json(verlauf));
						};
					})
				: basis(eingabe)
		);
		ort.url = new URL('http://localhost:8787/projects/beispiel#meldung-m-fertig');
		render(ProjektTeile, { slug: 'beispiel', fetchFn: f });
		// Vor dem Verlauf steht die Meldung noch eigens da, wird aber nicht angesteuert.
		await screen.findByText('Fertig: Gästeliste abgeschlossen');
		await screen.findByRole('heading', { name: /Läuft/, level: 3 });
		await tick();
		expect(document.activeElement?.id).not.toBe('meldung-m-fertig');
		verlaufFrei();
		await waitFor(() => {
			expect(document.activeElement?.id).toBe('zeitleiste-beispiel');
		});
		expect(document.getElementById('meldung-m-fertig')).toBeNull();
		expect(document.querySelector('.anker-satz')).toBeNull();
		// Gegenprobe: eine Meldung, die es nicht mehr gibt, bekommt den Satz statt der Zeitleiste.
		cleanup();
		ort.url = new URL('http://localhost:8787/projects/beispiel#meldung-m-weg');
		render(ProjektTeile, { slug: 'beispiel', fetchFn: basis });
		await waitFor(() => {
			expect(document.querySelector('.anker-satz')?.textContent).toBe(SATZ_NICHT_MEHR_DA);
		});
		expect(document.activeElement?.id).not.toBe('zeitleiste-beispiel');
	});

	it('Anker ohne Ziel: angemeldet „nicht mehr da“, ohne Anmeldung der Anmelde-Satz, bei kaputter Liste ihr Satz', async () => {
		const satz = async (f: ReturnType<typeof netz>): Promise<string | null> => {
			ort.url = new URL('http://localhost:8787/projects/beispiel#arbeit-ar-9');
			render(ProjektTeile, { slug: 'beispiel', fetchFn: f });
			const el = await waitFor(() => {
				const p = document.querySelector('.anker-satz');
				if (p === null) throw new Error('noch kein Satz');
				return p;
			});
			const text = el.textContent;
			cleanup();
			return text;
		};
		expect(await satz(netz())).toBe(SATZ_NICHT_MEHR_DA);
		expect(await satz(netz(false))).toBe(SATZ_OHNE_ANMELDUNG);
		expect(await satz(netz(true, [frage], () => Promise.resolve(json({ fehler: 'x' }, 500))))).toBe(
			SATZ_LISTE_FEHLT
		);
	});

	it('eine späte Antwort überschreibt keine neuere (Auflage aus A1)', async () => {
		let erste: (r: Response) => void = () => undefined;
		let n = 0;
		const mitTitel = (titel: string): Response => {
			const b = backlog();
			const k = b.spalten[1]?.karten[0];
			if (k) k.titel = titel;
			return json(b);
		};
		const f = netz(true, [frage], () => {
			n += 1;
			if (n === 1)
				return new Promise<Response>((fertig) => {
					erste = fertig;
				});
			return Promise.resolve(mitTitel('Neuer Stand'));
		});
		let hoerer: ((c: ChangeEvent) => void) | null = null;
		vi.spyOn(live, 'onChange').mockImplementation((l) => {
			hoerer = l;
			return () => undefined;
		});
		render(ProjektTeile, { slug: 'beispiel', fetchFn: f });
		(hoerer as ((c: ChangeEvent) => void) | null)?.({
			area: 'anstehend',
			path: 'anstehend/beispiel',
			at: ''
		});
		await screen.findByText('Neuer Stand');
		erste(mitTitel('Alter Stand'));
		await new Promise((r) => setTimeout(r, 0));
		await tick();
		expect(screen.getByText('Neuer Stand')).toBeInTheDocument();
		expect(screen.queryByText('Alter Stand')).toBeNull();
	});

	it('live: anstehend/<slug> lädt alle Teile neu, ein anderes Projekt nicht', async () => {
		let hoerer: ((c: ChangeEvent) => void) | null = null;
		vi.spyOn(live, 'onChange').mockImplementation((l) => {
			hoerer = l;
			return () => undefined;
		});
		const f = netz();
		render(ProjektTeile, { slug: 'beispiel', fetchFn: f });
		await waitFor(() => {
			expect(aufrufe(f, '/verlauf?')).toBe(1);
		});
		const melden = hoerer as ((c: ChangeEvent) => void) | null;
		melden?.({ area: 'anstehend', path: 'anstehend/anderes', at: '' });
		await tick();
		expect(aufrufe(f, '/backlog')).toBe(1);
		melden?.({ area: 'anstehend', path: 'anstehend/beispiel', at: '' });
		await waitFor(() => {
			for (const weg of ['/api/anstehend', '/backlog', '/plan', '/verlauf?'])
				expect(aufrufe(f, weg), weg).toBe(2);
		});
	});

	it('live: eine Änderung am Inhalt des Projekts (plan.toml) lädt Plan und Liste neu, nur für dieses Projekt', async () => {
		let hoerer: ((a: InhaltAenderung) => void) | null = null;
		vi.spyOn(live, 'onInhalt').mockImplementation((l) => {
			hoerer = l;
			return () => undefined;
		});
		const f = netz();
		render(ProjektTeile, { slug: 'beispiel', fetchFn: f });
		await waitFor(() => {
			expect(aufrufe(f, '/plan')).toBe(1);
		});
		const melden = hoerer as ((a: InhaltAenderung) => void) | null;
		const aenderung = (slug: string): InhaltAenderung => ({
			projekte: [slug],
			daten: [],
			version: 2,
			sammel: false
		});
		melden?.(aenderung('anderes'));
		await tick();
		expect(aufrufe(f, '/plan')).toBe(1);
		melden?.(aenderung('beispiel'));
		await waitFor(() => {
			expect(aufrufe(f, '/plan')).toBe(2);
			expect(aufrufe(f, '/backlog')).toBe(2);
		});
		expect(aufrufe(f, '/verlauf?')).toBe(1);
	});

	it('lädt um Mitternacht neu (Frist morgen wird heute, die Zeitleiste rückt weiter)', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		vi.setSystemTime(new Date(2026, 10, 13, 23, 59, 0));
		try {
			const f = netz();
			render(ProjektTeile, { slug: 'beispiel', fetchFn: f });
			await vi.waitFor(() => {
				expect(aufrufe(f, '/verlauf?')).toBe(1);
			});
			await vi.advanceTimersByTimeAsync(30_000);
			expect(aufrufe(f, '/verlauf?')).toBe(1);
			await vi.advanceTimersByTimeAsync(31_000);
			expect(aufrufe(f, '/verlauf?seit=2026-11-01')).toBe(1);
			for (const weg of ['/api/anstehend', '/backlog', '/plan'])
				expect(aufrufe(f, weg), weg).toBe(2);
		} finally {
			vi.useRealTimers();
		}
	});
});
