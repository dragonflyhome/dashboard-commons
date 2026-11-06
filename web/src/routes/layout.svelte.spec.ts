import { render, screen, within } from '@testing-library/svelte';
import { createRawSnippet, type ComponentProps } from 'svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type * as LiveModul from '#lib/live.svelte.ts';
import type * as AktualitaetModul from '#lib/aktualitaet.svelte.ts';

// Die Kopfleiste im Layout (Fassung 3): Brotkrumen, Seitenleiste und rechts
// PC, Glocke, Live-Punkt, Schloss. Route und Daten kommen aus `$app/state`; die Live-Kanäle
// und Kästen sind Platzhalter, damit nichts ins Netz geht.
interface Ort {
	route: { id: string | null };
	params: Record<string, string>;
	data: unknown;
	url: URL;
}
const { ort } = vi.hoisted(() => {
	const ort: Ort = {
		route: { id: '/' },
		params: {},
		data: {},
		url: new URL('http://localhost:8787/')
	};
	return { ort };
});
vi.mock('$app/state', () => ({ page: ort }));
vi.mock('$app/paths', () => ({
	resolve: (p: string, params: Record<string, string> = {}) =>
		p.replace(/\[(\w+)\]/g, (_: string, k: string) => params[k] ?? k),
	// Etappe 3b-2: das Logo kommt als eine Datei aus static/.
	asset: (p: string) => `/${p}`
}));
const { nav, hoerer } = vi.hoisted(() => ({
	nav: {
		invalidate: vi.fn(),
		refreshAll: vi.fn(() => Promise.resolve()),
		afterNavigate: vi.fn()
	},
	// Live-Lücken: die Hörer des Layouts, damit Tests Ereignisse auslösen können.
	hoerer: {
		vorgang: [] as ((v: unknown) => void)[],
		bereich: [] as ((b: string) => void)[],
		nachholen: [] as (() => void)[],
		bau: [] as ((b: { slug: string; zustand: string; meldung: string }) => void)[]
	}
}));
vi.mock('$app/navigation', () => nav);
vi.mock('#lib/live.svelte.ts', async (original) => {
	const echt = await original<typeof LiveModul>();
	const aus = () => () => undefined;
	const merken =
		<T>(liste: T[]) =>
		(l: T) => {
			liste.push(l);
			return () => undefined;
		};
	return {
		betroffen: () => () => false,
		bereichBetroffen: echt.bereichBetroffen,
		vorgangBetroffen: echt.vorgangBetroffen,
		live: {
			start: vi.fn(),
			stop: vi.fn(),
			onInhalt: aus,
			onVorgang: merken(hoerer.vorgang),
			onBereich: merken(hoerer.bereich),
			onNachholen: merken(hoerer.nachholen),
			onBau: merken(hoerer.bau)
		}
	};
});
vi.mock('#lib/assets.svelte.ts', () => ({ assetStand: { start: () => () => undefined } }));
// Kopfleiste, Fassung 2: der freie Kanal als Platzhalter (angemeldet).
vi.mock('#lib/freikanal.svelte.ts', () => ({
	freierKanal: {
		start: vi.fn(),
		stop: vi.fn(),
		onInhalt: () => () => undefined,
		zustand: 'angemeldet',
		aktiv: 'geschuetzt',
		pc: null
	}
}));
// Kopfleiste, Fassung 3: die Aktualitätsprüfung als Platzhalter (eigene Tests in aktualitaet.spec.ts).
vi.mock('#lib/aktualitaet.svelte.ts', async (original) => {
	const echt = await original<typeof AktualitaetModul>();
	return {
		ABGLEICH_SATZ: echt.ABGLEICH_SATZ,
		aktualitaet: {
			laeuft: false,
			ergebnis: null,
			start: () => () => undefined,
			basisHolen: vi.fn(),
			pruefen: vi.fn((f: () => Promise<unknown>) => f())
		}
	};
});
vi.mock('#lib/aufgaben.svelte.ts', () => ({
	aufgaben: {
		start: () => () => undefined,
		zaehlen: vi.fn(() => Promise.resolve()),
		laden: vi.fn(() => Promise.resolve()),
		benutzt: false
	}
}));
vi.mock('#lib/anstehend.svelte.ts', () => ({
	anstehend: {
		jetzt: 2,
		stand: 'da',
		glocke: { zahl: 1, meldungen: [], hinweise: [] },
		behoben: [],
		wartet: { alpha: 3 },
		beschaeftigt: null,
		start: () => () => undefined,
		laden: vi.fn(() => Promise.resolve()),
		meldungenLaden: vi.fn(() => Promise.resolve())
	}
}));
vi.mock('#lib/passkey.svelte.ts', () => ({
	passkeyAntraege: {
		offen: 0,
		start: () => () => undefined,
		zaehlen: vi.fn(() => Promise.resolve())
	}
}));
// Etappe 4c: der Bau-Stand als Platzhalter (eigene Tests in bau.svelte.spec.ts).
vi.mock('#lib/bau.svelte.ts', () => ({
	bauLage: { live: {}, liste: vi.fn(), start: () => () => undefined }
}));
vi.mock('#lib/components/LiveBadge.svelte', () => ({ default: vi.fn() }));
vi.mock('#lib/components/FassungWache.svelte', () => ({ default: vi.fn() }));

const { default: Layout } = await import('./+layout.svelte');

/** Die Projektliste für die Seitenleiste (Fakt Schwerpunkt vom Server). */
const PROJEKTE = [
	{
		slug: 'alpha',
		title: 'Architektur des Dashboards',
		bedeutung: 'laeuft',
		order: 10,
		schwerpunkt: 'Bienen und Honig',
		schwerpunkt_reihenfolge: 5
	},
	{
		slug: 'schaufenster',
		title: 'Schaufenster',
		bedeutung: 'laeuft',
		order: 5,
		schwerpunkt: 'Chor und Verein',
		schwerpunkt_reihenfolge: 2
	}
];

const inhalt = createRawSnippet(() => ({ render: () => '<p>Inhalt der Seite</p>' }));

function zeige(
	route: string,
	data: unknown = {},
	params: Record<string, string> = {}
): HTMLElement {
	ort.route = { id: route };
	ort.params = params;
	ort.data = data;
	const props = {
		children: inhalt,
		data: { leisteProjekte: PROJEKTE, ...(data as object) },
		params
	};
	render(Layout, props as unknown as ComponentProps<typeof Layout>);
	return screen.getByRole('banner');
}

describe('Kopfleiste im Layout (Fassung 3)', () => {
	it('auf /pc: Brotkrumen Symbol › PC, kein Menü oben, in der Leiste nichts markiert', () => {
		const kopf = zeige('/pc');
		const krumen = within(kopf).getByRole('navigation', { name: 'Brotkrumen' });
		const blatt = within(krumen).getByRole('button', { name: /^PC/ });
		expect(blatt).toHaveAttribute('aria-current', 'page');
		expect(within(krumen).getByRole('link', { name: 'Startseite' })).not.toHaveClass('hier');
		expect(within(kopf).queryByRole('navigation', { name: 'Hauptnavigation' })).toBeNull();
		expect(within(kopf).queryByRole('button', { name: /^Mehr/ })).toBeNull();
		const leiste = screen.getByRole('navigation', { name: 'Seiten' });
		expect(leiste.querySelectorAll('[aria-current]')).toHaveLength(0);
		// PC steht nicht in der Leiste, sondern rechts oben.
		expect(within(leiste).queryByRole('link', { name: /^PC/ })).toBeNull();
		expect(screen.getByText('Inhalt der Seite')).toBeInTheDocument();
	});

	it('rechts in dieser Reihenfolge: PC, Glocke, Live-Punkt, Schloss', () => {
		const kopf = zeige('/');
		const rechts = kopf.querySelector('.rechts');
		if (rechts === null) throw new Error('rechts fehlt');
		const pc = within(rechts as HTMLElement).getByRole('link', { name: /^PC: Prozessor/ });
		const glocke = within(rechts as HTMLElement).getByRole('button', { name: /^Meldungen/ });
		const schloss = within(rechts as HTMLElement).getByRole('link', {
			name: 'Angemeldet, Passkey verwalten'
		});
		expect(pc.compareDocumentPosition(glocke) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		expect(glocke.compareDocumentPosition(schloss) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		expect(pc).toHaveAttribute('href', '/pc');
		expect(schloss).toHaveAttribute('data-schloss', 'offen');
		expect(schloss.getAttribute('href')).toMatch(/^\/anmelden\?ziel=.*#passkey-anlegen$/);
		// Kein Band mehr: die Glocke trägt die Zahl.
		expect(document.querySelector('.band')).toBeNull();
		expect(glocke).toHaveAccessibleName('Meldungen: 1 ungelesen');
	});

	it('Unterseite: Symbol › Projekte › Projekt › Unterseite; in der Leiste Projekt und Unterseite markiert', () => {
		const kopf = zeige(
			'/projects/[slug]/[name]',
			{
				kopf: {
					vorschau: false,
					kopf: {
						title: 'Architektur des Dashboards',
						seiten: [{ name: 'zeitachse', title: 'Neue Zeitachse', order: 1 }]
					}
				}
			},
			{ slug: 'alpha', name: 'zeitachse' }
		);
		const krumen = within(kopf).getByRole('navigation', { name: 'Brotkrumen' });
		expect(within(krumen).getByRole('link', { name: 'Projekte' })).toHaveAttribute(
			'href',
			'/#projekte'
		);
		expect(
			within(krumen).getByRole('link', { name: 'Architektur des Dashboards' })
		).toHaveAttribute('href', '/projects/alpha');
		expect(within(krumen).getByRole('button', { name: /^Neue Zeitachse/ })).toBeInTheDocument();
		const leiste = screen.getByRole('navigation', { name: 'Seiten' });
		const projekt = within(leiste).getByRole('link', { name: /^Architektur des Dashboards/ });
		expect(projekt).toHaveClass('hier');
		expect(projekt).toHaveTextContent('3');
		const unter = within(leiste).getByRole('list', {
			name: 'Seiten von Architektur des Dashboards'
		});
		expect(within(unter).getByRole('link', { name: 'Neue Zeitachse' })).toHaveAttribute(
			'aria-current',
			'page'
		);
	});

	it('auf der Startseite: Blatt Übersicht, Ring am Symbol', () => {
		const kopf = zeige('/');
		const krumen = within(kopf).getByRole('navigation', { name: 'Brotkrumen' });
		expect(within(krumen).getByRole('link', { name: 'Startseite' })).toHaveClass('hier');
		expect(within(krumen).getByRole('button', { name: /^Übersicht/ })).toHaveAttribute(
			'aria-current',
			'page'
		);
	});

	it('Klick auf das Blatt: Aktualitätsprüfung mit refreshAll (früher invalidateAll), ohne Neuladen der Seite', async () => {
		const { aktualitaet } = await import('#lib/aktualitaet.svelte.ts');
		const nach = vi.spyOn(window, 'scrollTo').mockImplementation(() => undefined);
		const kopf = zeige('/daten');
		nav.refreshAll.mockClear();
		within(kopf)
			.getByRole('button', { name: /^Daten/ })
			.click();
		await Promise.resolve();
		expect(vi.mocked(aktualitaet).pruefen.mock.calls).toHaveLength(1);
		expect(nav.refreshAll).toHaveBeenCalledTimes(1);
		expect(nach).toHaveBeenCalledWith(expect.objectContaining({ top: 0 }));
		nach.mockRestore();
	});
});

/** Welche Schlüssel ein `invalidate(fn)` neu lädt (aus einer festen Liste). */
function geladen(aufruf: unknown): string[] {
	const fn = aufruf as (u: URL) => boolean;
	return [
		'api:projects',
		'api:pc',
		'api:pc:benutzer',
		'api:verlauf',
		'api:daten',
		'api:vorgaenge',
		'api:schluessel',
		'api:entscheidungen',
		'api:zielbild'
	].filter((k) => fn(new URL(k)));
}

describe('Live-Lücken im Layout: gezielt statt refreshAll', () => {
	beforeEach(() => {
		// Nur die Hörer des Layouts aus diesem Test.
		for (const liste of Object.values(hoerer)) liste.length = 0;
	});
	it('ein Vorgang lädt nur die Übersicht der Vorgänge, kein refreshAll', () => {
		zeige('/vorgaenge');
		nav.invalidate.mockClear();
		nav.refreshAll.mockClear();
		for (const l of hoerer.vorgang)
			l({ id: 'v', slug: 'x', art: 'abnahme', zustand: 'offen', nr: 1 });
		expect(nav.refreshAll).not.toHaveBeenCalled();
		expect(nav.invalidate).toHaveBeenCalledTimes(1);
		expect(geladen(nav.invalidate.mock.calls[0]?.[0])).toEqual(['api:vorgaenge']);
	});

	it.each([
		['aufgaben', ['api:daten']],
		['windows-ereignisse', ['api:verlauf', 'api:daten']],
		['inventar', ['api:pc:benutzer']],
		['bote', ['api:pc:benutzer', 'api:schluessel']],
		['entscheidungen', ['api:entscheidungen']],
		['zielbild', ['api:zielbild']]
	])('Bereich %s lädt %j', (bereich, schluessel) => {
		zeige('/daten');
		nav.invalidate.mockClear();
		for (const l of hoerer.bereich) l(bereich);
		expect(nav.refreshAll).not.toHaveBeenCalled();
		expect(nav.invalidate).toHaveBeenCalled();
		expect(geladen(nav.invalidate.mock.calls.at(-1)?.[0]).sort()).toEqual([...schluessel].sort());
	});

	it('Etappe 4c: ein fertiger Bau (grün oder rot) lädt nur die Projektliste; „läuft“ lädt nichts', () => {
		zeige('/');
		nav.invalidate.mockClear();
		nav.refreshAll.mockClear();
		for (const l of hoerer.bau) l({ slug: 'alpha', zustand: 'laeuft', meldung: '' });
		expect(nav.invalidate).not.toHaveBeenCalled();
		for (const l of hoerer.bau) l({ slug: 'alpha', zustand: 'rot', meldung: '' });
		expect(nav.refreshAll).not.toHaveBeenCalled();
		expect(nav.invalidate).toHaveBeenCalledTimes(1);
		expect(geladen(nav.invalidate.mock.calls[0]?.[0])).toEqual(['api:projects']);
	});

	it('nachholen lädt weiter alles (refreshAll)', () => {
		zeige('/');
		nav.refreshAll.mockClear();
		for (const l of hoerer.nachholen) l();
		expect(nav.refreshAll).toHaveBeenCalled();
	});
});
