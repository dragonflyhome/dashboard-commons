/**
 * Etappe B8: die vier Teile der Projektseite (`ProjektTeile`) auf der neuen
 * API: Stand (Satz, vier Zahlen als Sprung, Ampel als Wort mit Grund und
 * Link, Bau nur wenn auffällig, Notizen klein), Aufgabenliste (Frage mit
 * Antwort in „Wartet auf dich“), Plan, Zeitleiste (14 Tage, „ältere
 * anzeigen“, ungelesene Meldungen mit „gelesen“), Anker und die Sätze ohne
 * Anmeldung. Jede Regel mit Gegenprobe. Alle Daten erfunden.
 */
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SATZ_OHNE_ANMELDUNG } from '#lib/anstehend.ts';
import { beispielProjektStand, eintrag, json } from '#lib/anstehend.beispiel.ts';
import type { BauLage } from '#lib/bau.svelte.ts';
import type { AnstehendEintrag, ProjektStand, Verlauf, VerlaufEreignis } from '$vertrag';
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

/** fetch für alle Wege; `angemeldet: false` gibt auf den geschützten 401. */
function netz(angemeldet = true, eintraege: AnstehendEintrag[] = [frage, meldung]) {
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
		if (url.endsWith('/backlog')) return Promise.resolve(json(backlog()));
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
		await fireEvent.click(screen.getByRole('button', { name: 'Ältere anzeigen' }));
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

	it('ohne Anmeldung: Stand mit Zahlen, sonst je Teil der Satz, keine Titel', async () => {
		render(ProjektTeile, { slug: 'beispiel', fetchFn: netz(false) });
		await screen.findByRole('link', { name: '1 wartet auf dich' });
		await waitFor(() => {
			expect(screen.getAllByText(SATZ_OHNE_ANMELDUNG)).toHaveLength(3);
		});
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
