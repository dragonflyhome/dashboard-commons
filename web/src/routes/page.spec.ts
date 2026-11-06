import { isHttpError } from '@sveltejs/kit';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AnmeldungNoetig, ApiError, SATZ_KEINE_VERBINDUNG } from '#lib/api/client.ts';
import { zaehleProjekte, zaehlzeile } from '#lib/projekte.ts';
import type * as Client from '#lib/api/client.ts';
import { rueckfall } from '#lib/rueckfall.ts';
import type { Bedeutung, PcBenutzer, PcReport, ProjectList, RepoStatus, Verlauf } from '$vertrag';

const projects = vi.fn<() => Promise<ProjectList>>();
const pc = vi.fn<() => Promise<PcReport>>();
const pcBenutzer = vi.fn<() => Promise<PcBenutzer>>();
const verlauf = vi.fn<(seit?: string) => Promise<Verlauf>>();
const verlaufVoll = vi.fn<(seit?: string) => Promise<Verlauf>>();
vi.mock('#lib/api/client.ts', async (original) => ({
	...(await original<typeof Client>()),
	api: {
		projects: () => projects(),
		pc: () => pc(),
		pcBenutzer: () => pcBenutzer(),
		verlauf: (seit?: string) => verlauf(seit),
		verlaufVoll: (seit?: string) => verlaufVoll(seit)
	}
}));

const { load } = await import('./+page.ts');

const liste = { projects: [] } as unknown as ProjectList;
const bericht = { snapshot: { host: 'PC' } } as unknown as PcReport;
const benutzer = {
	schluessel: { vollstaendig: 'ja', gespeichert: 1, fehlen: 0, grund: null },
	bote: 'da'
} as PcBenutzer;
const pcVerlauf: Verlauf = { ereignisse: [], seit: '', bis: '', hinweise: [] };

/** `projektListe`: was das Layout geholt hat (`null`: dort ging es nicht). */
async function lade(projektListe: ProjectList | null = null): Promise<unknown> {
	const event = {
		fetch,
		depends: () => undefined,
		parent: () => Promise.resolve({ leisteProjekte: projektListe?.projects ?? null, projektListe })
	} as unknown as Parameters<typeof load>[0];
	return load(event);
}

describe('Laden der Startseite', () => {
	beforeEach(() => {
		rueckfall.vergessen();
		projects.mockReset().mockResolvedValue(liste);
		pc.mockReset().mockResolvedValue(bericht);
		pcBenutzer.mockReset();
		verlauf.mockReset().mockResolvedValue(pcVerlauf);
		verlaufVoll.mockReset().mockResolvedValue(pcVerlauf);
	});
	it('gibt dem Cockpit die Benutzer-Daten mit, dazu den Verlauf der letzten Tage', async () => {
		pcBenutzer.mockResolvedValue(benutzer);
		expect(await lade()).toEqual({
			projects: [],
			pc: bericht,
			ladung: { benutzer, grund: null },
			verlauf: { verlauf: pcVerlauf, fehler: null, ohneAnmeldung: false }
		});
		// Hotfix 3a-1: der geschützte Weg, damit Edge die Anmeldung mitschickt.
		expect(verlaufVoll.mock.calls[0]?.[0]).toMatch(/^\d{4}-\d{2}-\d{2}$/);
		expect(verlauf).not.toHaveBeenCalled();
	});
	it('/api/projects nur einmal: die Liste aus dem Layout, sonst selbst geholt', async () => {
		pcBenutzer.mockResolvedValue(benutzer);
		const ausLayout = { projects: [], broken: [{ slug: 'kaputt' }] } as unknown as ProjectList;
		expect(await lade(ausLayout)).toMatchObject({ broken: [{ slug: 'kaputt' }] });
		expect(projects).not.toHaveBeenCalled();
		// Ging sie im Layout nicht: die Seite holt sie selbst.
		expect(await lade(null)).toMatchObject({ projects: [] });
		expect(projects).toHaveBeenCalledTimes(1);
	});
	it('Verlauf ohne Anmeldung (401 oder 403): der freie Weg, mit Hinweis', async () => {
		pcBenutzer.mockResolvedValue(benutzer);
		for (const fehler of [
			new AnmeldungNoetig(),
			new ApiError(401, 'Bitte mit Passkey anmelden.', 'passkey'),
			new ApiError(403, 'Dieses Windows-Konto darf hier nicht.', 'fremdes-konto')
		]) {
			rueckfall.vergessen();
			verlaufVoll.mockReset().mockRejectedValue(fehler);
			verlauf.mockClear();
			expect(await lade()).toMatchObject({
				verlauf: { verlauf: pcVerlauf, fehler: null, ohneAnmeldung: true }
			});
			expect(verlauf.mock.calls[0]?.[0]).toBe(verlaufVoll.mock.calls[0]?.[0]);
		}
	});
	it('ohne Windows-Anmeldung (401): Ladung ohne Daten, keine Fehlerseite', async () => {
		pcBenutzer.mockRejectedValue(new AnmeldungNoetig());
		expect(await lade()).toMatchObject({ ladung: { benutzer: null, grund: 'anmeldung' } });
	});
	it('Verlauf nicht lesbar: Satz statt Fehlerseite', async () => {
		pcBenutzer.mockResolvedValue(benutzer);
		verlaufVoll.mockRejectedValue(new ApiError(500, 'Speicher zu.'));
		expect(await lade()).toMatchObject({ verlauf: { verlauf: null, fehler: 'Speicher zu.' } });
		expect(verlauf).not.toHaveBeenCalled();
		// Auch der freie Weg scheitert: Satz statt Fehlerseite.
		verlaufVoll.mockRejectedValue(new AnmeldungNoetig());
		verlauf.mockRejectedValue(new ApiError(503, 'Zu.'));
		expect(await lade()).toMatchObject({
			verlauf: { verlauf: null, fehler: 'Zu.', ohneAnmeldung: true }
		});
	});
	// Sonst fragt SvelteKit updated.check() und lädt bei neuer Fassung voll neu, auch über offene Eingaben.
	it('Fehler beim Laden werden HttpError, mit Satz und art; Netzfehler 503', async () => {
		pcBenutzer.mockResolvedValue(benutzer);
		pc.mockRejectedValue(new ApiError(403, 'Nicht von hier.', 'fremdes-konto'));
		const e = await lade().catch((x: unknown) => x);
		expect(isHttpError(e, 403)).toBe(true);
		expect(e).toMatchObject({ body: { message: 'Nicht von hier.', art: 'fremdes-konto' } });
		pc.mockRejectedValue(new TypeError('Failed to fetch'));
		const n = await lade().catch((x: unknown) => x);
		expect(isHttpError(n, 503)).toBe(true);
		expect(n).toMatchObject({ body: { message: SATZ_KEINE_VERBINDUNG } });
		// Ein Programmfehler bleibt, wie er ist.
		pc.mockRejectedValue(new RangeError('kaputt'));
		await expect(lade()).rejects.toThrow(RangeError);
	});
});

/** Ein Projekt für die Zählzeile; nur Bedeutung und Repos zählen. */
function projekt(bedeutung: Bedeutung, dirty = 0): { bedeutung: Bedeutung; repos: RepoStatus[] } {
	return { bedeutung, repos: [{ dirty_files: dirty } as RepoStatus] };
}
const mal = (n: number, b: Bedeutung) => Array.from({ length: n }, () => projekt(b));

describe('Zählzeile der Startseite', () => {
	it('zählt alle Projekte, fertige als eigene Gruppe', () => {
		// 19 Projekte wie im Prüfbericht: früher fehlten die 3 fertigen.
		const liste = [...mal(14, 'laeuft'), ...mal(3, 'ok'), ...mal(2, 'wartet')];
		const z = zaehleProjekte(liste);
		expect(z.gesamt).toBe(19);
		expect(z.gruppen.reduce((s, g) => s + g.anzahl, 0)).toBe(19);
		expect(zaehlzeile(z)).toBe('19 Projekte: 14 aktiv, 3 fertig, 2 pausiert');
	});
	it('jede Bedeutung hat eine Gruppe, die Summe stimmt immer', () => {
		const alle: Bedeutung[] = ['ok', 'laeuft', 'wartet', 'problem', 'info', 'neutral'];
		const liste = alle.flatMap((b, i) => mal(i + 1, b));
		const z = zaehleProjekte(liste);
		expect(z.gruppen.reduce((s, g) => s + g.anzahl, 0)).toBe(liste.length);
		expect(zaehlzeile(z)).toBe(
			'21 Projekte: 2 aktiv, 5 in Arbeit, 1 fertig, 3 pausiert, 6 geplant, 4 mit Problem'
		);
	});
	it('nennt ungesicherte Dateien quer dazu, ohne die Summe zu ändern', () => {
		const z = zaehleProjekte([projekt('laeuft', 2), projekt('ok')]);
		expect(zaehlzeile(z)).toBe('2 Projekte: 1 aktiv, 1 fertig · 1 mit ungesicherten Dateien');
	});
	it('Einzahl und leere Liste', () => {
		expect(zaehlzeile(zaehleProjekte([projekt('ok')]))).toBe('1 Projekt: 1 fertig');
		expect(zaehlzeile(zaehleProjekte([]))).toBe('0 Projekte');
	});
});
