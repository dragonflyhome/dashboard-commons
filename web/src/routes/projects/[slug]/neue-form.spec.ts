/**
 * Die Seite eines Projekts steht unter /projects/<slug>, die Unterseiten
 * unter /projects/<slug>/<name>. Seit Etappe 6a leiten /neu und /neu/<name>
 * für jedes Projekt dauerhaft dorthin, ohne Vorschau. Geprüft an den
 * Ladefunktionen der drei Routen (der Server hat dieselbe Regel, Test
 * `neu_leitet_dauerhaft_zur_projektseite`).
 */
import { isHttpError, isRedirect } from '@sveltejs/kit';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type * as Client from '#lib/api/client.ts';
import type { ProjectDetail, ProjektAntwort } from '$vertrag';

const project = vi.fn<(slug: string) => Promise<ProjectDetail>>();
const projekt = vi.fn<(slug: string) => Promise<ProjektAntwort>>();
vi.mock('#lib/api/client.ts', async (original) => ({
	...(await original<typeof Client>()),
	api: {
		project: (slug: string) => project(slug),
		projekt: (slug: string) => projekt(slug)
	}
}));
vi.mock('$app/paths', () => ({
	resolve: (id: string, p: { slug: string; name?: string }) =>
		id.replace('[slug]', p.slug).replace('[name]', p.name ?? '')
}));

// redirect() aus SvelteKit prüft im Browser-Code, ob das Ziel auf demselben Ursprung liegt.
vi.stubGlobal('window', { location: new URL('http://localhost:8787/projects/alpha/neu') });

const haupt = await import('./+page.ts');
const neuRahmen = await import('./neu/+layout.ts');
const unterseite = await import('./[name]/+page.ts');

/** Ein Kopf, wie der Server ihn schickt (nur die Felder, die hier zählen). */
function kopf(): ProjektAntwort {
	return {
		slug: 'alpha',
		kopf: { title: 'Architektur', seiten: [{ name: 'zeitachse', title: 'Zeitachse', order: 1 }] },
		daten: []
	} as unknown as ProjektAntwort;
}

function event(params: Record<string, string>): never {
	return { fetch, params, depends: () => undefined } as never;
}

/** Was eine Ladefunktion wirft (oder `null`). */
async function wurf(f: () => unknown): Promise<unknown> {
	try {
		await f();
		return null;
	} catch (e) {
		return e;
	}
}

describe('/projects/<slug>', () => {
	beforeEach(() => {
		project.mockReset();
		projekt.mockReset();
	});

	it('der Kopf aus projekt.toml kommt dazu', async () => {
		project.mockResolvedValue({ slug: 'alpha' } as unknown as ProjectDetail);
		projekt.mockResolvedValue(kopf());
		const d = (await haupt.load(event({ slug: 'alpha' }))) as { kopf: unknown; slug: string };
		expect(d.kopf).toEqual(kopf());
		expect(d.slug).toBe('alpha');
	});
});

describe('/projects/<slug>/neu', () => {
	beforeEach(() => {
		projekt.mockReset();
	});

	it('leitet immer weiter, ohne den Server zu fragen', async () => {
		const w = await wurf(() => neuRahmen.load(event({ slug: 'alpha' })));
		expect(isRedirect(w) && w.status).toBe(307);
		expect(isRedirect(w) && w.location).toBe('/projects/alpha');
		const u = await wurf(() => neuRahmen.load(event({ slug: 'alpha', name: 'zeitachse' })));
		expect(isRedirect(u) && u.location).toBe('/projects/alpha/zeitachse');
		const x = await wurf(() => neuRahmen.load(event({ slug: 'alpha', name: 'fehlt' })));
		expect(isRedirect(x) && x.location).toBe('/projects/alpha/fehlt');
		expect(projekt).not.toHaveBeenCalled();
	});
});

describe('/projects/<slug>/<name>', () => {
	beforeEach(() => {
		projekt.mockReset();
	});

	it('eine Unterseite aus projekt.toml: der Kopf', async () => {
		projekt.mockResolvedValue(kopf());
		const d = await unterseite.load(event({ slug: 'alpha', name: 'zeitachse' }));
		expect(d).toEqual({ kopf: kopf() });
	});

	it('unbekannte Unterseite: 404', async () => {
		projekt.mockResolvedValue(kopf());
		const w = await wurf(() => unterseite.load(event({ slug: 'alpha', name: 'fehlt' })));
		expect(isHttpError(w, 404)).toBe(true);
	});
});
