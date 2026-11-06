/**
 * Projektgruppen (Entscheidung projektgruppe): Projekte, die als
 * eine Karte und eine Seite erscheinen. Die Gruppen kommen vom Server
 * (`/api/projects`, Feld `gruppen`, aus `data/gruppen.toml`), schon ohne
 * archivierte Teile und in der Reihenfolge der Seite; an jedem Eintrag in
 * „Anstehend“ steht die Gruppe als `gruppe`. Hier stehen nur die kleinen
 * Regeln, die Startseite, Seitenleiste, Brotkrumen, „Anstehend“, Glocke und
 * Gruppenseite gemeinsam brauchen. Ohne Gruppen ändert keine von ihnen etwas.
 */
import type { Bedeutung, GruppeAnzeige, ProjectSummary } from '$vertrag';

/** Trenner zwischen Gruppe und Projekt: „Alpha und Beta › Alpha-Vorhaben“. */
export const GRUPPEN_TRENNER = ' › ';

/** Der Name eines Projekts mit dem Gruppentitel davor (ohne Gruppe nur der Name). */
export function mitGruppe(name: string, gruppe: { titel: string } | null | undefined): string {
	return gruppe ? `${gruppe.titel}${GRUPPEN_TRENNER}${name}` : name;
}

/** Ein Slug, wie der Server ihn für Projekte und Gruppen erlaubt (`seite::is_valid_slug`). */
export function istSlug(slug: string): boolean {
	return /^[a-z0-9][a-z0-9-]{0,63}$/.test(slug);
}

/**
 * Nur Gruppen, deren Slug geht und kein Projekt-Slug ist. Der Server lässt
 * solche schon weg; die Oberfläche prüft noch einmal, damit eine kaputte
 * Antwort weder doppelte Schlüssel in der Seitenleiste noch eine Adresse
 * ergibt, die sich nicht bauen lässt (die Leiste steht ohne Fehlergrenze im
 * Layout).
 */
export function gueltigeGruppen(
	gruppen: readonly GruppeAnzeige[] | null | undefined,
	projekte: readonly Pick<ProjectSummary, 'slug'>[]
): GruppeAnzeige[] {
	const slugs = new Set(projekte.map((p) => p.slug));
	return (gruppen ?? []).filter((g) => istSlug(g.slug) && !slugs.has(g.slug));
}

/** Schlüssel einer Gruppe in Listen, nie gleich einem Projekt-Slug. */
export function gruppenSchluessel(slug: string): string {
	return `gruppe:${slug}`;
}

/** Die Gruppe eines Projekts, sonst `null`. */
export function gruppeVon(
	gruppen: readonly GruppeAnzeige[] | null | undefined,
	slug: string
): GruppeAnzeige | null {
	return gruppen?.find((g) => g.teile.includes(slug)) ?? null;
}

/**
 * Wie dringend eine Bedeutung für die Karte einer Gruppe ist, vorn am
 * dringendsten: ein Problem, dann was wartet (pausiert), was läuft, ein
 * Hinweis, geplant, zuletzt fertig.
 */
export const DRINGLICHKEIT: readonly Bedeutung[] = [
	'problem',
	'wartet',
	'laeuft',
	'info',
	'neutral',
	'ok'
];

/** Der dringendere der Teile (für Punkt und Status der Gruppenkarte). */
export function dringendster<T extends { bedeutung: Bedeutung }>(teile: readonly T[]): T | null {
	let best: T | null = null;
	for (const t of teile) {
		if (best === null || DRINGLICHKEIT.indexOf(t.bedeutung) < DRINGLICHKEIT.indexOf(best.bedeutung))
			best = t;
	}
	return best;
}

/** Eine Karte der Startseite: ein Projekt oder eine Gruppe mit ihren Teilen. */
export type Kachel =
	| { art: 'projekt'; schluessel: string; projekt: ProjectSummary; bedeutung: Bedeutung }
	| {
			art: 'gruppe';
			schluessel: string;
			gruppe: GruppeAnzeige;
			/** Die Teile in der Reihenfolge der Seite. */
			teile: ProjectSummary[];
			/** Der dringendere der Teile. */
			bedeutung: Bedeutung;
			/** Das Statuswort dieses Teils. */
			status: string;
	  };

/**
 * Aus den gezeigten Projekten (schon gefiltert und sortiert) die Karten:
 * Eine Gruppe steht einmal, an der Stelle ihres ersten Teils (`teile[0]`;
 * ist der nicht gezeigt, an der Stelle des ersten gezeigten Teils), mit
 * allen Teilen aus `alle`. Ohne Gruppen je Projekt eine Karte wie bisher.
 */
export function kachelnAus(
	gezeigt: readonly ProjectSummary[],
	alle: readonly ProjectSummary[],
	gruppen: readonly GruppeAnzeige[] = []
): Kachel[] {
	gruppen = gueltigeGruppen(gruppen, alle);
	const nachSlug = new Map(alle.map((p) => [p.slug, p]));
	const aus: Kachel[] = [];
	const gesetzt = new Set<string>();
	for (const p of gezeigt) {
		const g = gruppeVon(gruppen, p.slug);
		if (g === null) {
			aus.push({ art: 'projekt', schluessel: p.slug, projekt: p, bedeutung: p.bedeutung });
			continue;
		}
		if (gesetzt.has(g.slug)) continue;
		const erster = g.teile[0];
		const ersterGezeigt = erster !== undefined && gezeigt.some((x) => x.slug === erster);
		// Erst an der Stelle von teile[0], wenn der gezeigt wird.
		if (ersterGezeigt && p.slug !== erster) continue;
		gesetzt.add(g.slug);
		const teile = g.teile.flatMap((t) => {
			const x = nachSlug.get(t);
			return x === undefined ? [] : [x];
		});
		const vorn = dringendster(teile) ?? p;
		aus.push({
			art: 'gruppe',
			schluessel: gruppenSchluessel(g.slug),
			gruppe: g,
			teile,
			bedeutung: vorn.bedeutung,
			status: vorn.status
		});
	}
	return aus;
}

/** Die Projekte einer Karte (bei einer Gruppe ihre Teile). */
export function projekteDer(k: Kachel): ProjectSummary[] {
	return k.art === 'gruppe' ? k.teile : [k.projekt];
}

/** Wie viele Einträge in den Teilen einer Gruppe auf den Inhaber warten (Summe). */
export function wartetSumme(
	teile: readonly string[],
	wartet: Readonly<Record<string, number>>
): number {
	return teile.reduce((n, t) => n + (wartet[t] ?? 0), 0);
}

/** Sprungmarke eines Teils auf der Gruppenseite. */
export function teilAnker(slug: string): string {
	return `teil-${slug}`;
}
