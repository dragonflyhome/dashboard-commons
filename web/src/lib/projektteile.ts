/**
 * Die vier Teile der Projektseite (Etappe B8): Stand, Aufgabenliste, Plan
 * und Zeitleiste. Stand, Zählung und Verlauf rechnet der Server aus dem
 * Tracker (`/stand`, `/backlog`, `/plan`, `/verlauf`); hier wird nur geladen
 * und in Wörter, Gruppen und den Zeitstreifen übersetzt. Die Teile zeigen
 * nur an.
 */
import { API } from './api/client.ts';
import { ART_WORT, kurzDatum } from './anstehend.ts';
import { chatWort } from './aufgaben.ts';
import { fmtDate } from './format.ts';
import { SCHRITT_STAND, wartetAufWort } from './katalog/widgets/worte.ts';
import { gepolstert, position } from './zeit/achse.ts';
import { tagAusZahl, tagPlus, tagText, tagZahl } from './zeit/tage.ts';
import type {
	AmpelGrund,
	AnstehendEintrag,
	Backlog,
	EintragsArt,
	BacklogKarte,
	Bedeutung,
	EtappenStand,
	PlanArbeit,
	PlanEtappe,
	ProjektPlan,
	StandAmpel,
	Verlauf,
	VerlaufEreignis
} from '$vertrag';

// ----- Laden -----

/** Was ein geschützter Weg geliefert hat: die Daten, oder warum nicht. */
export interface Ladung<T> {
	wert: T | null;
	/** 401 oder 403: nur mit Anmeldung. */
	anmelden: boolean;
	/**
	 * Der laufende Dienst kennt den Weg nicht (404, 405) oder liefert nicht die
	 * Felder, die diese Oberfläche braucht (etwa eine ältere Fassung des Dienstes).
	 */
	fehlt: boolean;
}

/** Satz eines Teils, wenn `fehlt` gilt; die übrigen Teile bleiben sichtbar. */
export const SATZ_TEIL_FEHLT = 'Dieser Teil ist mit dem laufenden Dienst noch nicht verfügbar.';

async function holen<T>(
	pfad: string,
	fetchFn: typeof fetch,
	passt: (x: unknown) => x is T
): Promise<Ladung<T>> {
	try {
		const res = await fetchFn(pfad, {
			credentials: 'same-origin',
			headers: { accept: 'application/json' }
		});
		if (res.ok) {
			const roh: unknown = await res.json();
			return passt(roh)
				? { wert: roh, anmelden: false, fehlt: false }
				: { wert: null, anmelden: false, fehlt: true };
		}
		return {
			wert: null,
			anmelden: res.status === 401 || res.status === 403,
			fehlt: res.status === 404 || res.status === 405
		};
	} catch {
		return { wert: null, anmelden: false, fehlt: false };
	}
}

// ----- Prüfen an der Grenze: jedes Pflichtfeld der Vertragstypen ($vertrag) -----

type Roh = Record<string, unknown>;
const istObjekt = (x: unknown): x is Roh =>
	typeof x === 'object' && x !== null && !Array.isArray(x);
const istText = (x: unknown): x is string => typeof x === 'string';
const istZahl = (x: unknown): x is number => typeof x === 'number';
const istJaNein = (x: unknown): x is boolean => typeof x === 'boolean';
const textOderNull = (x: unknown): boolean => x === null || typeof x === 'string';
const listeAus = (x: unknown, je: (e: unknown) => boolean): boolean =>
	Array.isArray(x) && x.every(je);

/** `Schritt` */
function istSchritt(s: unknown): boolean {
	return (
		istObjekt(s) &&
		istZahl(s.nr) &&
		istText(s.titel) &&
		istText(s.stand) &&
		istText(s.seit) &&
		[s.besitzer, s.wartet_auf, s.verweis, s.angeboten_an, s.fuer, s.notiz, s.vorgang].every(
			textOderNull
		)
	);
}

/** `BacklogKarte` */
function istKarte(k: unknown): boolean {
	return (
		istObjekt(k) &&
		[k.id, k.art, k.titel, k.zustand, k.ziel, k.geaendert].every(istText) &&
		[k.etappe, k.besitzer, k.wartet_auf, k.hinweis, k.frist, k.frist_stufe].every(textOderNull) &&
		listeAus(k.schritte, istSchritt)
	);
}

/** Hat die Antwort von `/backlog` die Form, die die Aufgabenliste liest? */
export function istBacklog(x: unknown): x is Backlog {
	return (
		istObjekt(x) &&
		istText(x.slug) &&
		istText(x.heute) &&
		istZahl(x.fragen_weitere) &&
		istJaNein(x.kanban_datei) &&
		istJaNein(x.speicher_fehlt) &&
		listeAus(
			x.spalten,
			(s) =>
				istObjekt(s) && [s.art, s.titel, s.bedeutung].every(istText) && listeAus(s.karten, istKarte)
		) &&
		listeAus(
			x.wartend,
			(g) =>
				istObjekt(g) && [g.rolle, g.wer, g.titel].every(istText) && listeAus(g.karten, istKarte)
		)
	);
}

/** `PlanArbeit` */
function istPlanArbeit(a: unknown): boolean {
	return (
		istObjekt(a) &&
		[a.id, a.titel, a.stand].every(istText) &&
		[a.aus, a.etappe, a.fertig].every(textOderNull) &&
		istJaNein(a.archiviert) &&
		istJaNein(a.ungefaehr)
	);
}

/** `PlanEtappe` */
function istEtappe(e: unknown): boolean {
	return (
		istObjekt(e) &&
		[e.kennung, e.titel, e.ziel, e.stand].every(istText) &&
		[e.reihenfolge, e.arbeiten_fertig, e.arbeiten_gesamt, e.dazugekommen].every(istZahl) &&
		[e.ab, e.begonnen, e.fertig, e.gewachsen_seit].every(textOderNull) &&
		istJaNein(e.ungefaehr) &&
		listeAus(e.arbeiten, istPlanArbeit)
	);
}

/** Hat die Antwort von `/plan` die Form, die der Plan liest? */
export function istPlan(x: unknown): x is ProjektPlan {
	return (
		istObjekt(x) &&
		istText(x.projekt) &&
		listeAus(x.etappen, istEtappe) &&
		listeAus(x.ohne_etappe, istPlanArbeit) &&
		listeAus(x.hinweise, istText)
	);
}

/** Hat die Antwort von `/verlauf` die Form, die die Zeitleiste liest? */
export function istVerlauf(x: unknown): x is Verlauf {
	return (
		istObjekt(x) &&
		listeAus(
			x.ereignisse,
			(e) =>
				istObjekt(e) &&
				[e.kennung, e.zeit, e.genauigkeit, e.titel, e.quelle, e.art].every(istText) &&
				[e.bis, e.text, e.verweis].every(textOderNull)
		) &&
		istText(x.seit) &&
		istText(x.bis) &&
		listeAus(x.hinweise, (h) => istObjekt(h) && istText(h.art) && istText(h.text))
	);
}

/** `GET /api/projekte/<slug>/backlog` (geschützt), die Aufgabenliste. Wirft nie. */
export function ladeBacklog(slug: string, fetchFn: typeof fetch): Promise<Ladung<Backlog>> {
	return holen(`${API}/projekte/${encodeURIComponent(slug)}/backlog`, fetchFn, istBacklog);
}

/** `GET /api/projekte/<slug>/plan` (geschützt). Wirft nie. */
export function ladePlan(slug: string, fetchFn: typeof fetch): Promise<Ladung<ProjektPlan>> {
	return holen(`${API}/projekte/${encodeURIComponent(slug)}/plan`, fetchFn, istPlan);
}

/** Die Zeitleiste zeigt zuerst so viele Tage bis heute. */
export const ZEITLEISTE_TAGE = 14;
/** „Ältere anzeigen“: so weit zurück, wie der Server erlaubt (400 Tage). */
export const ZEITLEISTE_AELTER = 400;

/** `GET /api/projekte/<slug>/verlauf?seit=` (geschützt) für `tage` Tage bis heute. Wirft nie. */
export function ladeProjektVerlauf(
	slug: string,
	tage: number,
	fetchFn: typeof fetch,
	jetzt: Date = new Date()
): Promise<Ladung<Verlauf>> {
	const seit = tagPlus(tagText(jetzt), -(tage - 1));
	return holen(
		`${API}/projekte/${encodeURIComponent(slug)}/verlauf?seit=${seit}`,
		fetchFn,
		istVerlauf
	);
}

// ----- Anker -----

export type ListenArt = 'dich' | 'laeuft' | 'naechstes' | 'andere';

/** Anker einer Gruppe der Aufgabenliste (kein Eintrags-Anker wie `arbeit-…`). */
export function gruppenAnker(slug: string, art: ListenArt): string {
	return `liste-${art}-${slug}`;
}

/** Anker der ganzen Aufgabenliste (eine Arbeit kann in jeder Gruppe stehen). */
export function listenAnker(slug: string): string {
	return `aufgabenliste-${slug}`;
}

/** Anker der Zeitleiste. */
export function zeitleisteAnker(slug: string): string {
	return `zeitleiste-${slug}`;
}

// ----- 1 Stand -----

/** Die Zahlen des Stands, wie `KurzAnsicht` sie hat. */
export interface StandKurz {
	wartet_auf_dich: number;
	laeuft: number;
	als_naechstes: number;
}

export interface StandZahl {
	zahl: number;
	wort: string;
	/** Anker des Sprungziels in der Aufgabenliste oder der Zeitleiste (ohne `#`). */
	ziel: string;
}

/** Die vier Zahlen des Stands; jede springt zu ihrer Gruppe. */
export function standZahlen(slug: string, k: StandKurz, fertig7: number): StandZahl[] {
	return [
		{
			zahl: k.laeuft,
			wort: k.laeuft === 1 ? 'läuft' : 'laufen',
			ziel: gruppenAnker(slug, 'laeuft')
		},
		{
			zahl: k.wartet_auf_dich,
			wort: k.wartet_auf_dich === 1 ? 'wartet auf dich' : 'warten auf dich',
			ziel: gruppenAnker(slug, 'dich')
		},
		{ zahl: k.als_naechstes, wort: 'als Nächstes', ziel: gruppenAnker(slug, 'naechstes') },
		{ zahl: fertig7, wort: 'fertig in 7 Tagen', ziel: zeitleisteAnker(slug) }
	];
}

/** Ein Satz über die Arbeit; was wartet, sagt die Ampel daneben. */
export function standSatz(k: StandKurz, fertig7: number): string {
	const laufen =
		k.laeuft === 0
			? 'Gerade läuft keine Arbeit.'
			: k.laeuft === 1
				? 'Eine Arbeit läuft.'
				: `${String(k.laeuft)} Arbeiten laufen.`;
	const fertig =
		fertig7 === 0
			? 'In den letzten 7 Tagen ist keine fertig geworden.'
			: fertig7 === 1
				? 'In den letzten 7 Tagen ist eine fertig geworden.'
				: `In den letzten 7 Tagen sind ${String(fertig7)} fertig geworden.`;
	return `${laufen} ${fertig}`;
}

/** Die Ampel als Bedeutung in Worten (nie nur als Farbe). */
export const AMPEL_WORT: Record<StandAmpel, string> = {
	gruen: 'In Ordnung',
	gelb: 'Wartet',
	rot: 'Problem',
	unbekannt: 'Unbekannt'
};

/** Anker, zu dem der Grund-Satz der Ampel führt (ohne `#`); `null` ohne passendes Ziel. */
export function ampelZiel(
	g: { grund_art: AmpelGrund; bezug_art: EintragsArt | null },
	slug: string
): string | null {
	switch (g.grund_art) {
		case 'wartet':
		case 'wartet-lange':
			return gruppenAnker(slug, 'dich');
		case 'ohne-bewegung':
			return gruppenAnker(slug, 'laeuft');
		case 'fehler-offen':
			return zeitleisteAnker(slug);
		case 'frist-vorbei':
			// Die Zeitleiste zeigt nur 14 Tage; Aufgabe, Frage und Arbeit stehen sicher in der Liste.
			// Eine Frist aus fristen.toml (ohne bezug_art) hat kein sicheres Ziel; der Satz nennt sie.
			if (g.bezug_art === 'aufgabe' || g.bezug_art === 'frage') return gruppenAnker(slug, 'dich');
			return g.bezug_art === 'arbeit' ? listenAnker(slug) : null;
		case 'nichts-wartet':
		case 'speicher-fehlt':
			return null;
	}
}

// ----- 2 Aufgabenliste -----

export interface ListenGruppe {
	art: ListenArt;
	titel: string;
	karten: BacklogKarte[];
}

const LISTEN_TITEL: Record<ListenArt, string> = {
	dich: 'Wartet auf dich',
	laeuft: 'Läuft',
	naechstes: 'Als Nächstes',
	andere: 'Wartet auf andere'
};

/**
 * Das Backlog als Gruppen untereinander: Wartet auf dich, Läuft, Als
 * Nächstes, Wartet auf andere (alle vier, auch leere), aus den Spalten des
 * Servers: Eine Arbeit, die über eine offene Frage auf dich wartet, steht
 * wie dort bei „Wartet“, die Frage selbst bei dir. `mitKasten`: Fragen
 * und Aufgaben stehen auf der Projektseite mit ihren Knöpfen (Frage mit
 * Antwort, AufgabenKasten samt „Beim Chat“); dann bleiben hier nur die
 * Arbeiten, die auf dich warten, und bei den anderen keine Aufgaben.
 */
export function listenGruppen(b: Backlog, mitKasten: boolean): ListenGruppe[] {
	const spalte = (art: string): BacklogKarte[] =>
		b.spalten.find((s) => s.art === art)?.karten ?? [];
	const dich = spalte('fuer-dich').filter((k) => !mitKasten || k.art === 'arbeit');
	const andere = spalte('wartet').filter((k) => !mitKasten || k.art !== 'aufgabe');
	const gruppe = (art: ListenArt, karten: BacklogKarte[]): ListenGruppe => ({
		art,
		titel: LISTEN_TITEL[art],
		karten
	});
	return [
		gruppe('dich', dich),
		gruppe('laeuft', spalte('laeuft')),
		gruppe('naechstes', spalte('als-naechstes')),
		gruppe('andere', andere)
	];
}

/** Titel der Etappen des Plans nach `<projekt>/<kennung>`. */
export function etappenNamen(plan: ProjektPlan | null): Map<string, string> {
	return new Map(
		(plan?.etappen ?? []).map((e) => [`${plan?.projekt ?? ''}/${e.kennung}`, e.titel])
	);
}

/**
 * Die Angaben einer Zeile in Worten: Art (außer bei Arbeiten), Etappe, wer
 * dran ist (ohne Schritte; mit Schritten sagt es ihre Kurzzeile), Hinweis.
 * Die Frist steht eigens daneben.
 */
export function zeilenAngaben(k: BacklogKarte, etappen: ReadonlyMap<string, string>): string[] {
	const aus: string[] = [];
	if (k.art !== 'arbeit') aus.push(ART_WORT[k.art]);
	if (k.etappe !== null) {
		aus.push(`Etappe: ${etappen.get(k.etappe) ?? k.etappe.slice(k.etappe.indexOf('/') + 1)}`);
	}
	const wartet = wartetAufWort(k.wartet_auf);
	if (wartet !== null) aus.push(wartet);
	else if (k.besitzer !== null && k.schritte.length === 0)
		aus.push(`dran: ${chatWort(k.besitzer)}`);
	if (k.hinweis !== null) aus.push(k.hinweis);
	return aus;
}

/** Die Kennungen offener Fragen und Aufgaben (Ziel eines wartenden Schritts). */
export function offeneVerweise(b: Backlog): string[] {
	return b.spalten.flatMap((s) =>
		s.karten.filter((k) => k.art === 'frage' || k.art === 'aufgabe').map((k) => k.id)
	);
}

/** Als Nächstes zeigt zuerst so viele, den Rest zum Aufklappen. */
export const NAECHSTES_SICHTBAR = 5;

// ----- 3 Plan -----

/** RFC 3339 oder JJJJ-MM-TT → „05.11.“. */
function datum(zeit: string): string {
	return kurzDatum(zeit.slice(0, 10));
}

export const ETAPPEN_BEDEUTUNG: Record<EtappenStand, Bedeutung> = {
	geplant: 'neutral',
	laeuft: 'laeuft',
	fertig: 'ok',
	'vor-dem-tracker': 'ok'
};

/**
 * Das Stand-Wort einer Etappe: geplant, läuft seit …, fertig am …, vor dem
 * Tracker abgeschlossen; mit echtem Termin dahinter „· ab 03/2027“ (der
 * Zeitstreifen zeigt ihn nur als Raute, ohne Text).
 */
export function etappenWort(e: PlanEtappe): string {
	const wort = standWort(e);
	return e.ab === null ? wort : `${wort} · ab ${fmtDate(e.ab)}`;
}

function standWort(e: PlanEtappe): string {
	switch (e.stand) {
		case 'geplant':
			return 'geplant';
		case 'laeuft':
			return e.begonnen === null ? 'läuft' : `läuft seit ${datum(e.begonnen)}`;
		case 'fertig':
			if (e.fertig === null) return 'fertig';
			return `fertig am ${datum(e.fertig)}${e.ungefaehr ? ' (ungefähr)' : ''}`;
		case 'vor-dem-tracker':
			return 'vor dem Tracker abgeschlossen';
	}
}

/** „3 von 4 Arbeiten fertig“; ohne Arbeiten `null`. */
export function zaehlung(e: PlanEtappe): string | null {
	if (e.arbeiten_gesamt === 0) return null;
	const n = e.arbeiten_gesamt;
	return `${String(e.arbeiten_fertig)} von ${String(n)} ${n === 1 ? 'Arbeit' : 'Arbeiten'} fertig`;
}

/** Wie weit, in Prozent (nur für den Balken; die Aussage steht in `zaehlung`). */
export function anteil(e: PlanEtappe): number {
	return e.arbeiten_gesamt === 0 ? 0 : (100 * e.arbeiten_fertig) / e.arbeiten_gesamt;
}

/** „seit 05.11. 1 Arbeit dazugekommen“, wenn die Etappe gewachsen ist. */
export function gewachsenSatz(e: PlanEtappe): string | null {
	if (e.gewachsen_seit === null || e.dazugekommen === 0) return null;
	const n = e.dazugekommen === 1 ? '1 Arbeit' : `${String(e.dazugekommen)} Arbeiten`;
	return `seit ${datum(e.gewachsen_seit)} ${n} dazugekommen`;
}

/** Eine Arbeit im Plan in Worten: Stand, wann fertig, aus welchem Projekt, ob im Archiv. */
export function planArbeitWort(a: PlanArbeit): string {
	const teile = [SCHRITT_STAND[a.stand]];
	if (a.fertig !== null)
		teile[0] = `fertig am ${datum(a.fertig)}${a.ungefaehr ? ' (ungefähr)' : ''}`;
	if (a.aus !== null) teile.push(`aus ${a.aus}`);
	if (a.archiviert) teile.push('im Archiv');
	return teile.join(' · ');
}

/** Ob eine Arbeit eine Zeile in der Aufgabenliste hat (offen und aus diesem Projekt). */
export function inListe(a: PlanArbeit): boolean {
	return a.stand !== 'fertig' && !a.archiviert && a.aus === null;
}

export interface StreifenZeile {
	kennung: string;
	titel: string;
	stand: EtappenStand;
	/** Links und Breite in Prozent; `null` ohne Ist-Daten. */
	balken: { links: number; breite: number; offen: boolean } | null;
	/** Ein echter Termin (`ab`), Position in Prozent. */
	raute: number | null;
}

export interface Streifen {
	zeilen: StreifenZeile[];
	/** Erster und letzter Tag der Achse (JJJJ-MM-TT). */
	von: string;
	bis: string;
	/** Heute auf der Achse, in Prozent. */
	heute: number;
}

/**
 * Der Zeitstreifen nur aus Ist-Daten: je Etappe ein Balken von begonnen bis
 * fertig, laufende bis heute (offen), geplante ohne Balken, echte Termine
 * aus `ab` als Raute. Nichts geschätzt. Ohne ein einziges Datum `null`.
 */
export function zeitstreifen(plan: ProjektPlan, heute: string): Streifen | null {
	const tag = (zeit: string | null): number | null => {
		if (zeit === null) return null;
		const z = tagZahl(zeit.length === 7 ? `${zeit}-01` : zeit);
		return Number.isNaN(z) ? null : z;
	};
	const jetzt = tagZahl(heute);
	const punkte = plan.etappen.flatMap((e) =>
		[tag(e.begonnen), tag(e.fertig), tag(e.ab)].filter((z): z is number => z !== null)
	);
	if (punkte.length === 0) return null;
	const roh = { von: Math.min(...punkte, jetzt), bis: Math.max(...punkte, jetzt) + 1 };
	const f = gepolstert(roh);
	const pos = (z: number): number => position(f, 100, z);
	const zeilen = plan.etappen.map((e): StreifenZeile => {
		const b = tag(e.begonnen);
		const ende = e.stand === 'fertig' ? tag(e.fertig) : e.stand === 'laeuft' ? jetzt : null;
		const balken =
			b === null || ende === null
				? null
				: { links: pos(b), breite: pos(ende + 1) - pos(b), offen: e.stand === 'laeuft' };
		const ab = tag(e.ab);
		return {
			kennung: e.kennung,
			titel: e.titel,
			stand: e.stand,
			balken,
			raute: ab === null ? null : pos(ab)
		};
	});
	return {
		zeilen,
		von: tagAusZahl(roh.von),
		bis: tagAusZahl(roh.bis - 1),
		heute: pos(jetzt + 0.5)
	};
}

// ----- 4 Zeitleiste -----

/**
 * Ungelesene Meldungen (aus den offenen Einträgen) in der Form des Widgets
 * `Meldung`, ohne die, deren Sache der Verlauf schon als eigene Zeile hat
 * (dieselbe Regel wie `aus_meldungen` im Server): „Fertig: …“ steht dort als
 * `arbeit:<id>:fertig`, „Bau rot“ als roter Bau.
 */
export function ungeleseneMeldungen(
	eintraege: readonly AnstehendEintrag[],
	ereignisse: readonly VerlaufEreignis[]
): {
	id: string;
	art: string;
	titel: string;
	zeit: string;
	anzahl: number;
	behoben: boolean;
	gelesen: boolean;
	text: string | null;
}[] {
	const kennungen = new Set(ereignisse.map((e) => e.kennung));
	const roterBau = ereignisse.some((e) => e.quelle === 'bau' && e.art === 'problem');
	const schonDa = (e: AnstehendEintrag): boolean => {
		const schluessel = e.meldung?.schluessel ?? '';
		const arbeit = /\/fertig:(.+)$/.exec(schluessel)?.[1];
		if (arbeit !== undefined) return kennungen.has(`arbeit:${arbeit}:fertig`);
		return roterBau && schluessel.endsWith('/bau-rot');
	};
	return eintraege
		.filter((e) => e.art === 'meldung' && !schonDa(e))
		.map((e) => ({
			id: e.id,
			art: e.meldung?.art ?? 'info',
			titel: e.titel,
			zeit: e.meldung?.zuletzt ?? e.erstellt,
			anzahl: e.meldung?.anzahl ?? 1,
			behoben: false,
			gelesen: false,
			text: e.meldung?.text ?? null
		}));
}

/**
 * Die Ereignisse der Zeitleiste ohne die ungelesenen Meldungen: Die stehen
 * eigens darüber mit dem Knopf „gelesen“ (kein Inhalt zweimal).
 */
export function zeitleisteEreignisse(
	ereignisse: readonly VerlaufEreignis[],
	ungelesen: readonly { id: string }[]
): VerlaufEreignis[] {
	const weg = new Set(ungelesen.map((m) => `meldung:${m.id}`));
	return ereignisse.filter((e) => !weg.has(e.kennung));
}
