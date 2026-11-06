import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { ProjectSummary } from '$vertrag';

vi.mock('$app/paths', () => ({
	resolve: (p: string, params: Record<string, string> = {}) =>
		p.replace(/\[(\w+)\]/g, (_: string, k: string) => params[k] ?? k)
}));
const { default: Seitenleiste } = await import('./Seitenleiste.svelte');
const { leistenGruppen, leistenUnterseiten } = await import('#lib/projekte.ts');

function projekt(
	slug: string,
	title: string,
	schwerpunkt: string,
	reihenfolge: number,
	teil: Partial<ProjectSummary> = {}
): ProjectSummary {
	return {
		slug,
		title,
		status: 'aktiv',
		bedeutung: 'laeuft',
		last_change: '2026-10-30',
		summary: '',
		chat: null,
		backup: null,
		backup_bedeutung: null,
		backup_note: '',
		vorschau: null,
		order: 100,
		open: [],
		repos: [],
		file_modified: null,
		schwerpunkt,
		schwerpunkt_reihenfolge: reihenfolge,
		...teil
	};
}

const LISTE: ProjectSummary[] = [
	projekt('dashboard', 'Dashboard', 'Bienen und Honig', 5, { order: 1, bedeutung: 'problem' }),
	projekt('alpha', 'Architektur des Dashboards', 'Bienen und Honig', 5, { order: 2 }),
	projekt('schaufenster', 'Schaufenster', 'Chor und Verein', 4, {
		status: 'fertig',
		bedeutung: 'ok'
	}),
	projekt('gamma', 'Gamma', 'Haus und Garten', 1, { status: 'pausiert' }),
	projekt('neu-ohne', 'Ohne Schwerpunkt', 'Sonstiges', 6)
];

const KEINE = { seite: null, projekt: null, unterseite: null };

describe('Seitenleiste (Kopfleiste, Fassung 3)', () => {
	it('gruppiert nach Schwerpunkt in dessen Reihenfolge (nicht nach Status), darin wie geplant', () => {
		const g = leistenGruppen(LISTE, { alpha: 2 });
		expect(g.map((x) => x.name)).toEqual([
			'Haus und Garten',
			'Chor und Verein',
			'Bienen und Honig',
			'Sonstiges'
		]);
		expect(g[2]?.projekte.map((p) => [p.slug, p.wartet])).toEqual([
			['dashboard', 0],
			['alpha', 2]
		]);
	});

	it('oben Anstehend, dann die Projekte, abgesetzt die Dashboard-Seiten; PC fehlt', () => {
		render(Seitenleiste, {
			projekte: LISTE,
			markiert: KEINE,
			daten: {},
			jetztZahl: 4,
			antragZahl: 1
		});
		const nav = screen.getByRole('navigation', { name: 'Seiten' });
		const links = within(nav)
			.getAllByRole('link')
			.map((a) => a.textContent.replace(/,.*$/, '').trim());
		expect(links[0]).toMatch(/^Anstehend/);
		expect(links.slice(-7).map((l) => l.replace(/\d+$/, ''))).toEqual([
			'Schlüssel',
			'Hallo',
			'Vorgänge',
			'Daten',
			'Anleitung',
			'Zielbild',
			'Entscheidungen'
		]);
		expect(links.some((l) => /^PC\b/.test(l))).toBe(false);
		expect(within(nav).getByRole('link', { name: /^Anstehend/ })).toHaveAccessibleName(
			'Anstehend, 4 jetzt anstehend'
		);
		expect(within(nav).getByRole('link', { name: /^Schlüssel/ })).toHaveAccessibleName(
			'Schlüssel, 1 offene Passkey-Anträge'
		);
		// Die Gruppen tragen den Namen des Schwerpunkts.
		expect(within(nav).getByRole('region', { name: 'Haus und Garten' })).toBeInTheDocument();
	});

	it('Zähler „wartet“ je Projekt; ohne Anmeldung (leer) keiner', () => {
		const { unmount } = render(Seitenleiste, {
			projekte: LISTE,
			markiert: KEINE,
			daten: {},
			wartet: { schaufenster: 3 }
		});
		expect(screen.getByRole('link', { name: /^Schaufenster/ })).toHaveAccessibleName(
			'Schaufenster, 3 warten auf dich'
		);
		unmount();
		render(Seitenleiste, { projekte: LISTE, markiert: KEINE, daten: {} });
		expect(screen.getByRole('link', { name: /^Schaufenster/ })).toHaveAccessibleName(
			'Schaufenster'
		);
	});

	it('das aktuelle Projekt ist markiert, seine Unterseiten stehen eingerückt darunter', () => {
		const daten = {
			kopf: {
				vorschau: false,
				kopf: {
					title: 'Architektur des Dashboards',
					seiten: [
						{ name: 'notizen', title: 'Notizen', order: 2 },
						{ name: 'zeitachse', title: 'Neue Zeitachse', order: 1 }
					]
				}
			}
		};
		render(Seitenleiste, {
			projekte: LISTE,
			markiert: { seite: null, projekt: 'alpha', unterseite: 'notizen' },
			daten
		});
		const p = screen.getByRole('link', { name: 'Architektur des Dashboards' });
		expect(p).toHaveClass('hier');
		// Eine Unterseite ist offen: das Projekt ist markiert, aber nicht „aktuelle Seite“.
		expect(p).not.toHaveAttribute('aria-current');
		const unter = screen.getByRole('list', { name: 'Seiten von Architektur des Dashboards' });
		expect(
			within(unter)
				.getAllByRole('link')
				.map((a) => a.textContent.trim())
		).toEqual(['Neue Zeitachse', 'Notizen']);
		expect(within(unter).getByRole('link', { name: 'Notizen' })).toHaveAttribute(
			'href',
			'/projects/alpha/notizen'
		);
		expect(within(unter).getByRole('link', { name: 'Notizen' })).toHaveAttribute(
			'aria-current',
			'page'
		);
		// Andere Projekte zeigen keine Unterseiten.
		expect(screen.getAllByRole('list', { name: /^Seiten von/ })).toHaveLength(1);
	});

	it('Unterseiten: Seiten aus projekt.toml und Entwurf', () => {
		expect(
			leistenUnterseiten(
				{ kopf: { kopf: { seiten: [{ name: 'plan', title: 'Plan', order: 1 }] } } },
				{ vorschau: '/projects/schaufenster/assets/a.html' }
			).map((u) => u.titel)
		).toEqual(['Plan', 'Vorschau']);
		expect(
			leistenUnterseiten({ kopf: null }, { vorschau: '/projects/schaufenster/assets/a.html' }).map(
				(u) => u.titel
			)
		).toEqual(['Vorschau']);
		expect(leistenUnterseiten({}, null)).toEqual([]);
	});

	it('ohne Projektliste: ein Satz statt der Projekte, die Seiten bleiben', () => {
		render(Seitenleiste, { projekte: null, markiert: KEINE, daten: {} });
		expect(screen.getByText('Die Projekte lassen sich gerade nicht laden.')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Daten' })).toBeInTheDocument();
	});

	it('Etappe 4c: Marke für den Bau-Stand (rot, im Bau, noch nie gebaut; grün ohne), live über bau', () => {
		const gut = new Date(2026, 9, 31, 14, 5).toISOString();
		const liste = [
			projekt('dashboard', 'Dashboard', 'PC', 1, { bau: { zustand: 'rot', letzte_gute: gut } }),
			projekt('alpha', 'Architektur', 'PC', 1, {
				bau: { zustand: 'gruen', letzte_gute: gut }
			}),
			projekt('schaufenster', 'Schaufenster', 'PC', 1, {
				bau: { zustand: 'noch_nicht', letzte_gute: null }
			}),
			projekt('gamma', 'Gamma', 'PC', 1)
		];
		const { unmount } = render(Seitenleiste, { projekte: liste, markiert: KEINE, daten: {} });
		expect(screen.getByRole('link', { name: /^Dashboard/ })).toHaveAccessibleName(
			'Dashboard, Seite mit Fehler'
		);
		const marke = document.querySelector('.bau-marke[data-bau="rot"]');
		expect(marke?.getAttribute('title')).toMatch(
			/^Seite mit Fehler: zeigt die letzte gute Fassung von /
		);
		expect(screen.getByRole('link', { name: /^Architektur/ })).toHaveAccessibleName('Architektur');
		expect(screen.getByRole('link', { name: /^Schaufenster/ })).toHaveAccessibleName(
			'Schaufenster, Seite noch nicht gebaut'
		);
		expect(screen.getByRole('link', { name: /^Gamma/ })).toHaveAccessibleName('Gamma');
		unmount();
		render(Seitenleiste, {
			projekte: liste,
			markiert: KEINE,
			daten: {},
			bau: {
				alpha: { slug: 'alpha', zustand: 'laeuft', meldung: '', gescheitert: false }
			}
		});
		expect(screen.getByRole('link', { name: /^Architektur/ })).toHaveAccessibleName(
			'Architektur, Seite wird gebaut'
		);
	});
});
