/**
 * Gruppenseite (Entscheidung projektgruppe, [anzeige].tests 2):
 * beide Teile in der Reihenfolge aus `teile`; ein roter Teil (lässt sich
 * nicht laden oder wirft beim Zeichnen) lässt den anderen stehen. Live:
 * eine Änderung an einem Teil lädt nur diesen Abschnitt neu. Dazu die
 * Ladefunktion (unbekannte Gruppe 404). Alle Projekte sind erfunden.
 */
import { isHttpError } from '@sveltejs/kit';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type * as Client from '#lib/api/client.ts';
import type { InhaltAenderung } from '#lib/live.svelte.ts';
import type {
	GruppeAnzeige,
	ProjectDetail,
	ProjectList,
	ProjectSummary,
	ProjektAntwort
} from '$vertrag';

const project = vi.fn<(slug: string) => Promise<ProjectDetail>>();
const projekt = vi.fn<(slug: string) => Promise<ProjektAntwort>>();
vi.mock('#lib/api/client.ts', async (original) => {
	const o = await original<typeof Client>();
	return {
		...o,
		api: {
			...o.api,
			project: (slug: string) => project(slug),
			projekt: (slug: string) => projekt(slug)
		}
	};
});
vi.mock('$app/paths', () => ({
	resolve: (p: string, params: Record<string, string> = {}) =>
		p.replace(/\[(\w+)\]/g, (_: string, k: string) => params[k] ?? k)
}));

const { default: Page } = await import('./+page.svelte');
const { default: GruppenTeil } = await import('#lib/components/projekt/GruppenTeil.svelte');
const { load } = await import('./+page.ts');
const { ApiError } = await import('#lib/api/client.ts');

const GRUPPE: GruppeAnzeige = {
	slug: 'alpha-und-beta',
	titel: 'Alpha und Beta',
	teile: ['beta', 'alpha']
};

function detail(slug: string, title: string, teil: Partial<ProjectDetail> = {}): ProjectDetail {
	return {
		slug,
		title,
		status: 'aktiv',
		bedeutung: 'laeuft',
		last_change: '2026-10-30',
		summary: `Zusammenfassung von ${title}`,
		chat: null,
		backup: null,
		backup_bedeutung: null,
		backup_note: '',
		vorschau: null,
		order: 1,
		open: [],
		repos: [],
		links: [],
		checks: [],
		processes: [],
		seiten: [],
		daten: null,
		...teil
	} as unknown as ProjectDetail;
}

function summary(slug: string, title: string): ProjectSummary {
	return { slug, title, status: 'aktiv', bedeutung: 'laeuft' } as unknown as ProjectSummary;
}

/** Der Kopf aus projekt.toml, wie der Server ihn schickt (nur die Felder, die hier zählen). */
function kopfVon(slug: string, title: string, teil: Partial<ProjektAntwort> = {}): ProjektAntwort {
	return {
		slug,
		kopf: {
			title,
			status: 'aktiv',
			summary: `Zusammenfassung von ${title}`,
			chat: null,
			seiten: [],
			open: [],
			links: []
		},
		daten: [],
		...teil
	} as unknown as ProjektAntwort;
}

/** Der Abschnitt eines Teils. */
function teil(container: HTMLElement, slug: string): HTMLElement {
	const t = container.querySelector<HTMLElement>(`section[data-teil="${slug}"]`);
	if (t === null) throw new Error(`Teil ${slug} fehlt`);
	return t;
}

function zeige() {
	return render(Page, {
		data: {
			gruppe: GRUPPE,
			teile: { alpha: summary('alpha', 'Alpha-Vorhaben'), beta: summary('beta', 'Beta-Vorhaben') }
		},
		params: { slug: GRUPPE.slug }
	} as never);
}

beforeEach(() => {
	project.mockReset();
	projekt.mockReset();
	projekt.mockImplementation((s) =>
		Promise.resolve(kopfVon(s, s === 'alpha' ? 'Alpha-Vorhaben' : 'Beta-Vorhaben'))
	);
	// Unter-Dashboard, Aufgaben und Co. fragen selbst; hier antwortet der Server „ohne Anmeldung“.
	vi.stubGlobal(
		'fetch',
		vi.fn(() => Promise.resolve(new Response('{"fehler":"x"}', { status: 401 })))
	);
});
afterEach(() => {
	vi.unstubAllGlobals();
});

describe('/gruppen/<slug>', () => {
	it('zeigt beide Teile in der Reihenfolge aus teile, mit Kopf und Link „eigene Seite“', async () => {
		project.mockImplementation((s) =>
			Promise.resolve(detail(s, s === 'alpha' ? 'Alpha-Vorhaben' : 'Beta-Vorhaben'))
		);
		const { container } = zeige();
		expect(screen.getByRole('heading', { level: 1, name: 'Alpha und Beta' })).toBeInTheDocument();
		const teile = [...container.querySelectorAll('section[data-teil]')];
		expect(teile.map((t) => t.getAttribute('data-teil'))).toEqual(['beta', 'alpha']);
		await screen.findByText('Zusammenfassung von Beta-Vorhaben');
		await screen.findByText('Zusammenfassung von Alpha-Vorhaben');
		const eigene = screen.getAllByRole('link', { name: 'eigene Seite' });
		expect(eigene.map((a) => a.getAttribute('href'))).toEqual([
			'/projects/beta',
			'/projects/alpha'
		]);
		// Oben: „Wartet auf dich“ mit Sprungmarken zu den Teilen.
		const wartet = screen.getByLabelText('Wartet auf dich');
		expect(
			within(wartet)
				.getAllByRole('link')
				.map((a) => a.getAttribute('href'))
		).toEqual(['#teil-beta', '#teil-alpha', '/anstehend?gruppe=alpha-und-beta']);
		expect(teile.map((t) => t.id)).toEqual(['teil-beta', 'teil-alpha']);
	});

	it('ein Teil lässt sich nicht laden: nur er ist rot, der andere steht', async () => {
		project.mockImplementation((s) =>
			s === 'beta'
				? Promise.reject(new ApiError(422, 'projects/beta/projekt.toml, Zeile 2: kaputt'))
				: Promise.resolve(detail(s, 'Alpha-Vorhaben'))
		);
		const { container } = zeige();
		await screen.findByText('Zusammenfassung von Alpha-Vorhaben');
		const beta = teil(container, 'beta');
		await waitFor(() => {
			expect(within(beta).getByRole('alert')).toHaveTextContent('kaputt');
		});
		const alpha = teil(container, 'alpha');
		expect(within(alpha).queryByRole('alert')).toBeNull();
	});

	it('ein Teil wirft beim Zeichnen: seine Fehlergrenze fängt es, der andere steht', async () => {
		vi.spyOn(console, 'error').mockImplementation(() => undefined);
		project.mockImplementation((s) =>
			Promise.resolve(detail(s, s === 'alpha' ? 'Alpha-Vorhaben' : 'Beta-Vorhaben'))
		);
		projekt.mockImplementation((s) =>
			Promise.resolve(
				s === 'beta'
					? kopfVon(s, 'Beta-Vorhaben', { kopf: null as never })
					: kopfVon(s, 'Alpha-Vorhaben')
			)
		);
		const { container } = zeige();
		await screen.findByText('Zusammenfassung von Alpha-Vorhaben');
		const beta = teil(container, 'beta');
		await waitFor(() => {
			expect(within(beta).getByRole('alert')).toHaveTextContent('beim Zeichnen einen Fehler');
		});
		const alpha = teil(container, 'alpha');
		expect(within(alpha).queryByRole('alert')).toBeNull();
		expect(within(alpha).getByText('Zusammenfassung von Alpha-Vorhaben')).toBeInTheDocument();
	});
});

describe('Fehler in einer Klick-Funktion', () => {
	/** Ein Projekt (Rahmen mit Fehlerfang am Fenster, wenn nicht eingebettet). */
	function neuForm(s: string, titel: string): ProjektAntwort {
		return {
			slug: s,
			kopf: {
				title: titel,
				status: 'aktiv',
				summary: `Satz von ${titel}`,
				chat: null,
				seiten: [],
				open: [],
				links: []
			},
			daten: []
		} as unknown as ProjektAntwort;
	}

	it('ein Fehler aus einer Klick-Funktion in einem Teil lässt den anderen stehen', async () => {
		const konsole = vi.spyOn(console, 'error').mockImplementation(() => undefined);
		project.mockImplementation((s) =>
			Promise.resolve(detail(s, s === 'alpha' ? 'Alpha-Vorhaben' : 'Beta-Vorhaben'))
		);
		projekt.mockImplementation((s) =>
			Promise.resolve(neuForm(s, s === 'alpha' ? 'Alpha-Vorhaben' : 'Beta-Vorhaben'))
		);
		const { container } = zeige();
		await screen.findByText('Satz von Alpha-Vorhaben');
		await screen.findByText('Satz von Beta-Vorhaben');
		// Der Test fängt den Fehler selbst ab (wie die Konsole des Browsers), damit
		// Vitest ihn nicht als unbehandelt meldet; die Teile sehen ihn trotzdem.
		const abfangen = (e: ErrorEvent): void => {
			e.preventDefault();
			e.stopImmediatePropagation();
		};
		window.addEventListener('error', abfangen);
		// So meldet der Browser einen Fehler, den ein Klick in einem Teil wirft.
		window.dispatchEvent(
			new ErrorEvent('error', { error: new Error('Klick im Teil beta kaputt'), cancelable: true })
		);
		await Promise.resolve();
		window.removeEventListener('error', abfangen);
		for (const s of ['alpha', 'beta']) {
			const t = teil(container, s);
			expect(within(t).queryByText('Diese Seite hat gerade einen Fehler')).toBeNull();
		}
		expect(
			within(teil(container, 'alpha')).getByText('Satz von Alpha-Vorhaben')
		).toBeInTheDocument();
		expect(within(teil(container, 'beta')).getByText('Satz von Beta-Vorhaben')).toBeInTheDocument();
		konsole.mockRestore();
	});
});

describe('GruppenTeil: live nur der betroffene Abschnitt', () => {
	it('eine Änderung an alpha lädt nur alpha neu', async () => {
		const hoerer: ((a: InhaltAenderung) => void)[] = [];
		const geladen: string[] = [];
		const lader = {
			laden: (s: string) => {
				geladen.push(s);
				return Promise.resolve({ ...detail(s, s), kopf: kopfVon(s, s) });
			},
			onInhalt: (l: (a: InhaltAenderung) => void) => {
				hoerer.push(l);
				return () => undefined;
			}
		};
		render(GruppenTeil, { slug: 'alpha', projekt: null, anker: 'teil-alpha', lader });
		render(GruppenTeil, { slug: 'beta', projekt: null, anker: 'teil-beta', lader });
		await waitFor(() => {
			expect([...geladen].sort()).toEqual(['alpha', 'beta']);
		});
		for (const h of hoerer) h({ projekte: ['alpha'], daten: [], version: 7, sammel: false });
		await waitFor(() => {
			expect(geladen.filter((s) => s === 'alpha')).toHaveLength(2);
		});
		expect(geladen.filter((s) => s === 'beta')).toHaveLength(1);
	});
});

describe('Ladefunktion', () => {
	const liste = (gruppen: GruppeAnzeige[]): ProjectList => ({
		projects: [summary('alpha', 'Alpha-Vorhaben'), summary('beta', 'Beta-Vorhaben')],
		broken: [],
		gruppen
	});
	const event = (slug: string, projektListe: ProjectList | null): never =>
		({ params: { slug }, parent: () => Promise.resolve({ projektListe }) }) as never;

	it('die Gruppe mit ihren Teilen aus der Projektliste', async () => {
		const d = (await load(event('alpha-und-beta', liste([GRUPPE])))) as {
			gruppe: GruppeAnzeige;
			teile: Record<string, ProjectSummary>;
		};
		expect(d.gruppe).toEqual(GRUPPE);
		expect(Object.keys(d.teile).sort()).toEqual(['alpha', 'beta']);
	});

	it('unbekannte Gruppe oder Eintrag entfernt: 404; ohne Liste 503', async () => {
		for (const [ev, status] of [
			[event('gibt-es-nicht', liste([GRUPPE])), 404],
			[event('alpha-und-beta', liste([])), 404],
			[event('alpha-und-beta', null), 503]
		] as const) {
			try {
				await load(ev);
				expect.unreachable();
			} catch (e) {
				expect(isHttpError(e) && e.status).toBe(status);
			}
		}
	});
});
