/**
 * Die Kopfleiste, Fassung 3 (Entscheidung kopfleiste,
 * [fassung_3]): links Brotkrumen, rechts PC-Symbol, Glocke, Live-Punkt und
 * Schloss; das Menü oben ist durch die Seitenleiste ersetzt. Hier steht nur
 * die Zuordnung von Route zu Brotkrumen und zum markierten Eintrag der
 * Seitenleiste, ohne Svelte, damit sie getestet ist.
 *
 * Brotkrumen: Das Symbol ist die Wurzel (Startseite), dann je Ebene
 * ein Glied (etwa Projekte › Architektur des Dashboards), zuletzt das Blatt
 * (die aktuelle Seite). Das Blatt ist ein Knopf: Aktualitätsprüfung und nach
 * oben (`aktualitaet.svelte.ts`). Die Namen kommen aus der Route und den
 * Daten, die die Seite ohnehin geladen hat (`page.data`): auf Projektseiten
 * der Projekttitel, auf Unterseiten deren Titel.
 *
 * Projektgruppen (Entscheidung projektgruppe): Auf den Seiten
 * eines Teils steht der Titel der Gruppe vor dem Projekt („Projekte › Alpha
 * und Beta › Alpha-Vorhaben“); die Gruppen kommen aus der Projektliste des
 * Layouts (`projektListe.gruppen`). Die Gruppenseite `/gruppen/<slug>` hat
 * „Projekte“ als Glied und den Titel der Gruppe als Blatt.
 *
 * Die Seitenleiste (`Seitenleiste.svelte`): oben Anstehend, dann die
 * Projekte nach Schwerpunkt, abgesetzt die Dashboard-Seiten (`SEITEN`). PC
 * steht nicht darin (rechts oben, Symbol mit Verlauf).
 */

import { istSlug } from './gruppen.ts';

/** Ein Eintrag der Seitenleiste außer den Projekten. */
export interface Seite {
	schluessel: string;
	pfad:
		| '/anstehend'
		| '/schluessel'
		| '/hallo'
		| '/vorgaenge'
		| '/daten'
		| '/anleitung'
		| '/zielbild'
		| '/entscheidungen';
	label: string;
}

/** Oben in der Leiste. */
export const ANSTEHEND: Seite = { schluessel: 'anstehend', pfad: '/anstehend', label: 'Anstehend' };

/** Abgesetzt unter den Projekten, in dieser Reihenfolge (Fassung 3). */
export const SEITEN: readonly Seite[] = [
	{ schluessel: 'schluessel', pfad: '/schluessel', label: 'Schlüssel' },
	{ schluessel: 'hallo', pfad: '/hallo', label: 'Hallo' },
	{ schluessel: 'vorgaenge', pfad: '/vorgaenge', label: 'Vorgänge' },
	{ schluessel: 'daten', pfad: '/daten', label: 'Daten' },
	{ schluessel: 'anleitung', pfad: '/anleitung', label: 'Anleitung' },
	{ schluessel: 'zielbild', pfad: '/zielbild', label: 'Zielbild' },
	{ schluessel: 'entscheidungen', pfad: '/entscheidungen', label: 'Entscheidungen' }
];

/** Seiten mit festem Namen (auch die, die nicht in der Leiste stehen). */
const NAMEN: Record<string, string> = {
	anstehend: 'Anstehend',
	pc: 'PC',
	anmelden: 'Anmelden',
	katalog: 'Katalog',
	fassungen: 'Fassungen',
	...Object.fromEntries(SEITEN.map((s) => [s.schluessel, s.label]))
};

/** Anker der Projekt-Übersicht auf der Startseite (Ziel des Glieds „Projekte“). */
export const PROJEKTE_ANKER = 'projekte';

/** Wohin ein mittleres Glied führt; die Adresse baut die Komponente mit `resolve()`. */
export type GliedZiel =
	| { art: 'projekte' }
	| { art: 'projekt'; slug: string }
	| { art: 'gruppe'; slug: string }
	| { art: 'katalog' };

/** Ein mittleres Glied der Brotkrumen. */
export interface Glied {
	text: string;
	ziel: GliedZiel;
}

/** Was in der Seitenleiste markiert ist. */
export interface Markiert {
	/** Eine Seite der Leiste (`anstehend`, `schluessel` …) oder keine. */
	seite: string | null;
	/** Das aktuelle Projekt (Slug) oder keins. */
	projekt: string | null;
	/**
	 * Die Unterseite des Projekts: `null` Übersicht, sonst der Name einer
	 * Unterseite oder `vorschau` (Entwurf).
	 */
	unterseite: string | null;
	/** Projektgruppe: die Gruppenseite `/gruppen/<slug>` ist offen (Slug der Gruppe). */
	gruppe?: string;
}

export interface Kopf {
	/** Die Glieder zwischen Symbol und Blatt (am Handy zu „…“ gekürzt). */
	glieder: Glied[];
	/** Name der aktuellen Seite: das Blatt. */
	blatt: string;
	/** Startseite: das Symbol ist die aktuelle Seite (Ring). */
	zuhause: boolean;
	/** Der ganze Weg als ein Satz (Titel-Hinweis, Tests): „Projekte · Architektur“. */
	titel: string;
	markiert: Markiert;
}

/** Trenner im ganzen Weg, wie im Tab-Titel. */
export const TRENNER = ' · ';

function feld(o: unknown, k: string): unknown {
	return typeof o === 'object' && o !== null ? (o as Record<string, unknown>)[k] : undefined;
}

function text(v: unknown): string | null {
	return typeof v === 'string' && v.trim() !== '' ? v : null;
}

/** Titel einer Unterseite der neuen Form aus `kopf.kopf.seiten`. */
function unterseite(kopf: unknown, name: string): string | null {
	const seiten = feld(kopf, 'seiten');
	if (!Array.isArray(seiten)) return null;
	const s: unknown = seiten.find((x) => feld(x, 'name') === name);
	return text(feld(s, 'title'));
}

function fertig(glieder: Glied[], blatt: string, markiert: Markiert, zuhause = false): Kopf {
	return {
		glieder,
		blatt,
		zuhause,
		titel: [...glieder.map((g) => g.text), blatt].join(TRENNER),
		markiert
	};
}

const KEINE: Markiert = { seite: null, projekt: null, unterseite: null };

/**
 * Brotkrumen und Markierung zur Route.
 *
 * @param routeId `page.route.id` (null, wenn keine Route passt)
 * @param params `page.params`
 * @param data `page.data` (die Daten aller Ebenen, schon geladen)
 */
export function kopfleiste(
	routeId: string | null,
	params: Partial<Record<string, string>>,
	data: unknown
): Kopf {
	if (routeId === null) return fertig([], 'Nicht gefunden', KEINE);
	if (routeId === '/') return fertig([], 'Übersicht', KEINE, true);

	const teile = routeId.split('/');
	const erstes = teile[1] ?? '';

	if (erstes === 'projects') return mitGruppe(projektKopf(routeId, params, data), params, data);

	if (routeId === '/gruppen/[slug]') {
		const slug = params.slug ?? '';
		const titel = text(feld(feld(data, 'gruppe'), 'titel')) ?? slug;
		return fertig([{ text: 'Projekte', ziel: { art: 'projekte' } }], titel, {
			...KEINE,
			gruppe: slug
		});
	}

	// Entscheidung archiv-status: die Archivansicht unter der Projektübersicht,
	// ohne Eintrag in der Seitenleiste.
	if (routeId === '/projekte/archiviert') {
		return fertig([{ text: 'Projekte', ziel: { art: 'projekte' } }], 'Archiv', KEINE);
	}

	if (erstes === 'katalog' && teile.length > 2) {
		const blatt = routeId.endsWith('/logo') ? 'Logo-Entwürfe' : (teile.at(-1) ?? '');
		return fertig([{ text: 'Katalog', ziel: { art: 'katalog' } }], blatt, KEINE);
	}

	const name = NAMEN[erstes];
	if (name !== undefined) {
		const inLeiste = erstes === ANSTEHEND.schluessel || SEITEN.some((s) => s.schluessel === erstes);
		return fertig([], name, { ...KEINE, seite: inLeiste ? erstes : null });
	}
	// Eine neue Route ohne Eintrag hier: ihr Pfad, bis sie einen Namen bekommt.
	return fertig([], erstes, KEINE);
}

/** Die Gruppe eines Projekts aus `projektListe.gruppen` (Layout-Daten), sonst `null`. */
function gruppeAus(data: unknown, slug: string): { slug: string; titel: string } | null {
	const liste = feld(data, 'projektListe');
	const gruppen = feld(liste, 'gruppen');
	if (!Array.isArray(gruppen)) return null;
	const projekte = feld(liste, 'projects');
	const projektSlugs = Array.isArray(projekte)
		? (projekte as unknown[]).map((p) => feld(p, 'slug'))
		: [];
	for (const g of gruppen as unknown[]) {
		const teile = feld(g, 'teile');
		const gslug = text(feld(g, 'slug'));
		// Wie gueltigeGruppen: ein kaputter Slug gäbe eine Adresse, die sich nicht bauen lässt.
		if (gslug === null || !istSlug(gslug) || projektSlugs.includes(gslug)) continue;
		if (Array.isArray(teile) && teile.includes(slug))
			return { slug: gslug, titel: text(feld(g, 'titel')) ?? gslug };
	}
	return null;
}

/** Auf den Seiten eines Teils: das Glied der Gruppe gleich nach „Projekte“. */
function mitGruppe(k: Kopf, params: Partial<Record<string, string>>, data: unknown): Kopf {
	const g = gruppeAus(data, params.slug ?? '');
	if (g === null) return k;
	const [erstes, ...rest] = k.glieder;
	if (erstes === undefined) return k;
	return fertig(
		[erstes, { text: g.titel, ziel: { art: 'gruppe', slug: g.slug } }, ...rest],
		k.blatt,
		k.markiert
	);
}

function projektKopf(
	routeId: string,
	params: Partial<Record<string, string>>,
	data: unknown
): Kopf {
	const slug = params.slug ?? '';
	const projekte: Glied = { text: 'Projekte', ziel: { art: 'projekte' } };
	const markiert = (u: string | null): Markiert => ({ seite: null, projekt: slug, unterseite: u });
	// Etappe 3b-2: Die Unterseiten stehen direkt unter /projects/<slug>/<name>.
	const name = params.name;
	if (routeId === '/projects/[slug]/[name]' && name !== undefined) {
		const kopf = feld(feld(data, 'kopf'), 'kopf');
		const titel = text(feld(kopf, 'title')) ?? slug;
		const projektGlied: Glied = { text: titel, ziel: { art: 'projekt', slug } };
		return fertig([projekte, projektGlied], unterseite(kopf, name) ?? name, markiert(name));
	}
	const titel = text(feld(data, 'title')) ?? slug;
	if (routeId === '/projects/[slug]/vorschau') {
		return fertig(
			[projekte, { text: titel, ziel: { art: 'projekt', slug } }],
			'Vorschau',
			markiert('vorschau')
		);
	}
	return fertig([projekte], titel, markiert(null));
}
