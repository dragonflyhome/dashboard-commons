/**
 * Auswertung der Projektliste für die Startseite (Etappe 3b, Grenze
 * „Zustand im Server, Auswertung im Frontend“): Suchen, Filtern, Sortieren,
 * Zählen und Gruppieren der Projekte geschieht hier und nur hier, wie
 * Anstehend in anstehend.ts. Die Startseite zeigt nur, was `auswerten`
 * liefert; der Wächter in anstehend.grenze.spec.ts lehnt eigenes Sortieren,
 * Filtern und Zählen in der Seite ab.
 *
 * Status (`status`, das Wort aus dem Kopf) und seine Bedeutung (`bedeutung`)
 * kommen vom Server (/api/projects); die Spalten der Ansicht „Spalten“ sind
 * die Bedeutungen in der Reihenfolge von `GRUPPEN`, dieselben wie in der
 * Zählzeile. Seit 3b steht hier auch die Zählzeile (vorher zaehlung.ts).
 *
 * Etappe 4c: Auch der Bau-Stand der Seiten wird nur hier ausgewertet
 * (`bauAnzeige`): Kopf der Projektseite, Punkt in der
 * Seitenleiste, Karten und Zählzeile der Startseite lesen alle dieselbe
 * Auswertung, keine zweite Zählung (Plan 4, Ergänzung 4c).
 */
import type {
	BauEvent,
	Bedeutung,
	GruppeAnzeige,
	ProjectSummary,
	ProjektBau,
	SeitenBau
} from '$vertrag';
import { fmtTagUhrzeit } from './format.ts';
import {
	dringendster,
	gruppeVon,
	gueltigeGruppen,
	kachelnAus,
	projekteDer,
	type Kachel
} from './gruppen.ts';

/** Wort je Bedeutung in Zählzeile und Spalten, in Anzeigereihenfolge. */
export const GRUPPEN: readonly (readonly [Bedeutung, string])[] = [
	['laeuft', 'aktiv'],
	['info', 'in Arbeit'],
	['ok', 'fertig'],
	['wartet', 'pausiert'],
	['neutral', 'geplant'],
	['problem', 'mit Problem']
];

export interface Gruppe {
	bedeutung: Bedeutung;
	wort: string;
	anzahl: number;
}

export interface Zaehlung {
	gesamt: number;
	/** Nur Gruppen mit mindestens einem Projekt, in der Reihenfolge von `GRUPPEN`. */
	gruppen: Gruppe[];
	/** Quer dazu: Projekte mit ungesicherten Dateien in einem Repo. */
	ungesichert: number;
	/** Etappe 4c, quer dazu: Projekte, deren Seite gerade rot ist. */
	seitenRot: number;
}

type Zaehlbar = Pick<ProjectSummary, 'bedeutung' | 'repos'> & { slug?: string };

/**
 * Zählzeile der Startseite: alle Projekte, nach Bedeutung gruppiert. Jedes
 * Projekt fällt in genau eine Gruppe, die Summe der Gruppen ist die
 * Gesamtzahl (auch fertige Projekte zählen mit).
 */
export function zaehleProjekte(
	projekte: readonly Zaehlbar[],
	bau: Readonly<Record<string, BauAnzeige | null>> = {}
): Zaehlung {
	const gruppen = GRUPPEN.map(([bedeutung, wort]) => ({
		bedeutung,
		wort,
		anzahl: projekte.filter((p) => p.bedeutung === bedeutung).length
	})).filter((g) => g.anzahl > 0);
	return {
		gesamt: projekte.length,
		gruppen,
		ungesichert: projekte.filter((p) => p.repos.some((r) => (r.dirty_files ?? 0) > 0)).length,
		seitenRot: projekte.filter((p) => p.slug !== undefined && bau[p.slug]?.zustand === 'rot').length
	};
}

/** Eine Karte zum Zählen: bei einer Gruppe ihre Bedeutung, alle Repos und ein roter Teil, wenn es einen gibt. */
function zaehlbar(bau: Readonly<Record<string, BauAnzeige | null>>): (k: Kachel) => Zaehlbar {
	return (k) => {
		if (k.art === 'projekt') return k.projekt;
		const rot = k.teile.find((t) => bau[t.slug]?.zustand === 'rot');
		const z: Zaehlbar = { bedeutung: k.bedeutung, repos: k.teile.flatMap((t) => t.repos) };
		if (rot !== undefined) z.slug = rot.slug;
		return z;
	};
}

/** „19 Projekte: 14 aktiv, 3 fertig, 2 pausiert · 1 mit ungesicherten Dateien“. */
export function zaehlzeile(z: Zaehlung): string {
	const kopf = `${String(z.gesamt)} ${z.gesamt === 1 ? 'Projekt' : 'Projekte'}`;
	const teile = z.gruppen.map((g) => `${String(g.anzahl)} ${g.wort}`).join(', ');
	let zeile = teile === '' ? kopf : `${kopf}: ${teile}`;
	if (z.ungesichert > 0) zeile += ` · ${String(z.ungesichert)} mit ungesicherten Dateien`;
	if (z.seitenRot > 0)
		zeile += ` · ${String(z.seitenRot)} ${z.seitenRot === 1 ? 'Seite' : 'Seiten'} mit Fehler`;
	return zeile;
}

// ----- Startseite: Suchen, Filtern, Sortieren, Spalten -----

export type Sortierung = 'order' | 'title' | 'change';
export type Ansicht = 'kacheln' | 'spalten';

export interface Auswahl {
	/** Freitext; jedes Wort muss in Titel, Satz, Chat oder Slug vorkommen. */
	suche: string;
	/** Statuswort aus dem Kopf oder `alle`. */
	status: string;
	sortierung: Sortierung;
}

/**
 * Eine Spalte der Ansicht „Spalten“: eine Bedeutung mit ihren Karten. Eine
 * Projektgruppe ist eine Karte und steht in der Spalte ihres dringenderen
 * Teils (Entscheidung projektgruppe).
 */
export interface Spalte {
	bedeutung: Bedeutung;
	wort: string;
	/** Die Projekte der Karten (bei einer Gruppe ihre Teile). */
	projekte: ProjectSummary[];
	/** Die Karten der Spalte (ohne Gruppen je Projekt eine). */
	karten: Kachel[];
	/** So viele Karten stehen in der Spalte (nach Suche und Filter). */
	anzahl: number;
	/** So viele Karten dieser Bedeutung gibt es insgesamt (ohne Suche und Filter). */
	gesamt: number;
}

export interface Auswertung {
	/** Die Statuswörter für die Filter-Knöpfe, alphabetisch. */
	statusWoerter: string[];
	/** Gefiltert und sortiert, für die Kacheln. */
	gezeigt: ProjectSummary[];
	/**
	 * Dieselben als Karten: eine Projektgruppe als eine Karte an der Stelle
	 * ihres ersten Teils (Entscheidung projektgruppe).
	 */
	kacheln: Kachel[];
	/**
	 * Dieselben Projekte nach Bedeutung, in der Reihenfolge von `GRUPPEN`;
	 * leere Spalten fehlen. Jedes gezeigte Projekt steht in genau einer.
	 */
	spalten: Spalte[];
	zaehlung: Zaehlung;
	zeile: string;
	/** Etappe 4c: Bau-Stand je Slug (nur Projekte mit eigener Seite). */
	bau: Record<string, BauAnzeige>;
}

function passt(p: ProjectSummary, suche: string): boolean {
	const q = suche.trim().toLowerCase();
	if (q === '') return true;
	const heu = `${p.title} ${p.summary} ${p.chat ?? ''} ${p.slug}`.toLowerCase();
	return q.split(/\s+/).every((w) => heu.includes(w));
}

function vergleich(a: ProjectSummary, b: ProjectSummary, s: Sortierung): number {
	switch (s) {
		case 'order':
			return a.order - b.order || a.title.localeCompare(b.title, 'de');
		case 'title':
			return a.title.localeCompare(b.title, 'de');
		case 'change':
			return b.last_change.localeCompare(a.last_change) || a.title.localeCompare(b.title, 'de');
	}
}

/**
 * Alles, was die Startseite über die Projekte zeigt, aus einer Liste und der
 * Auswahl. `live`: die letzten Ereignisse `bau` (nur angemeldet, Etappe 4c).
 */
export function auswerten(
	projekte: readonly ProjectSummary[],
	wahl: Auswahl,
	live: BauLive = {},
	gruppen: readonly GruppeAnzeige[] = []
): Auswertung {
	const gezeigt = projekte
		.filter((p) => wahl.status === 'alle' || p.status === wahl.status)
		.filter((p) => passt(p, wahl.suche))
		.sort((a, b) => vergleich(a, b, wahl.sortierung));
	// Unbekannte Bedeutungen gibt es im Vertrag nicht; zur Sicherheit landen sie bei „geplant“.
	const bekannt = new Set<Bedeutung>(GRUPPEN.map(([b]) => b));
	const kacheln = kachelnAus(gezeigt, projekte, gruppen);
	const alleKacheln = kachelnAus(projekte, projekte, gruppen);
	const vonKachel = (k: Kachel): Bedeutung => (bekannt.has(k.bedeutung) ? k.bedeutung : 'neutral');
	const spalten = GRUPPEN.map(([bedeutung, wort]): Spalte => {
		const karten = kacheln.filter((k) => vonKachel(k) === bedeutung);
		return {
			bedeutung,
			wort,
			projekte: karten.flatMap(projekteDer),
			karten,
			anzahl: karten.length,
			gesamt: alleKacheln.filter((k) => vonKachel(k) === bedeutung).length
		};
	}).filter((s) => s.anzahl > 0);
	const bau = bauJeProjekt(projekte, live);
	// Projektgruppen: Die Zählzeile zählt wie die Karten, eine Gruppe einmal
	// (mit dem dringenderen Teil; ungesichert oder rot, wenn ein Teil es ist).
	const zaehlung = zaehleProjekte(alleKacheln.map(zaehlbar(bau)), bau);
	return {
		statusWoerter: [...new Set(projekte.map((p) => p.status))].sort(),
		gezeigt,
		kacheln,
		spalten,
		zaehlung,
		zeile: zaehlzeile(zaehlung),
		bau
	};
}

// ----- Bau-Stand der Seiten (Etappe 4c) -----

/** Die letzten Ereignisse `bau` je Slug (geschützter Kanal; ohne Anmeldung leer). */
export type BauLive = Readonly<Record<string, BauEvent>>;

/** Was die Oberfläche zum Bau einer Seite zeigt. */
export interface BauAnzeige {
	zustand: SeitenBau;
	/** Farbe des Punkts. */
	bedeutung: Bedeutung;
	/** Kurz, für Punkt, Leiste und Karte: „Seite in Ordnung“, „Seite mit Fehler“ … */
	wort: string;
	/** Die Zeile im Kopf der Projektseite. */
	zeile: string;
	/** Bei rot und nicht geprüft der Satz des Servers (nur angemeldet, `/api/bauen`), sonst `null`. */
	satz: string | null;
	/** Tag und Uhrzeit der letzten guten Fassung, sonst `null`. */
	zeit: string | null;
}

const BAU_BEDEUTUNG: Readonly<Record<SeitenBau, Bedeutung>> = {
	gruen: 'ok',
	rot: 'problem',
	laeuft: 'laeuft',
	noch_nicht: 'neutral',
	unbekannt: 'wartet',
	nicht_geprueft: 'wartet'
};

const BAU_WORT: Readonly<Record<SeitenBau, string>> = {
	gruen: 'Seite in Ordnung',
	rot: 'Seite mit Fehler',
	laeuft: 'Seite wird gebaut',
	noch_nicht: 'Seite noch nicht gebaut',
	unbekannt: 'Bau-Stand gerade nicht lesbar',
	nicht_geprueft: 'Seite noch nicht geprüft'
};

/**
 * Der Bau-Stand einer Seite: `bau` aus `/api/projects` (frei), dazu das
 * letzte Ereignis `bau` (geschützt; gilt, bis die Liste neu da ist) und bei
 * rot und nicht geprüft der Satz aus `/api/bauen` (geschützt). Ohne `bau` (Projekt ohne eigene
 * Seite) `null`. Rot heißt: Das Dashboard zeigt die letzte gute Fassung
 * (`zeit`); ohne `zeit` fehlt die Seite gerade (keine Kopie, oder in der
 * Gesamtprüfung ganz ausgeschlossen). Ein Ereignis eines ganzen Baus, der
 * nicht durchlief (`gescheitert`), sagt nichts über die Seite selbst.
 */
export function bauAnzeige(
	bau: ProjektBau | null | undefined,
	live: BauEvent | null = null,
	satz: string | null = null,
	now: Date = new Date()
): BauAnzeige | null {
	if (bau === null || bau === undefined) return null;
	const vomServer: SeitenBau =
		live?.gescheitert === true && bau.zustand === 'laeuft' ? 'unbekannt' : bau.zustand;
	// Das Ereignis `bau` kennt kein „nicht geprüft“ und meldet solche Seiten
	// grün: Ein grünes Ereignis zeigt darum bis zur nächsten Liste „wird gebaut“,
	// erst die Liste sagt „in Ordnung“.
	const liveGilt = live !== null && !live.gescheitert;
	const zustand: SeitenBau = liveGilt
		? live.zustand === 'gruen'
			? 'laeuft'
			: live.zustand
		: vomServer;
	const zeit = fmtTagUhrzeit(bau.letzte_gute, now);
	let zeile: string;
	switch (zustand) {
		case 'gruen':
			// Ohne Zeit: Eine Zeitzeile darunter gibt es nicht.
			zeile = 'Seite in Ordnung';
			break;
		case 'rot':
			zeile =
				zeit === null
					? 'Seite mit Fehler, gerade nicht zu sehen'
					: `Seite mit Fehler: zeigt die letzte gute Fassung von ${zeit}`;
			break;
		case 'nicht_geprueft':
			// Ohne Anmeldung nur der
			// Zustand; angemeldet (mit Satz) wie bei rot die Zeit der Fassung.
			zeile =
				satz === null || zeit === null
					? 'Seite noch nicht geprüft'
					: `Seite noch nicht geprüft: zeigt die letzte geprüfte Fassung von ${zeit}`;
			break;
		case 'laeuft':
			zeile = 'Seite wird gebaut …';
			break;
		case 'noch_nicht':
			zeile = 'Seite noch nicht gebaut';
			break;
		case 'unbekannt':
			zeile = 'Bau-Stand gerade nicht lesbar';
			break;
	}
	return {
		zustand,
		bedeutung: BAU_BEDEUTUNG[zustand],
		wort: BAU_WORT[zustand],
		zeile,
		satz: zustand === 'rot' || zustand === 'nicht_geprueft' ? satz : null,
		zeit
	};
}

/** Kopfzeile, wenn die Seite gebaut ist, die Karte „Stand“ aber rot. */
export const SATZ_GEBAUT_STAND_ROT = 'offen ist etwas, siehe „Stand“';

/**
 * Ein grünes „Seite in Ordnung“ im Kopf neben einer roten
 * Karte „Stand“ (Prüfung rot, Fehler offen, Frist vorbei) widerspricht sich.
 * Ist der Stand rot, sagt der Kopf nur, dass die Seite gebaut ist, ohne Grün;
 * den Grund nennt die Karte. Sonst bleibt die Anzeige, wie sie ist.
 */
export function kopfMitStand(a: BauAnzeige | null, standRot: boolean): BauAnzeige | null {
	if (a === null || !standRot || a.zustand !== 'gruen') return a;
	const zeile =
		a.zeit === null
			? `Seite gebaut; ${SATZ_GEBAUT_STAND_ROT}.`
			: `Seite gebaut (Fassung von ${a.zeit}); ${SATZ_GEBAUT_STAND_ROT}.`;
	return { ...a, bedeutung: 'neutral', wort: 'Seite gebaut', zeile };
}

/** Der Bau-Stand aller Projekte mit eigener Seite, nach Slug. */
export function bauJeProjekt(
	projekte: readonly Pick<ProjectSummary, 'slug' | 'bau'>[],
	live: BauLive = {},
	saetze: Readonly<Record<string, string>> = {}
): Record<string, BauAnzeige> {
	const aus: Record<string, BauAnzeige> = {};
	for (const p of projekte) {
		const a = bauAnzeige(p.bau, live[p.slug] ?? null, saetze[p.slug] ?? null);
		if (a !== null) aus[p.slug] = a;
	}
	return aus;
}

// ----- Seitenleiste (Kopfleiste, Fassung 3) -----

/** Ein Projekt in der Seitenleiste. */
export interface LeistenProjekt {
	slug: string;
	titel: string;
	bedeutung: Bedeutung;
	/** Einträge, die auf den Inhaber warten (aus „Anstehend“; ohne Anmeldung 0). */
	wartet: number;
	/** Etappe 4c: Bau-Stand der Seite (`null`: ohne eigene Seite). */
	bau: BauAnzeige | null;
	/**
	 * Nur bei einer Projektgruppe (Entscheidung projektgruppe): Der
	 * Eintrag ist die Gruppe (`slug` ihre Adresse unter /gruppen, `bedeutung`
	 * der dringendere Teil, `wartet` die Summe), darunter eingerückt die Teile.
	 */
	teile?: LeistenProjekt[];
}

/** Ein Schwerpunkt mit seinen Projekten. */
export interface LeistenGruppe {
	name: string;
	projekte: LeistenProjekt[];
}

/** Schwerpunkt ohne Zuordnung (wie der Server, `vertrag::schwerpunkte::SONSTIGES`). */
export const SONSTIGES = 'Sonstiges';

/**
 * Die Projekte der Seitenleiste nach Schwerpunkt (Fakt `schwerpunkt` und
 * `schwerpunkt_reihenfolge` vom Server), nicht nach Status: die Schwerpunkte
 * in ihrer Reihenfolge, darin die Projekte wie die Kacheln „wie geplant“
 * (`order`, dann Titel). Ohne Schwerpunkt: „Sonstiges“ am Ende.
 *
 * @param wartet Zähler „wartet“ je Slug (`anstehend.wartet`)
 * @param live die letzten Ereignisse `bau` (Etappe 4c)
 */
export function leistenGruppen(
	projekte: readonly ProjectSummary[],
	wartet: Readonly<Record<string, number>> = {},
	live: BauLive = {},
	projektGruppen: readonly GruppeAnzeige[] = []
): LeistenGruppe[] {
	const bau = bauJeProjekt(projekte, live);
	projektGruppen = gueltigeGruppen(projektGruppen, projekte);
	const eintrag = (p: ProjectSummary): LeistenProjekt => ({
		slug: p.slug,
		titel: p.title,
		bedeutung: p.bedeutung,
		wartet: wartet[p.slug] ?? 0,
		bau: bau[p.slug] ?? null
	});
	const nachSlug = new Map(projekte.map((p) => [p.slug, p]));
	const reihe = new Map<string, number>();
	const gruppen = new Map<string, ProjectSummary[]>();
	for (const p of projekte) {
		const name = p.schwerpunkt ?? SONSTIGES;
		const r = p.schwerpunkt_reihenfolge ?? Number.MAX_SAFE_INTEGER;
		reihe.set(name, Math.min(reihe.get(name) ?? r, r));
		gruppen.set(name, [...(gruppen.get(name) ?? []), p]);
	}
	return [...gruppen.entries()]
		.sort(([a], [b]) => (reihe.get(a) ?? 0) - (reihe.get(b) ?? 0) || a.localeCompare(b, 'de'))
		.map(([name, liste]) => ({
			name,
			projekte: [...liste]
				.sort((a, b) => vergleich(a, b, 'order'))
				.flatMap((p): LeistenProjekt[] => {
					// Projektgruppe: ein Eintrag an der Stelle des ersten Teils, die Teile darunter.
					const g = gruppeVon(projektGruppen, p.slug);
					if (g === null) return [eintrag(p)];
					const erster = g.teile.find((t) => nachSlug.has(t));
					if (erster !== p.slug) return [];
					const teile = g.teile.flatMap((t) => {
						const x = nachSlug.get(t);
						return x === undefined ? [] : [eintrag(x)];
					});
					return [
						{
							slug: g.slug,
							titel: g.titel,
							bedeutung: dringendster(teile)?.bedeutung ?? p.bedeutung,
							wartet: teile.reduce((n, t) => n + t.wartet, 0),
							bau: null,
							teile
						}
					];
				})
		}))
		.filter((g) => g.projekte.length > 0);
}

/** Wohin eine Unterseite in der Leiste führt; die Adresse baut die Komponente. */
export type UnterseitenZiel = { art: 'unterseite'; name: string } | { art: 'vorschau' };

/** Eine Unterseite des aktuellen Projekts, eingerückt unter ihm. */
export interface LeistenUnterseite {
	/** Wie `Markiert.unterseite` in kopfleiste.ts: Name oder `vorschau`. */
	schluessel: string;
	titel: string;
	ziel: UnterseitenZiel;
}

function feld(o: unknown, k: string): unknown {
	return typeof o === 'object' && o !== null ? (o as Record<string, unknown>)[k] : undefined;
}

/** `[[seiten]]` aus einem Kopf der neuen Form (`ProjektAntwort`), nach `order`. */
function seitenAus(kopf: unknown): { name: string; title: string }[] {
	const seiten = feld(feld(kopf, 'kopf'), 'seiten');
	if (!Array.isArray(seiten)) return [];
	return (seiten as unknown[])
		.flatMap((s) => {
			const name = feld(s, 'name');
			const title = feld(s, 'title');
			const order = feld(s, 'order');
			return typeof name === 'string'
				? [{ name, title: typeof title === 'string' ? title : name, order: Number(order) || 0 }]
				: [];
		})
		.sort((a, b) => a.order - b.order || a.name.localeCompare(b.name));
}

/**
 * Die Unterseiten des aktuellen Projekts für die Leiste, aus den Daten, die
 * die Seite ohnehin geladen hat (`page.data`, mit `kopf` aus projekt.toml) und
 * dem Eintrag der Projektliste:
 *
 * - die Unterseiten unter /projects/<slug>/<name>;
 * - mit Entwurf (`vorschau`): „Vorschau“.
 */
export function leistenUnterseiten(
	data: unknown,
	projekt: Pick<ProjectSummary, 'vorschau'> | null
): LeistenUnterseite[] {
	const kopf = feld(data, 'kopf');
	const aus: LeistenUnterseite[] = [];
	for (const s of seitenAus(kopf)) {
		aus.push({ schluessel: s.name, titel: s.title, ziel: { art: 'unterseite', name: s.name } });
	}
	if (projekt?.vorschau)
		aus.push({ schluessel: 'vorschau', titel: 'Vorschau', ziel: { art: 'vorschau' } });
	return aus;
}

// ----- Wahl Kacheln/Spalten (je Browser) -----

/** Schlüssel im localStorage; ohne Wahl gelten die Kacheln (plan-3b.toml, Antwort 2). */
export const ANSICHT_SCHLUESSEL = 'dashboard-startseite-ansicht';

export function ansichtLesen(): Ansicht {
	try {
		return localStorage.getItem(ANSICHT_SCHLUESSEL) === 'spalten' ? 'spalten' : 'kacheln';
	} catch {
		return 'kacheln';
	}
}

export function ansichtMerken(a: Ansicht): void {
	try {
		if (a === 'kacheln') localStorage.removeItem(ANSICHT_SCHLUESSEL);
		else localStorage.setItem(ANSICHT_SCHLUESSEL, a);
	} catch {
		// Speicher gesperrt: gilt bis zum Neuladen.
	}
}
