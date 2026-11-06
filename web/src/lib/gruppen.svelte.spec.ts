/**
 * Projektgruppen (Entscheidung projektgruppe, [anzeige].tests):
 * Seitenleiste, Startseite, Brotkrumen, „Anstehend“ mit Gruppentitel, der
 * Zähler „Projekte mit etwas für dich“ zählt die Gruppe einmal; ohne
 * Gruppen (Eintrag aus gruppen.toml entfernt) alles wie vorher. Alle
 * Projekte und Gruppen sind erfunden.
 */
import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { AnstehendEintrag, GruppeAnzeige, ProjectSummary } from '$vertrag';

vi.mock('$app/paths', () => ({
	resolve: (p: string, params: Record<string, string> = {}) =>
		p.replace(/\[(\w+)\]/g, (_: string, k: string) => params[k] ?? k)
}));
const { default: Seitenleiste } = await import('./components/kopf/Seitenleiste.svelte');
const { default: JetztFuerDich } = await import('./components/anstehend/JetztFuerDich.svelte');
const { auswerten, leistenGruppen } = await import('./projekte.ts');
const { kachelnAus, mitGruppe, dringendster, wartetSumme } = await import('./gruppen.ts');
const { kopfleiste } = await import('./kopfleiste.ts');
const {
	filterAus,
	gefiltert,
	glocke,
	jetztFuerDich,
	jetztKarten,
	meldungsOrt,
	nachSchwerpunkt,
	sucheAus
} = await import('./anstehend.ts');
const { eintrag } = await import('./anstehend.beispiel.ts');

function projekt(slug: string, title: string, teil: Partial<ProjectSummary> = {}): ProjectSummary {
	return {
		slug,
		title,
		status: 'aktiv',
		bedeutung: 'laeuft',
		last_change: '2026-10-30',
		summary: `Über ${title}`,
		chat: null,
		backup: null,
		backup_bedeutung: null,
		backup_note: '',
		vorschau: null,
		order: 100,
		open: [],
		repos: [],
		file_modified: null,
		schwerpunkt: 'Arbeit',
		schwerpunkt_reihenfolge: 1,
		...teil
	};
}

// delta (order 1) vor alpha (2) vor gamma (3) vor beta (4); die Gruppe nennt alpha zuerst.
const LISTE: ProjectSummary[] = [
	projekt('delta', 'Delta', { order: 1 }),
	projekt('alpha', 'Alpha-Vorhaben', { order: 2 }),
	projekt('gamma', 'Gamma', { order: 3, schwerpunkt: 'Haus', schwerpunkt_reihenfolge: 2 }),
	projekt('beta', 'Beta-Vorhaben', {
		order: 4,
		status: 'pausiert',
		bedeutung: 'wartet',
		schwerpunkt: 'Haus',
		schwerpunkt_reihenfolge: 2
	})
];
const GRUPPEN: GruppeAnzeige[] = [
	{ slug: 'alpha-und-beta', titel: 'Alpha und Beta', teile: ['alpha', 'beta'] }
];
const ALLE = { suche: '', status: 'alle', sortierung: 'order' as const };

describe('Startseite: eine Karte für die Gruppe', () => {
	it('an der Stelle des ersten Teils, Status = der dringendere Teil, je Teil eine Zeile', () => {
		const a = auswerten(LISTE, ALLE, {}, GRUPPEN);
		expect(a.kacheln.map((k) => k.schluessel)).toEqual(['delta', 'gruppe:alpha-und-beta', 'gamma']);
		const g = a.kacheln[1];
		if (g?.art !== 'gruppe') throw new Error('keine Gruppenkarte');
		expect(g.teile.map((t) => t.slug)).toEqual(['alpha', 'beta']);
		// pausiert (wartet) ist dringender als aktiv (läuft).
		expect([g.bedeutung, g.status]).toEqual(['wartet', 'pausiert']);
		// Spalten: die Gruppe als eine Karte in der Spalte des dringenderen Teils.
		expect(a.spalten.map((s) => [s.bedeutung, s.karten.map((k) => k.schluessel)])).toEqual([
			['laeuft', ['delta', 'gamma']],
			['wartet', ['gruppe:alpha-und-beta']]
		]);
		// Die Zählzeile zählt wie die Karten: die Gruppe einmal, mit dem dringenderen Teil.
		expect(a.zeile).toBe('3 Projekte: 2 aktiv, 1 pausiert');
	});

	it('ist der erste Teil weggefiltert, steht die Karte beim ersten gezeigten Teil', () => {
		const gezeigt = LISTE.filter((p) => p.slug !== 'alpha');
		expect(kachelnAus(gezeigt, LISTE, GRUPPEN).map((k) => k.schluessel)).toEqual([
			'delta',
			'gamma',
			'gruppe:alpha-und-beta'
		]);
	});

	it('der dringendere Teil: Problem vor wartet vor läuft vor fertig', () => {
		const b = (bedeutung: ProjectSummary['bedeutung']) => ({ bedeutung });
		expect(dringendster([b('ok'), b('laeuft'), b('problem'), b('wartet')])?.bedeutung).toBe(
			'problem'
		);
		expect(dringendster([b('ok'), b('neutral')])?.bedeutung).toBe('neutral');
		expect(dringendster([])).toBeNull();
	});

	it('ohne Gruppen (Eintrag entfernt): je Projekt eine Karte, alles wie vorher', () => {
		const ohne = auswerten(LISTE, ALLE, {});
		expect(ohne.kacheln.map((k) => k.schluessel)).toEqual(ohne.gezeigt.map((p) => p.slug));
		expect(auswerten(LISTE, ALLE, {}, []).spalten).toEqual(ohne.spalten);
		expect(ohne.spalten.map((s) => s.projekte.map((p) => p.slug))).toEqual([
			['delta', 'alpha', 'gamma'],
			['beta']
		]);
	});
});

describe('Seitenleiste: ein Eintrag für die Gruppe, die Teile eingerückt', () => {
	it('an der Stelle des ersten Teils, wartet = Summe; ohne Gruppen wie vorher', () => {
		const g = leistenGruppen(LISTE, { alpha: 1, beta: 2 }, {}, GRUPPEN);
		expect(g.map((x) => [x.name, x.projekte.map((p) => p.slug)])).toEqual([
			['Arbeit', ['delta', 'alpha-und-beta']],
			['Haus', ['gamma']]
		]);
		const gruppe = g[0]?.projekte[1];
		expect(gruppe?.titel).toBe('Alpha und Beta');
		expect(gruppe?.wartet).toBe(3);
		expect(gruppe?.bedeutung).toBe('wartet');
		expect(gruppe?.teile?.map((t) => [t.slug, t.wartet])).toEqual([
			['alpha', 1],
			['beta', 2]
		]);
		const ohne = leistenGruppen(LISTE, { alpha: 1 });
		expect(leistenGruppen(LISTE, { alpha: 1 }, {}, [])).toEqual(ohne);
		expect(ohne.flatMap((x) => x.projekte).every((p) => p.teile === undefined)).toBe(true);
	});

	it('zeigt den Gruppeneintrag mit Link zur Gruppenseite und die Teile darunter', () => {
		render(Seitenleiste, {
			projekte: LISTE,
			gruppen: GRUPPEN,
			markiert: { seite: null, projekt: 'beta', unterseite: null },
			daten: null
		});
		const eintrag = screen.getByRole('link', { name: /^Alpha und Beta/ });
		expect(eintrag.getAttribute('href')).toBe('/gruppen/alpha-und-beta');
		expect(eintrag.classList.contains('hier')).toBe(true);
		const teile = screen.getByRole('list', { name: 'Teile von Alpha und Beta' });
		expect(
			within(teile)
				.getAllByRole('link')
				.map((a) => a.getAttribute('href'))
		).toEqual(['/projects/alpha', '/projects/beta']);
		expect(within(teile).getByRole('link', { name: /Beta-Vorhaben/ })).toHaveAttribute(
			'aria-current',
			'page'
		);
	});
});

describe('schlechte Gruppen-Slugs zur Laufzeit (Seitenleiste ohne Fehlergrenze)', () => {
	// Der Server lässt sie weg; kommen sie doch an, bleibt die Oberfläche stehen.
	const SCHLECHT: GruppeAnzeige[] = [
		{ slug: '', titel: 'Leer', teile: ['delta'] },
		{ slug: 'x/', titel: 'Schräg', teile: ['delta'] },
		{ slug: 'delta', titel: 'Wie ein Projekt', teile: ['delta', 'gamma'] },
		...GRUPPEN
	];

	it('Seitenleiste, Startseite und Brotkrumen lassen sie weg, die gültige Gruppe bleibt', () => {
		const g = leistenGruppen(LISTE, {}, {}, SCHLECHT);
		expect(g.map((x) => [x.name, x.projekte.map((p) => [p.slug, p.teile !== undefined])])).toEqual([
			[
				'Arbeit',
				[
					['delta', false],
					['alpha-und-beta', true]
				]
			],
			['Haus', [['gamma', false]]]
		]);
		expect(auswerten(LISTE, ALLE, {}, SCHLECHT).kacheln.map((k) => k.schluessel)).toEqual([
			'delta',
			'gruppe:alpha-und-beta',
			'gamma'
		]);
		const data = { projektListe: { projects: LISTE, broken: [], gruppen: SCHLECHT } };
		expect(kopfleiste('/projects/[slug]', { slug: 'delta' }, data).titel).toBe('Projekte · delta');
	});

	it('die Seitenleiste zeichnet ohne Fehler, ohne doppelte Schlüssel', () => {
		const fehler = vi.spyOn(console, 'error').mockImplementation(() => undefined);
		const warnung = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
		render(Seitenleiste, {
			projekte: LISTE,
			gruppen: SCHLECHT,
			markiert: { seite: null, projekt: 'delta', unterseite: null },
			daten: null
		});
		const hrefs = screen.getAllByRole('link').map((a) => a.getAttribute('href'));
		expect(hrefs).toContain('/projects/delta');
		expect(hrefs).toContain('/gruppen/alpha-und-beta');
		expect(hrefs.filter((h) => h?.startsWith('/gruppen/'))).toEqual(['/gruppen/alpha-und-beta']);
		expect(fehler).not.toHaveBeenCalled();
		expect(warnung).not.toHaveBeenCalled();
		fehler.mockRestore();
		warnung.mockRestore();
	});

	it('gleicher Slug für Gruppe und Projekt: verschiedene Schlüssel', async () => {
		const { gruppenSchluessel } = await import('./gruppen.ts');
		expect(gruppenSchluessel('delta')).not.toBe('delta');
	});
});

describe('Brotkrumen mit Gruppentitel', () => {
	const data = {
		projektListe: { projects: LISTE, broken: [], gruppen: GRUPPEN },
		title: 'Alpha-Vorhaben'
	};

	it('auf der Seite eines Teils: Projekte › Gruppe › Projekt', () => {
		const k = kopfleiste('/projects/[slug]', { slug: 'alpha' }, data);
		expect(k.titel).toBe('Projekte · Alpha und Beta · Alpha-Vorhaben');
		expect(k.glieder[1]?.ziel).toEqual({ art: 'gruppe', slug: 'alpha-und-beta' });
		expect(k.markiert).toEqual({ seite: null, projekt: 'alpha', unterseite: null });
	});

	it('auf der Gruppenseite: Projekte › Gruppe, markiert ist die Gruppe', () => {
		const k = kopfleiste(
			'/gruppen/[slug]',
			{ slug: 'alpha-und-beta' },
			{
				...data,
				gruppe: GRUPPEN[0]
			}
		);
		expect(k.titel).toBe('Projekte · Alpha und Beta');
		expect(k.markiert.gruppe).toBe('alpha-und-beta');
	});

	it('ohne Gruppe (Eintrag entfernt): wie vorher', () => {
		const ohne = { ...data, projektListe: { ...data.projektListe, gruppen: [] } };
		expect(kopfleiste('/projects/[slug]', { slug: 'alpha' }, ohne).titel).toBe(
			'Projekte · Alpha-Vorhaben'
		);
	});
});

describe('Anstehend, Glocke und „Jetzt für dich“ mit Gruppentitel', () => {
	const G = { slug: 'alpha-und-beta', titel: 'Alpha und Beta' };
	function liste(mitGruppen: boolean): AnstehendEintrag[] {
		const g = mitGruppen ? { gruppe: G } : {};
		return [
			eintrag({
				art: 'frage',
				id: 'f-a',
				projekt: 'alpha',
				projekt_titel: 'Alpha-Vorhaben',
				stufe: 'jetzt',
				...g
			}),
			eintrag({
				art: 'frage',
				id: 'f-b',
				projekt: 'beta',
				projekt_titel: 'Beta-Vorhaben',
				stufe: 'jetzt',
				...g
			}),
			eintrag({
				art: 'frage',
				id: 'f-d',
				projekt: 'delta',
				projekt_titel: 'Delta',
				stufe: 'jetzt'
			}),
			eintrag({
				art: 'meldung',
				id: 'm-1',
				projekt: 'alpha',
				projekt_titel: 'Alpha-Vorhaben',
				stufe: 'jetzt',
				meldung: {
					art: 'info',
					anzahl: 1,
					zuletzt: '2026-11-01T08:00:00+01:00',
					einzelheiten: null,
					schluessel: null,
					system: false
				} as never,
				...g
			})
		];
	}

	it('Einträge bleiben beim Projekt, angezeigt mit dem Gruppentitel davor', () => {
		const l = liste(true);
		expect(mitGruppe('Alpha-Vorhaben', G)).toBe('Alpha und Beta › Alpha-Vorhaben');
		expect(l.map((e) => e.projekt)).toEqual(['alpha', 'beta', 'delta', 'alpha']);
		const titel = nachSchwerpunkt(l).flatMap((s) => s.projekte.map((p) => p.titel));
		expect(titel).toEqual([
			'Alpha und Beta › Alpha-Vorhaben',
			'Alpha und Beta › Beta-Vorhaben',
			'Delta'
		]);
		// Glocke: unverändert je Eintrag, mit dem Gruppentitel davor.
		const gl = glocke(l);
		expect(gl.zahl).toBe(1);
		expect(gl.meldungen.map(meldungsOrt)).toEqual(['Alpha und Beta › Alpha-Vorhaben']);
	});

	it('Filter nach Gruppe zeigt beide Teile und steht in der Adresse', () => {
		const f = filterAus(new URLSearchParams('gruppe=alpha-und-beta'));
		expect(f.gruppe).toBe('alpha-und-beta');
		expect(sucheAus(f)).toBe('?gruppe=alpha-und-beta');
		expect(gefiltert(liste(true), f).map((e) => e.id)).toEqual(['f-a', 'f-b', 'm-1']);
		// Ohne ?gruppe= gibt es das Feld nicht (wie vorher).
		expect(filterAus(new URLSearchParams(''))).toEqual({
			ansicht: 'prioritaet',
			arten: [],
			nur: false
		});
	});

	it('Zähler „Projekte mit etwas für dich“: eine Gruppe zählt einmal, Einträge einzeln', () => {
		const karten = jetztKarten(liste(true));
		expect(karten.map((k) => [k.titel, k.teile.map((t) => t.projekt)])).toEqual([
			['Alpha und Beta', ['alpha', 'beta']],
			['Delta', ['delta']]
		]);
		// Ohne Gruppen: je Projekt eine Karte, wie jetztFuerDich.
		const ohne = jetztKarten(liste(false));
		expect(ohne.map((k) => k.schluessel)).toEqual(
			jetztFuerDich(liste(false)).map((p) => p.projekt)
		);
	});

	it('„Jetzt für dich“ nennt die Gruppe einmal und darunter die Teile', () => {
		render(JetztFuerDich, { liste: { eintraege: liste(true), archiv_fristen: [] }, hoechstens: 1 });
		expect(screen.getByRole('heading', { level: 3, name: 'Alpha und Beta' })).toBeInTheDocument();
		expect(screen.getAllByRole('heading', { level: 4 }).map((h) => h.textContent)).toEqual([
			'Alpha-Vorhaben',
			'Beta-Vorhaben'
		]);
		// Eine Gruppe und ein Projekt: bei höchstens einem steht „ein weiteres Projekt“.
		expect(screen.getByRole('link', { name: 'und ein weiteres Projekt' })).toBeInTheDocument();
	});

	it('wartet auf dich: Summe der Teile', () => {
		expect(wartetSumme(['alpha', 'beta'], { alpha: 2, beta: 1, delta: 5 })).toBe(3);
	});
});
