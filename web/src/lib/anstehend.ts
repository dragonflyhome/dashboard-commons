/**
 * „Anstehend“ (Etappe A1): laden, umstufen, gruppieren, Filter in der Adresse.
 *
 * `/api/anstehend` ist geschützt wie `/api/aufgaben` (Windows-Anmeldung);
 * 401 ist hier kein Fehler, sondern der Stand „ohne Anmeldung“ mit dem Satz
 * für den Inhaber. Die Seite holt die ganze Liste einmal und filtert im Browser:
 * Die Zähler gelten ohnehin für alle Arten, und die Chips schalten so ohne
 * neue Anfrage um. Klick-Links von Aufgaben stehen nie in der Liste.
 *
 * Etappe A3: Meldungen stehen als eigene Art in der Liste (offen heißt
 * ungelesen); `gelesen` klickt sie weg, `bandMeldungen` wählt die fürs Band.
 *
 * Etappe 3a+ (Grenze „Zustand im Server, Auswertung im Frontend“): Der
 * Server liefert alle offenen Einträge mit Fakten (Stufe, Frist-Stufe,
 * wartet_auf_dich, Schwerpunkt mit reihenfolge) in neutraler Ordnung
 * (Projekt, Art, Kennung), ohne Zahlen. Hier und nur hier ordnet, wählt,
 * gruppiert und zählt die Oberfläche: `auswerten` für „Anstehend“, die
 * Startseite und die Zahl am Reiter, `standAnsicht` für das Unter-Dashboard.
 * Seiten und Komponenten sortieren Anstehend-Daten nicht selbst (Wächter-Test
 * in anstehend.grenze.spec.ts).
 *
 * Projektgruppen (Entscheidung projektgruppe): Die Einträge
 * bleiben beim Projekt; angezeigt wird der Gruppentitel davor (`gruppe` vom
 * Server, `meldungsOrt`). „Jetzt für dich“ zählt eine Gruppe einmal
 * (`jetztKarten`), der Filter `?gruppe=` zeigt alle ihre Teile.
 */
import { API, readError } from './api/client';
import { mitGruppe } from './gruppen.ts';
import { BROWSER_ADRESSE, SATZ_FREMDES_KONTO } from './anmeldung.ts';
import type {
	Aktualisierung,
	AmpelGrund,
	AnstehendEintrag,
	AnstehendListe,
	ArchivFrist,
	EinstufungAntwort,
	EintragsArt,
	FristStufe,
	FristZeile,
	GesamtStand,
	Meldung,
	MeldungGelesen,
	MeldungsArt,
	MeldungsZahlen,
	ProjektStand,
	Schritt,
	StandZahlen,
	Stufe
} from '$vertrag';

export type AnstehendStand = 'laedt' | 'da' | 'ohne-anmeldung' | 'fremdes-konto' | 'fehler';

export type AnstehendLadung =
	| { stand: 'da'; liste: AnstehendListe; fehler: null }
	| { stand: 'ohne-anmeldung' | 'fremdes-konto' | 'fehler'; liste: null; fehler: string };

export const SATZ_OHNE_ANMELDUNG = `Anstehendes siehst du in Microsoft Edge unter ${BROWSER_ADRESSE}`;
export const SATZ_NICHT_MEHR_DA = 'Dieser Eintrag ist erledigt oder nicht mehr da.';
/** Etappe B2: Notizen (open[]) kommen nur mit Anmeldung. */
export const SATZ_NOTIZEN_OHNE_ANMELDUNG = `Anmelden, um sie zu sehen: in Microsoft Edge unter ${BROWSER_ADRESSE}.`;

/** Holt „Anstehend“ (wahlweise nur für ein Projekt). Wirft nie. */
export async function ladeAnstehend(
	fetchFn: typeof fetch = fetch,
	projekt: string | null = null
): Promise<AnstehendLadung> {
	const suche = projekt === null ? '' : `?projekt=${encodeURIComponent(projekt)}`;
	try {
		const res = await fetchFn(`${API}/anstehend${suche}`, {
			credentials: 'same-origin',
			headers: { accept: 'application/json' }
		});
		if (res.status === 401)
			return { stand: 'ohne-anmeldung', liste: null, fehler: SATZ_OHNE_ANMELDUNG };
		if (!res.ok) {
			const e = await readError(res);
			if (e.art === 'fremdes-konto')
				return { stand: 'fremdes-konto', liste: null, fehler: SATZ_FREMDES_KONTO };
			return { stand: 'fehler', liste: null, fehler: e.message };
		}
		return { stand: 'da', liste: (await res.json()) as AnstehendListe, fehler: null };
	} catch {
		return { stand: 'fehler', liste: null, fehler: 'Anstehendes lässt sich gerade nicht laden.' };
	}
}

/** Der Inhaber stuft um (`null`: zurück zum Vorschlag des Chats). Wirft `ApiError`. */
export async function einstufen(
	e: Pick<AnstehendEintrag, 'projekt' | 'art' | 'id' | 'gestellt'>,
	stufe: Stufe | null,
	fetchFn: typeof fetch = fetch
): Promise<EinstufungAntwort> {
	const koerper: Record<string, unknown> = { projekt: e.projekt, art: e.art, id: e.id, stufe };
	if (e.art === 'frage' && e.gestellt !== null) koerper.gestellt = e.gestellt;
	const res = await fetchFn(`${API}/einstufung`, {
		method: 'POST',
		credentials: 'same-origin',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify(koerper)
	});
	if (!res.ok) throw await readError(res);
	return (await res.json()) as EinstufungAntwort;
}

/** Der Inhaber klickt eine Meldung weg (nur im Browser; Etappe A3). Wirft `ApiError`. */
export async function meldungGelesen(
	id: string,
	fetchFn: typeof fetch = fetch
): Promise<MeldungGelesen> {
	const res = await fetchFn(`${API}/meldungen/${encodeURIComponent(id)}/gelesen`, {
		method: 'POST',
		credentials: 'same-origin',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: '{}'
	});
	if (!res.ok) throw await readError(res);
	return (await res.json()) as MeldungGelesen;
}

/**
 * Alle Meldungen außerhalb des Archivs (`/api/meldungen`, geschützt) für den
 * Teil „behoben“ der Glocke. Ohne Anmeldung, bei Fehler oder ohne Netz: leer
 * (die Glocke zeigt dann nur, was „Anstehend“ hat). Wirft nie.
 */
export async function ladeMeldungen(fetchFn: typeof fetch = fetch): Promise<Meldung[]> {
	try {
		const res = await fetchFn(`${API}/meldungen`, {
			credentials: 'same-origin',
			headers: { accept: 'application/json' }
		});
		if (!res.ok) {
			await res.body?.cancel().catch(() => undefined);
			return [];
		}
		const l = (await res.json()) as { meldungen?: Meldung[] };
		return Array.isArray(l.meldungen) ? l.meldungen : [];
	} catch {
		return [];
	}
}

export const STUFEN: readonly Stufe[] = ['jetzt', 'bald', 'irgendwann'];
export const STUFE_WORT: Record<Stufe, string> = {
	jetzt: 'Jetzt',
	bald: 'Bald',
	irgendwann: 'Irgendwann'
};
export const ARTEN: readonly EintragsArt[] = ['aufgabe', 'frage', 'arbeit', 'meldung'];
export const ART_WORT: Record<EintragsArt, string> = {
	aufgabe: 'Aufgabe',
	frage: 'Frage',
	arbeit: 'Arbeit',
	meldung: 'Meldung'
};
/** Mehrzahl für die Chips. */
export const ART_MEHRZAHL: Record<EintragsArt, string> = {
	aufgabe: 'Aufgaben',
	frage: 'Fragen',
	arbeit: 'Arbeiten',
	meldung: 'Meldungen'
};
/** Zeichen vor dem Wort (nur Schmuck, das Wort steht immer daneben). */
export const ART_ZEICHEN: Record<EintragsArt, string> = {
	aufgabe: '☐',
	frage: '?',
	arbeit: '▸',
	meldung: '•'
};
/** Art einer Meldung in Worten (Etappe A3). */
export const MELDUNG_WORT: Record<MeldungsArt, string> = {
	fehler: 'Fehler',
	frist: 'Frist',
	fertig: 'Fertig',
	info: 'Info'
};
/** Zustand einer Aufgabe, auf die du mit eigenen Worten geantwortet hast (wie im Server). */
export const ZUSTAND_BEIM_CHAT = 'beim-chat';

const ZUSTAND_WORT: Record<string, string> = {
	laeuft: 'läuft',
	wartet: 'wartet',
	'als-naechstes': 'als Nächstes'
};

/**
 * Zustand einer Arbeit in Worten; bei einer Aufgabe nur „beim Chat“ (eigene
 * Worte geschickt, Nachtrag zu „Etwas anderes …“), sonst bei Aufgaben und Fragen `null`.
 */
export function zustandWort(e: AnstehendEintrag): string | null {
	if (e.art === 'aufgabe') return e.zustand === ZUSTAND_BEIM_CHAT ? 'beim Chat' : null;
	return e.art === 'arbeit' ? (ZUSTAND_WORT[e.zustand] ?? e.zustand) : null;
}

/** Wie viele Schritte einer Arbeit fertig sind (Zweig teilschritte, T4); gezählt wird hier. */
export function fertigeSchritte(schritte: readonly Schritt[]): number {
	return schritte.filter((s) => s.stand === 'fertig').length;
}

/** Wartet der Eintrag auf den Inhaber? Ein Regel-Ergebnis des Servers (Fakt je Eintrag). */
export function wartetAufDich(e: AnstehendEintrag): boolean {
	return e.wartet_auf_dich;
}

/** `2026-10-30` → `30.10.` */
export function kurzDatum(tag: string): string {
	const [, m, t] = tag.split('-');
	return `${t ?? ''}.${m ?? ''}.`;
}

/** Frist in Worten aus den Tagen bis dahin (vom Server gerechnet). */
export function fristWorte(tage: number | null): string | null {
	if (tage === null) return null;
	if (tage < 0) return 'überfällig';
	if (tage === 0) return 'heute';
	if (tage === 1) return 'morgen';
	return `in ${String(tage)} Tagen`;
}

/**
 * Ob eine Frist hervorgehoben wird: überfällig oder heute. Aus der
 * Frist-Stufe des Servers (Etappe 3a-4), derselben Quelle, aus der er die
 * Stufe des Eintrags hebt; Rand (Stufe Jetzt) und Hervorhebung passen so
 * immer zusammen.
 */
export function fristDringend(stufe: FristStufe | null): boolean {
	return stufe === 'ueberfaellig' || stufe === 'heute';
}

/** Kurze Vermerke zu einem Eintrag. */
export function vermerke(e: AnstehendEintrag): string[] {
	const v: string[] = [];
	if (e.meldung !== null) {
		if (e.meldung.system) v.push('PC');
		if (e.meldung.anzahl > 1) v.push(`${String(e.meldung.anzahl)}-mal`);
		return v;
	}
	if (e.vermerk) v.push(e.vermerk);
	else if (e.art === 'arbeit' && e.wartet_auf === 'inhaber') v.push('wartet auf dich');
	if (e.art === 'frage') v.push('wartet auf deine Antwort');
	if (e.stufe.grund === 'frist') v.push('Frist');
	if (e.stufe.grund === 'inhaber') v.push('von dir eingestuft');
	return v;
}

/** Anker auf der Projektseite: `<art>-<id>`. */
export function anker(e: Pick<AnstehendEintrag, 'art' | 'id'>): string {
	return `${e.art}-${e.id}`;
}

/** Ansicht und Filter, wie sie in der Adresse stehen. */
export interface AnstehendFilter {
	ansicht: 'prioritaet' | 'schwerpunkt';
	/** Leer: alle Arten. */
	arten: EintragsArt[];
	nur: boolean;
	/** Nur die Teile dieser Projektgruppe (Slug); fehlt: alle. */
	gruppe?: string;
}

/** Liest `?ansicht=&art=&nur=&gruppe=`; Unbekanntes fällt still auf den Standard zurück. */
export function filterAus(suche: Pick<URLSearchParams, 'get'>): AnstehendFilter {
	const ansicht = suche.get('ansicht') === 'schwerpunkt' ? 'schwerpunkt' : 'prioritaet';
	const gewaehlt = new Set(
		(suche.get('art') ?? '')
			.split(',')
			.map((a) => a.trim())
			.filter((a): a is EintragsArt => (ARTEN as readonly string[]).includes(a))
	);
	const arten = ARTEN.filter((a) => gewaehlt.has(a));
	const nur = suche.get('nur') === '1';
	const f: AnstehendFilter = { ansicht, arten: arten.length === ARTEN.length ? [] : arten, nur };
	const gruppe = suche.get('gruppe')?.trim() ?? '';
	if (gruppe !== '') f.gruppe = gruppe;
	return f;
}

/** Die Suche zur Adresse, nur mit dem, was vom Standard abweicht (`''` oder `?…`). */
export function sucheAus(f: AnstehendFilter): string {
	const teile: string[] = [];
	if (f.ansicht === 'schwerpunkt') teile.push('ansicht=schwerpunkt');
	if (f.arten.length > 0 && f.arten.length < ARTEN.length) teile.push(`art=${f.arten.join(',')}`);
	if (f.nur) teile.push('nur=1');
	if (f.gruppe !== undefined) teile.push(`gruppe=${encodeURIComponent(f.gruppe)}`);
	return teile.length === 0 ? '' : `?${teile.join('&')}`;
}

/** Eine Zahl-Kachel oben in „Anstehend“: ein Link auf ihr Ziel. */
export interface ZahlKachel {
	zahl: number;
	wort: string;
	/** `jetzt`, `dich` oder leer: die Farbe des Rands. */
	klasse: string;
	/** Hinter `/anstehend`: die Stufe in der Ansicht „nach Priorität“ (Suche und Anker), bei „wartet auf dich“ der Filter. */
	suche: string;
}

/**
 * Die Kacheln Jetzt, Bald, Irgendwann und „wartet auf
 * dich“ waren nicht klickbar. Jede führt jetzt zu dem, was sie zählt: die drei
 * Stufen zu ihrer Gruppe (Ansicht „nach Priorität“, Anker `#stufe-<stufe>`,
 * der übrige Filter bleibt), „wartet auf dich“ zum Filter „nur was auf mich
 * wartet“.
 *
 * Jede Kachel zählt genau die Liste, die ihr Link zeigt (mit
 * dem Filter dieses Links), nicht die ungefilterte Liste. Bewusst nicht der
 * andere Weg (Link setzt den Filter zurück): Die Kacheln stehen direkt über
 * dem Filter und den Gruppen; mit gesetztem Filter (etwa „nur Fragen“)
 * sollen Kachel und Gruppenüberschrift dieselbe Zahl tragen, und ein Klick
 * soll die Auswahl nicht wegwerfen. Die ungefilterte Zahl steht weiter am
 * Reiter und im Tab-Titel. `eintraege` kommen ausgewertet (`auswerten`).
 */
export function zahlKacheln(
	eintraege: readonly AnstehendEintrag[],
	f: AnstehendFilter
): ZahlKachel[] {
	const stufen = { ...f, ansicht: 'prioritaet' as const };
	const sichtbar = gefiltert([...eintraege], stufen);
	const stufe = (s: Stufe, klasse = ''): ZahlKachel => ({
		zahl: sichtbar.filter((e) => e.stufe.wirksam === s).length,
		wort: STUFE_WORT[s],
		klasse,
		suche: `${sucheAus(stufen)}#stufe-${s}`
	});
	const dich = { ...f, nur: true };
	return [
		stufe('jetzt', 'jetzt'),
		stufe('bald'),
		stufe('irgendwann'),
		{
			zahl: gefiltert([...eintraege], dich).length,
			wort: 'wartet auf dich',
			klasse: 'dich',
			suche: sucheAus(dich)
		}
	];
}

/** Titel der Projektgruppe `slug` aus den Einträgen (für den Filter-Chip); unbekannt: der Slug. */
export function gruppenTitel(eintraege: readonly AnstehendEintrag[], slug: string): string {
	for (const e of eintraege) if (e.gruppe?.slug === slug) return e.gruppe.titel;
	return slug;
}

/** Schaltet eine Art im Filter um (leer heißt: alle). */
export function artUmschalten(f: AnstehendFilter, art: EintragsArt): AnstehendFilter {
	const an = new Set(f.arten);
	if (an.has(art)) an.delete(art);
	else an.add(art);
	const arten = ARTEN.filter((a) => an.has(a));
	return { ...f, arten: arten.length === ARTEN.length ? [] : arten };
}

export function gefiltert(eintraege: AnstehendEintrag[], f: AnstehendFilter): AnstehendEintrag[] {
	return eintraege.filter(
		(e) =>
			(f.arten.length === 0 || f.arten.includes(e.art)) &&
			(!f.nur || wartetAufDich(e)) &&
			(f.gruppe === undefined || e.gruppe?.slug === f.gruppe)
	);
}

/** Nach Frist (ohne Frist zuletzt); sonst bleibt die Reihenfolge des Servers. */
function nachFrist(a: AnstehendEintrag, b: AnstehendEintrag): number {
	if (a.frist === b.frist) return 0;
	if (a.frist === null) return 1;
	if (b.frist === null) return -1;
	return a.frist < b.frist ? -1 : 1;
}

function nachStufe(a: AnstehendEintrag, b: AnstehendEintrag): number {
	return STUFEN.indexOf(a.stufe.wirksam) - STUFEN.indexOf(b.stufe.wirksam) || nachFrist(a, b);
}

export interface StufenGruppe {
	stufe: Stufe;
	eintraege: AnstehendEintrag[];
}

/** Drei Gruppen Jetzt, Bald, Irgendwann (auch leere), darin nach Frist. */
export function nachPrioritaet(eintraege: AnstehendEintrag[]): StufenGruppe[] {
	return STUFEN.map((stufe) => ({
		stufe,
		eintraege: eintraege.filter((e) => e.stufe.wirksam === stufe).sort(nachFrist)
	}));
}

export interface ProjektGruppe {
	projekt: string;
	titel: string;
	eintraege: AnstehendEintrag[];
}
export interface SchwerpunktGruppe {
	name: string;
	projekte: ProjektGruppe[];
	anzahl: number;
}

/**
 * Je Schwerpunkt eine Gruppe (nur solche mit Einträgen), nach dessen
 * `reihenfolge` (Fakt vom Server, „Sonstiges“ zuletzt), bei Gleichstand nach
 * Name; darunter die Projekte: die bekannten nach Slug, unbekannte (ohne
 * Projekttitel) dahinter in der Reihenfolge ihres ersten Eintrags; darin nach
 * Stufe und Frist. `eintraege` kommen schon geordnet (`auswerten`).
 */
export function nachSchwerpunkt(eintraege: AnstehendEintrag[]): SchwerpunktGruppe[] {
	const namen: { name: string; reihenfolge: number }[] = [];
	for (const e of eintraege) {
		if (!namen.some((n) => n.name === e.schwerpunkt))
			namen.push({ name: e.schwerpunkt, reihenfolge: e.schwerpunkt_reihenfolge });
	}
	namen.sort((a, b) => a.reihenfolge - b.reihenfolge || vergleich(a.name, b.name));
	return namen.map(({ name }) => {
		const darin = eintraege.filter((e) => e.schwerpunkt === name);
		const bekannt = [
			...new Set(darin.filter((e) => e.projekt_titel !== null).map((e) => e.projekt))
		].sort(vergleich);
		const unbekannt = [
			...new Set(darin.filter((e) => e.projekt_titel === null).map((e) => e.projekt))
		].filter((p) => !bekannt.includes(p));
		const projekte: ProjektGruppe[] = [];
		for (const projekt of [...bekannt, ...unbekannt]) {
			const p = darin.filter((e) => e.projekt === projekt).sort(nachStufe);
			const erster = p[0];
			if (erster === undefined) continue;
			projekte.push({
				projekt,
				titel: mitGruppe(erster.projekt_titel ?? projekt, erster.gruppe),
				eintraege: p
			});
		}
		return { name, projekte, anzahl: darin.length };
	});
}

/**
 * Wort vor einer Meldung (Glocke, Zeile in „Anstehend“): „PC“ bei
 * Systemmeldungen, sonst das Projekt, bei einer Projektgruppe mit ihrem Titel davor.
 */
export function meldungsOrt(e: AnstehendEintrag): string {
	return e.meldung?.system === true ? 'PC' : mitGruppe(e.projekt_titel ?? e.projekt, e.gruppe);
}

/** Zeit zum Vergleichen (über Zeitumstellungen hinweg). */
function zeitWert(t: string): number {
	const z = Date.parse(t);
	return Number.isNaN(z) ? 0 : z;
}

/**
 * Das Band oben auf jeder Seite (Etappe A3, [benachrichtigungen]): die
 * ungelesenen Meldungen der Stufe Jetzt, neueste zuerst; `vorn` sind die
 * höchstens `n` angezeigten, `alle` alle (für die Ansage neuer Meldungen).
 * Bald und Irgendwann stehen bewusst nicht im Band (Entscheidung Designteam
 * in A3): Fertig- und Info-Meldungen drängen nicht, sie stehen in
 * „Anstehend“ und im Unter-Dashboard.
 */
export function bandMeldungen(
	eintraege: AnstehendEintrag[],
	n = 3
): { vorn: AnstehendEintrag[]; alle: AnstehendEintrag[] } {
	const alle = eintraege
		.filter((e) => e.art === 'meldung' && e.stufe.wirksam === 'jetzt')
		.sort(
			(a, b) =>
				zeitWert(b.meldung?.zuletzt ?? b.erstellt) - zeitWert(a.meldung?.zuletzt ?? a.erstellt)
		);
	return { vorn: alle.slice(0, n), alle };
}

// ----- Kopfleiste, Fassung 3: Glocke und Zähler „wartet“ in der Seitenleiste -----

/**
 * Rechte- und Zugriffsmeldungen (etwa `dashboard/lesen:docs`, Hotfix 4d: ein
 * Ordner lässt sich nicht lesen). Wunsch: als Hinweis, nicht als
 * Fehler. Erkannt am Schlüssel, den der Server als Fakt mitschickt.
 */
export function istRechteMeldung(schluessel: string | null | undefined): boolean {
	return typeof schluessel === 'string' && /\/(?:lesen|zugriff|rechte?):/.test(schluessel);
}

/** Was die Glocke zeigt: die Zahl und die Liste in zwei Teilen. */
export interface GlockenAnsicht {
	/** Ungelesene Meldungen ohne Hinweise; behobene sind gar nicht darunter. */
	zahl: number;
	/** Diese Meldungen, neueste zuerst. */
	meldungen: AnstehendEintrag[];
	/** Rechte- und Zugriffsmeldungen: als Hinweis, ohne Zahl. */
	hinweise: AnstehendEintrag[];
}

/**
 * Die Glocke (ersetzt das Band): ungelesene Meldungen aus derselben Liste wie
 * „Anstehend“, Reiter und Tab-Titel (eine Auswertungsstelle). Der Server
 * schickt in „Anstehend“ nur Meldungen, die ungelesen und nicht behoben sind;
 * behobene zählen also nie mit (Vorschlag aus dem Auftrag: sie stehen nur in
 * der Liste unter „behoben“, siehe `behobenUngelesen`). Alle Arten zählen
 * (Fehler, Frist, fertig, Info), außer Rechte-Meldungen: die sind Hinweise.
 */
export function glocke(eintraege: readonly AnstehendEintrag[]): GlockenAnsicht {
	const neueste = (a: AnstehendEintrag, b: AnstehendEintrag): number =>
		zeitWert(b.meldung?.zuletzt ?? b.erstellt) - zeitWert(a.meldung?.zuletzt ?? a.erstellt) ||
		vergleich(b.id, a.id);
	const alle = eintraege.filter((e) => e.art === 'meldung').sort(neueste);
	const meldungen = alle.filter((e) => !istRechteMeldung(e.meldung?.schluessel));
	const hinweise = alle.filter((e) => istRechteMeldung(e.meldung?.schluessel));
	return { zahl: meldungen.length, meldungen, hinweise };
}

/**
 * Behobene, aber ungelesene Meldungen (aus `/api/meldungen`) für den Teil
 * „behoben“ in der Liste der Glocke; neueste zuerst. Sie zählen nicht.
 */
export function behobenUngelesen(meldungen: readonly Meldung[]): Meldung[] {
	return meldungen
		.filter((m) => m.behoben !== null && m.gelesen === null && m.archiviert === null)
		.sort((a, b) => zeitWert(b.zuletzt) - zeitWert(a.zuletzt) || vergleich(b.id, a.id));
}

/**
 * Zähler „wartet“ je Projekt für die Seitenleiste: wie viele Einträge auf
 * den Inhaber warten (Fakt `wartet_auf_dich`), aus der ausgewerteten Liste (je
 * Projekt höchstens eine Frage, wie überall).
 */
export function wartetJeProjekt(eintraege: readonly AnstehendEintrag[]): Record<string, number> {
	const z: Record<string, number> = {};
	for (const e of eintraege) {
		if (e.wartet_auf_dich) z[e.projekt] = (z[e.projekt] ?? 0) + 1;
	}
	return z;
}

/** Tab-Titel mit der Zahl der Stufe Jetzt vorn, etwa „(2) Anstehend · Dashboard“. */
export function titelMitZahl(name: string, jetzt: number): string {
	return jetzt > 0 ? `(${String(jetzt)}) ${name} · Dashboard` : `${name} · Dashboard`;
}

/** Ein Projekt in „Jetzt für dich“ auf der Startseite. */
export interface JetztProjekt {
	projekt: string;
	titel: string;
	/** Die aktuelle Frage des Projekts, nur wenn sie unter Jetzt steht (`auswerten` lässt je Projekt nur eine stehen). */
	frage: AnstehendEintrag | null;
	/** Arbeiten unter Jetzt, die auf den Inhaber warten (Aufgaben stehen schon in „Für dich“). */
	jetzt: AnstehendEintrag[];
}

/**
 * „Jetzt für dich“ ([uebersicht] ort): je Projekt die aktuelle Frage und die
 * Arbeiten unter Jetzt, die auf den Inhaber warten (Auflage aus A1: nicht jede
 * laufende Arbeit). Aufgaben fehlen hier, weil der Kasten „Für dich“
 * darüber schon alle offenen Aufgaben mit Haken zeigt; Meldungen stehen im Band. Die Projekte stehen
 * in der Reihenfolge von `auswerten` (nach Stufe, Frist, Alter): wer zuerst
 * etwas hat, steht vorn.
 *
 * Etappe 3b-2 (Abnahme 3b, `entscheidung_startseite`): auch die Frage nur,
 * wenn ihre wirksame Stufe (`stufe.wirksam` vom Server) Jetzt ist. Vorher
 * standen hier Fragen aus Bald und Irgendwann zwischen den Jetzt-Arbeiten;
 * die übrigen Fragen stehen in „Anstehend“.
 */
export function jetztFuerDich(eintraege: AnstehendEintrag[]): JetztProjekt[] {
	const projekte: JetztProjekt[] = [];
	for (const e of eintraege) {
		if (e.art === 'aufgabe' || e.art === 'meldung') continue;
		if (e.stufe.wirksam !== 'jetzt') continue;
		const istFrage = e.art === 'frage';
		if (!istFrage && !wartetAufDich(e)) continue;
		let p = projekte.find((x) => x.projekt === e.projekt);
		if (p === undefined) {
			p = { projekt: e.projekt, titel: e.projekt_titel ?? e.projekt, frage: null, jetzt: [] };
			projekte.push(p);
		}
		if (istFrage) p.frage ??= e;
		else p.jetzt.push(e);
	}
	return projekte;
}

/** Eine Karte in „Jetzt für dich“: ein Projekt oder eine Projektgruppe mit ihren Teilen. */
export interface JetztKarte {
	/** Slug des Projekts, bei einer Gruppe `gruppe:<slug>`. */
	schluessel: string;
	/** Titel des Projekts oder der Gruppe. */
	titel: string;
	/** Gruppe: Slug, sonst `null`. */
	gruppe: string | null;
	/** Die Projekte (bei einer Gruppe die Teile mit etwas für dich, in der Reihenfolge von `jetztFuerDich`). */
	teile: JetztProjekt[];
}

/**
 * „Jetzt für dich“ mit Projektgruppen (Entscheidung projektgruppe):
 * die Projekte aus `jetztFuerDich`; die Teile einer Gruppe stehen in einer
 * Karte an der Stelle des ersten von ihnen. So zählt eine Gruppe bei
 * „Projekte mit etwas für dich“ einmal; die Einträge zählen weiter einzeln.
 */
export function jetztKarten(eintraege: AnstehendEintrag[]): JetztKarte[] {
	const gruppeVon = new Map<string, { slug: string; titel: string }>();
	for (const e of eintraege) if (e.gruppe) gruppeVon.set(e.projekt, e.gruppe);
	const karten: JetztKarte[] = [];
	for (const p of jetztFuerDich(eintraege)) {
		const g = gruppeVon.get(p.projekt);
		if (g === undefined) {
			karten.push({ schluessel: p.projekt, titel: p.titel, gruppe: null, teile: [p] });
			continue;
		}
		const da = karten.find((k) => k.gruppe === g.slug);
		if (da) da.teile.push(p);
		else
			karten.push({ schluessel: `gruppe:${g.slug}`, titel: g.titel, gruppe: g.slug, teile: [p] });
	}
	return karten;
}

// ----- Auswertung (Etappe 3a+): die eine Stelle im Web -----

/** Wie der Server Texte verglich (Rust: Bytes, hier UTF-16; für Kennungen und Daten gleich). */
function vergleich(a: string, b: string): number {
	return a < b ? -1 : a > b ? 1 : 0;
}

const ART_RANG: Record<EintragsArt, number> = { aufgabe: 0, frage: 1, arbeit: 2, meldung: 3 };

/**
 * Die Reihenfolge, in der der Inhaber die Einträge sieht (bis Etappe 3a+ die des
 * Servers): wirksame Stufe, Frist (ohne Frist zuletzt), Alter, dann fest
 * nach Art, Projekt und Kennung.
 */
export function vergleicheEintraege(a: AnstehendEintrag, b: AnstehendEintrag): number {
	return (
		STUFEN.indexOf(a.stufe.wirksam) - STUFEN.indexOf(b.stufe.wirksam) ||
		nachFrist(a, b) ||
		vergleich(a.erstellt, b.erstellt) ||
		ART_RANG[a.art] - ART_RANG[b.art] ||
		vergleich(a.projekt, b.projekt) ||
		vergleich(a.id, b.id)
	);
}

/** Eine neue, geordnete Liste (die Eingabe bleibt, wie sie ist). */
export function ordnen(eintraege: readonly AnstehendEintrag[]): AnstehendEintrag[] {
	return [...eintraege].sort(vergleicheEintraege);
}

/** „noch 1 Frage wartet“, „noch 3 Fragen warten“. */
export function weitereFragenSatz(n: number): string {
	return n === 1 ? 'noch 1 Frage wartet' : `noch ${String(n)} Fragen warten`;
}

/**
 * Eine Frage auf einmal ([klaerungen_a1], Fragen-Warteschlange seit Etappe
 * B2): je Projekt nur die sichtbare Frage; sie trägt den Vermerk zu den
 * übrigen. Alle anderen Einträge bleiben. Welche sichtbar ist, sagt der
 * Server (`warteschlange.platz` 1, `crate::anstehend::fragen_warteschlange`:
 * wirksame Stufe, dann Frist, dann Reihenfolge in fragen.toml); dieselbe Stelle nehmen
 * Frist-Meldungen, die Zahlen ohne Anmeldung und das Backlog. Fehlt der
 * Platz (Beispieldaten von vor B2), gilt die alte Regel: höchste wirksame
 * Stufe, dann die älteste (`gestellt`), dann nach Kennung.
 */
export function auswaehlen(eintraege: readonly AnstehendEintrag[]): AnstehendEintrag[] {
	const erste = new Map<string, AnstehendEintrag>();
	const anzahl = new Map<string, number>();
	const vor = (a: AnstehendEintrag, b: AnstehendEintrag): boolean =>
		(STUFEN.indexOf(a.stufe.wirksam) - STUFEN.indexOf(b.stufe.wirksam) ||
			vergleich(a.erstellt, b.erstellt) ||
			vergleich(a.id, b.id)) < 0;
	for (const f of eintraege) {
		if (f.art !== 'frage') continue;
		anzahl.set(f.projekt, (anzahl.get(f.projekt) ?? 0) + 1);
		// Mit Platz vom Server: nur Platz 1 ist sichtbar (auch wenn ein Filter ihn ausließe).
		if (f.warteschlange !== undefined) {
			if (f.warteschlange.platz === 1) erste.set(f.projekt, f);
			continue;
		}
		const bisher = erste.get(f.projekt);
		if (bisher?.warteschlange !== undefined) continue;
		if (bisher === undefined || vor(f, bisher)) erste.set(f.projekt, f);
	}
	const aus: AnstehendEintrag[] = [];
	for (const e of eintraege) {
		if (e.art !== 'frage') aus.push(e);
		else if (erste.get(e.projekt) === e) {
			const weitere = (e.warteschlange?.offen ?? anzahl.get(e.projekt) ?? 1) - 1;
			aus.push(weitere > 0 ? { ...e, vermerk: weitereFragenSatz(weitere) } : e);
		}
	}
	return aus;
}

/** Die Zahlen für Reiter, Tab-Titel und die Kacheln in „Anstehend“. */
export interface AnstehendZaehler {
	jetzt: number;
	bald: number;
	irgendwann: number;
	/** Nach dem Fakt `wartet_auf_dich`; je Projekt zählt höchstens eine Frage (nach `auswaehlen`). */
	wartet_auf_dich: number;
}

export function zaehlen(eintraege: readonly AnstehendEintrag[]): AnstehendZaehler {
	const z: AnstehendZaehler = { jetzt: 0, bald: 0, irgendwann: 0, wartet_auf_dich: 0 };
	for (const e of eintraege) {
		z[e.stufe.wirksam] += 1;
		if (e.wartet_auf_dich) z.wartet_auf_dich += 1;
	}
	return z;
}

/** „Anstehend“, wie die Seiten es zeigen: ausgewählt, geordnet, gezählt. */
export interface AnstehendAnsicht extends Omit<AnstehendListe, 'eintraege'> {
	/** Ausgewählt (eine Frage je Projekt) und geordnet. */
	eintraege: AnstehendEintrag[];
	zaehler: AnstehendZaehler;
}

/** Aus der Antwort des Servers die Ansicht (die eine Stelle für Startseite, Anstehend, Reiter). */
export function auswerten(l: AnstehendListe): AnstehendAnsicht {
	const eintraege = ordnen(auswaehlen(l.eintraege));
	return { ...l, eintraege, zaehler: zaehlen(eintraege) };
}

/** Eine Zahl der Startseite und was sie zählt. */
export interface StartZahl {
	zahl: number;
	/** Was gezählt ist, etwa „jetzt dran“. */
	wort: string;
	/** Wo dieselbe Zahl sonst steht, etwa „an ‚Anstehend‘ und im Tab“. */
	wo: string;
}

/**
 * Vier Zahlen ohne Erklärung (Reiter, Leiste, „Für dich“,
 * Glocke) passten scheinbar nicht zusammen. Sie zählen Verschiedenes, alle
 * aus derselben ausgewerteten Liste; die Startseite nennt sie in einer Zeile
 * mit dem, was sie zählen und wo sie sonst stehen:
 *
 * - jetzt dran: Stufe Jetzt, alle Arten (auch Meldungen); Zahl an
 *   „Anstehend“ und im Tab-Titel;
 * - warten auf dich: alle Stufen, nur was den Inhaber braucht (Fragen,
 *   Aufgaben, Arbeiten mit `wartet_auf = inhaber`); die Leiste zeigt sie je Projekt;
 * - offene Aufgaben: davon die Aufgaben (Kasten „Für dich“);
 * - neue Meldungen: ungelesen, nicht behoben (Glocke).
 */
export function startZahlen(l: Pick<AnstehendAnsicht, 'eintraege' | 'zaehler'>): StartZahl[] {
	const aufgaben = l.eintraege.filter((e) => e.art === 'aufgabe' && e.wartet_auf_dich).length;
	const meldungen = glocke(l.eintraege).zahl;
	return [
		// Ausdrücklich „alle Arten“, denn „Jetzt für dich“ darunter zeigt nur einen Teil.
		{ zahl: l.zaehler.jetzt, wort: 'jetzt dran, alle Arten', wo: 'an „Anstehend“ und im Tab' },
		{
			zahl: l.zaehler.wartet_auf_dich,
			wort: l.zaehler.wartet_auf_dich === 1 ? 'wartet auf dich' : 'warten auf dich',
			wo: 'alle Stufen, in der Leiste je Projekt'
		},
		{
			zahl: aufgaben,
			wort: aufgaben === 1 ? 'offene Aufgabe' : 'offene Aufgaben',
			wo: 'Kasten „Für dich“'
		},
		{
			zahl: meldungen,
			wort: meldungen === 1 ? 'neue Meldung' : 'neue Meldungen',
			wo: 'Glocke'
		}
	];
}

// ----- Unter-Dashboard -----

/** So viele offene Aufgaben zeigt das Unter-Dashboard, dann „und N weitere“. */
export const STAND_AUFGABEN = 3;
/** So viele Arbeiten je Gruppe. */
export const STAND_ARBEITEN = 2;
/** So viele Aktualisierungen. */
export const STAND_AKTUALISIERUNGEN = 5;
/** Satz der Ampel ohne Datenspeicher. */
export const SATZ_AMPEL_UNBEKANNT =
	'Unbekannt: Der Datenspeicher ist gerade nicht offen, darum fehlen Aufgaben, Fristen und Meldungen.';

/**
 * Satz der Karte „Stand“ eines archivierten Projekts (Entscheidung
 * archiv-status: ruhig, keine Farbe, keine Ampel; früher
 * zeigte das archivierte Projekt eine rote Karte und „3 neu“).
 */
export const SATZ_ARCHIV_RUHIG = 'Dieses Projekt ruht. Nichts davon meldet sich.';

/**
 * Fristen im Archiv für die Startseite (Auflage der Architektur zu B3): nur
 * die mit Datum (Regel der sieben Tage). Zeilen ohne Datum („hängt an einem
 * Ereignis“, „lassen sich nicht lesen“) haben kein Ende und stünden sonst
 * womöglich monatelang täglich da; sie stehen nur in „Anstehend“.
 */
export function archivFristenStart(
	liste: readonly ArchivFrist[] | null | undefined
): ArchivFrist[] {
	return (liste ?? []).filter((f) => f.frist !== null);
}

/** Status `archiviert` im Kopf (alte oder neue Form). */
export function istArchiviert(status: string): boolean {
	return status.trim().toLowerCase() === 'archiviert';
}

/** Zeit zum Vergleichen wie im Server; Unlesbares gilt als ganz alt. */
function zeitOderAlt(t: string): number {
	const z = Date.parse(t);
	return Number.isNaN(z) ? Number.NEGATIVE_INFINITY : z;
}

/** Neueste zuerst (bei gleicher Zeit nach Kennung absteigend), höchstens `n`. */
export function aktualisierungenWaehlen(
	liste: readonly Aktualisierung[],
	n = STAND_AKTUALISIERUNGEN
): Aktualisierung[] {
	return [...liste]
		.sort((a, b) => zeitOderAlt(b.zeit) - zeitOderAlt(a.zeit) || vergleich(b.id, a.id))
		.slice(0, n);
}

/** Wie viele Meldungen das Projekt hat und wie viele ungelesen sind. */
export function meldungsZahlen(aktualisierungen: readonly Aktualisierung[]): MeldungsZahlen {
	const m = aktualisierungen.filter((a) => a.quelle === 'meldung');
	return { neu: m.filter((a) => !a.gelesen).length, gesamt: m.length };
}

/** Die früheste Frist; bei gleicher Frist die erste in der Liste (wie früher `min_by`). */
function fruehesteFrist(eintraege: readonly AnstehendEintrag[]): AnstehendEintrag | null {
	let beste: AnstehendEintrag | null = null;
	for (const e of eintraege) {
		if (e.frist === null) continue;
		if (beste === null || (beste.frist !== null && e.frist < beste.frist)) beste = e;
	}
	return beste;
}

/** Etappe B3: die früheste Frist aus fristen.toml (mit Datum); bei gleichem Datum die erste. */
function fruehesteDateiFrist(zeilen: readonly FristZeile[]): FristZeile | null {
	let beste: FristZeile | null = null;
	for (const z of zeilen) {
		if (z.frist === null) continue;
		if (beste?.frist == null || z.frist < beste.frist) beste = z;
	}
	return beste;
}

/**
 * Dieselben Zahlen, die der Server ohne Anmeldung als Fakt schickt
 * (`zahlen`), aus den Einträgen und Aktualisierungen mit Anmeldung. Ein
 * vitest-Fall hält beide gleich. Etappe B3: die nächste Frist auch aus den
 * Fristen aus fristen.toml (`stand.fristen`), wenn sie früher ist (bei
 * gleichem Datum bleibt der Eintrag); nie aus zeitstrahl.toml. Etappe B8:
 * Fertige Arbeiten stehen nicht in den Einträgen; ihre Zahl kommt als Fakt
 * (`stand.fertig_7_tage`).
 */
export function standZahlen(
	eintraege: readonly AnstehendEintrag[],
	aktualisierungen: readonly Aktualisierung[],
	dateiFristen: readonly FristZeile[] = [],
	fertig7Tage = 0
): StandZahlen {
	const auswahl = ordnen(auswaehlen(eintraege));
	const arbeiten = (z: string): number =>
		eintraege.filter((e) => e.art === 'arbeit' && e.zustand === z).length;
	const eintrag = fruehesteFrist(auswahl);
	const datei = fruehesteDateiFrist(dateiFristen);
	const naechste =
		datei?.frist != null && (eintrag?.frist == null || datei.frist < eintrag.frist)
			? datei
			: eintrag;
	return {
		wartet_auf_dich: auswahl.filter((e) => e.wartet_auf_dich).length,
		aufgaben: eintraege.filter((e) => e.art === 'aufgabe').length,
		fragen: eintraege.filter((e) => e.art === 'frage').length,
		laeuft: arbeiten('laeuft'),
		wartet: arbeiten('wartet'),
		als_naechstes: arbeiten('als-naechstes'),
		fertig_7_tage: fertig7Tage,
		meldungen: meldungsZahlen(aktualisierungen),
		naechste_frist: naechste?.frist ?? null,
		tage_bis: naechste?.tage_bis ?? null,
		frist_stufe: naechste?.frist_stufe ?? null
	};
}

/** Der Satz zur Ampel (Etappe 3a+: der Server schickt nur den Grund als Kennung). */
export function ampelSatz(
	g: Pick<GesamtStand, 'grund' | 'bezug' | 'bezug_art' | 'tage'>,
	wartet: number
): string {
	const satz: Record<AmpelGrund, () => string> = {
		'fehler-offen': () => (g.bezug === null ? 'Ein Fehler ist offen.' : `Fehler: ${g.bezug}`),
		'frist-vorbei': () => {
			if (g.bezug !== null) return `Die Frist von „${g.bezug}“ ist vorbei.`;
			if (g.bezug_art === 'frage') return 'Die Frist einer Frage ist vorbei.';
			if (g.bezug_art === 'arbeit') return 'Die Frist einer Arbeit ist vorbei.';
			// Etappe B3: eine Frist aus fristen.toml hat keine Art.
			if (g.bezug_art === null) return 'Eine Frist ist vorbei.';
			return 'Die Frist einer Aufgabe ist vorbei.';
		},
		wartet: () =>
			wartet === 1 ? 'Ein Eintrag wartet auf dich.' : `${String(wartet)} Einträge warten auf dich.`,
		'nichts-wartet': () => 'Nichts wartet auf dich.',
		'speicher-fehlt': () => SATZ_AMPEL_UNBEKANNT,
		// Etappe B8: Der Server schickt die Tage mit.
		'wartet-lange': () => {
			const seit = `seit ${String(g.tage ?? 0)} Tagen`;
			return g.bezug === null
				? `Ein Eintrag wartet ${seit} auf dich.`
				: `„${g.bezug}“ wartet ${seit} auf dich.`;
		},
		'ohne-bewegung': () =>
			`Seit ${String(g.tage ?? 0)} Tagen hat sich an keiner offenen Arbeit etwas bewegt.`
	};
	return satz[g.grund]();
}

/** Gesamtstand mit dem Satz der Oberfläche in `grund`; `grund_art` ist die Kennung des Servers. */
export interface GesamtAnsicht extends Omit<GesamtStand, 'grund'> {
	grund: string;
	grund_art: AmpelGrund;
	/** Archiviert: ruhig, ohne Ampel und ohne Farbe (`SATZ_ARCHIV_RUHIG`). */
	ruhig?: boolean;
}
export interface KurzAnsicht {
	wartet_auf_dich: number;
	laeuft: number;
	als_naechstes: number;
	naechste_frist: string | null;
	tage_bis: number | null;
	frist_stufe: FristStufe | null;
	frist_bald_tage: number;
}
/** Höchstens ein paar Einträge und wie viele noch. */
export interface ArbeitsGruppe {
	eintraege: AnstehendEintrag[];
	weitere: number;
}
/** Das Unter-Dashboard, wie die Widgets es zeigen (bis 3a+ rechnete das der Server). */
export interface StandAnsicht {
	slug: string;
	schwerpunkt: string;
	gesamt: GesamtAnsicht;
	kurz: KurzAnsicht;
	frage: { aktuell: AnstehendEintrag | null; weitere: number };
	aufgaben: { offen: AnstehendEintrag[]; anzahl: number; weitere: number; sichtbar: boolean };
	/** Neueste zuerst, höchstens fünf; ohne Anmeldung leer. */
	aktualisierungen: Aktualisierung[];
	meldungen: MeldungsZahlen;
	arbeiten: { laufen: ArbeitsGruppe; wartet: ArbeitsGruppe; als_naechstes: ArbeitsGruppe };
	notizen: string[];
	/** Etappe B2: wie viele Notizen; ohne Anmeldung ist `notizen` leer, die Zahl bleibt. */
	notizen_zahl: number;
	speicher_fehlt: boolean;
	/** Etappe B3: offene Fristen aus fristen.toml mit Datum; ohne Anmeldung leer. */
	fristen: FristZeile[];
}

function gruppe(eintraege: AnstehendEintrag[], hoechstens: number): ArbeitsGruppe {
	return {
		eintraege: eintraege.slice(0, hoechstens),
		weitere: Math.max(0, eintraege.length - hoechstens)
	};
}

/**
 * Das Unter-Dashboard aus dem Stand des Servers. Mit Anmeldung (`eintraege`
 * aus der Liste oder `stand.eintraege`) ordnet, wählt und zählt diese
 * Funktion; ohne Anmeldung nimmt sie die Zahlen des Servers (`stand.zahlen`)
 * und zeigt keine Einträge.
 */
export function standAnsicht(
	stand: ProjektStand,
	eintraege: readonly AnstehendEintrag[] | null = null,
	aktualisierungen: readonly Aktualisierung[] | null = null
): StandAnsicht {
	const roh = eintraege ?? (stand.sichtbar ? stand.eintraege : null);
	const akt = aktualisierungen ?? (stand.sichtbar ? stand.aktualisierungen : null);
	const leer: ArbeitsGruppe = { eintraege: [], weitere: 0 };
	let kurz: KurzAnsicht;
	let frage: StandAnsicht['frage'] = { aktuell: null, weitere: 0 };
	let aufgaben: StandAnsicht['aufgaben'];
	let arbeiten: StandAnsicht['arbeiten'];
	let meldungen: MeldungsZahlen;
	if (roh !== null) {
		const z = standZahlen(roh, akt ?? [], stand.fristen ?? [], stand.fertig_7_tage ?? 0);
		const auswahl = ordnen(auswaehlen(roh));
		const art = (a: EintragsArt): AnstehendEintrag[] => auswahl.filter((e) => e.art === a);
		const zustand = (w: string): AnstehendEintrag[] =>
			auswahl.filter((e) => e.art === 'arbeit' && e.zustand === w);
		const aktuell = art('frage')[0] ?? null;
		frage = { aktuell, weitere: aktuell === null ? 0 : z.fragen - 1 };
		const offen = art('aufgabe');
		aufgaben = {
			offen: offen.slice(0, STAND_AUFGABEN),
			anzahl: offen.length,
			weitere: Math.max(0, offen.length - STAND_AUFGABEN),
			sichtbar: true
		};
		arbeiten = {
			laufen: gruppe(zustand('laeuft'), STAND_ARBEITEN),
			wartet: gruppe(zustand('wartet'), STAND_ARBEITEN),
			als_naechstes: gruppe(zustand('als-naechstes'), STAND_ARBEITEN)
		};
		kurz = { ...kurzAus(z), frist_bald_tage: stand.frist_bald_tage };
		meldungen = akt === null ? (stand.zahlen?.meldungen ?? { neu: 0, gesamt: 0 }) : z.meldungen;
	} else {
		const z: StandZahlen = stand.zahlen ?? {
			wartet_auf_dich: 0,
			aufgaben: 0,
			fragen: 0,
			laeuft: 0,
			wartet: 0,
			als_naechstes: 0,
			fertig_7_tage: 0,
			meldungen: { neu: 0, gesamt: 0 },
			naechste_frist: null,
			tage_bis: null,
			frist_stufe: null
		};
		// Ohne Anmeldung keine Einträge (Grenze aus 7b): nur die Zahlen.
		aufgaben = { offen: [], anzahl: z.aufgaben, weitere: 0, sichtbar: false };
		arbeiten = {
			laufen: { ...leer, weitere: z.laeuft },
			wartet: { ...leer, weitere: z.wartet },
			als_naechstes: { ...leer, weitere: z.als_naechstes }
		};
		kurz = { ...kurzAus(z), frist_bald_tage: stand.frist_bald_tage };
		meldungen = z.meldungen;
	}
	const { grund, ...gesamt } = stand.gesamt;
	return {
		slug: stand.slug,
		schwerpunkt: stand.schwerpunkt,
		gesamt: istArchiviert(stand.gesamt.status)
			? { ...gesamt, ampel: 'unbekannt', grund: SATZ_ARCHIV_RUHIG, grund_art: grund, ruhig: true }
			: { ...gesamt, grund: ampelSatz(stand.gesamt, kurz.wartet_auf_dich), grund_art: grund },
		kurz,
		frage,
		aufgaben,
		aktualisierungen: akt === null ? [] : aktualisierungenWaehlen(akt),
		meldungen,
		arbeiten,
		notizen: stand.notizen,
		notizen_zahl: stand.notizen_zahl ?? stand.notizen.length,
		speicher_fehlt: stand.speicher_fehlt,
		fristen: stand.fristen ?? []
	};
}

function kurzAus(z: StandZahlen): Omit<KurzAnsicht, 'frist_bald_tage'> {
	return {
		wartet_auf_dich: z.wartet_auf_dich,
		laeuft: z.laeuft,
		als_naechstes: z.als_naechstes,
		naechste_frist: z.naechste_frist,
		tage_bis: z.tage_bis,
		frist_stufe: z.frist_stufe
	};
}

/** Einträge mit Frist, die nächste zuerst, bei gleicher Frist nach Titel (Widget „Fristen“). */
export function mitFristNaechsteZuerst(liste: readonly AnstehendEintrag[]): AnstehendEintrag[] {
	return liste
		.filter((e) => e.frist !== null)
		.sort((a, b) => (a.frist ?? '').localeCompare(b.frist ?? '') || a.titel.localeCompare(b.titel));
}

/** Eine Zeile im Widget „Fristen“: aus einem Eintrag oder aus fristen.toml (Etappe B3). */
export interface FristPunkt {
	schluessel: string;
	frist: string;
	tage_bis: number | null;
	titel: string;
	/** Anker auf der Projektseite: `<art>-<id>` oder `frist-<kennung>` (Fristenliste). */
	anker: string;
}

/**
 * Etappe B3: Einträge mit Frist und die Fristen aus fristen.toml mit Datum in
 * einer Liste, die nächste zuerst, bei gleicher Frist nach Titel (wie
 * `mitFristNaechsteZuerst`). Fristen ohne Datum (hängen an einem Ereignis)
 * stehen nur in der Fristenliste.
 */
export function fristPunkte(
	eintraege: readonly AnstehendEintrag[],
	dateiFristen: readonly FristZeile[]
): FristPunkt[] {
	const aus: FristPunkt[] = [];
	for (const e of eintraege) {
		if (e.frist === null) continue;
		const a = anker(e);
		aus.push({ schluessel: a, frist: e.frist, tage_bis: e.tage_bis, titel: e.titel, anker: a });
	}
	for (const z of dateiFristen) {
		if (z.frist === null) continue;
		const a = `frist-${z.id}`;
		aus.push({ schluessel: a, frist: z.frist, tage_bis: z.tage_bis, titel: z.titel, anker: a });
	}
	return aus.sort((a, b) => a.frist.localeCompare(b.frist) || a.titel.localeCompare(b.titel));
}

// ----- Unter-Dashboard: alles, was die Reihe zeigt (Abnahme 3a+, Nachbesserung 1) -----

/** Eine Gruppe in der Karte „Arbeiten“: Wort, Einträge und wie viele noch. */
export interface ArbeitsGruppeZeile {
	wort: string;
	eintraege: AnstehendEintrag[];
	weitere: number;
}

/** Was das Unter-Dashboard zeigt; die Komponente rechnet daran nichts mehr. */
export interface UnterDashboardAnsicht {
	/** `null`: kein Stand (Projekt unbekannt, kein Netz). */
	stand: StandAnsicht | null;
	/** Mit Anmeldung (Einträge mit Titeln sind da). */
	angemeldet: boolean;
	/** Ausgewählt (eine Frage je Projekt) und geordnet; ohne Anmeldung leer. */
	eintraege: AnstehendEintrag[];
	/** Neueste zuerst, höchstens fünf; ohne Anmeldung leer. */
	aktualisierungen: Aktualisierung[];
	/** Ungelesene Meldungen, die nicht unter den fünf stehen (damit jeder Anker ein Ziel hat). */
	weitereMeldungen: AnstehendEintrag[];
	/** Nur ohne Anmeldung: die Zahlen der Meldungen vom Server (dann ohne Titel). */
	meldungsZahlen: MeldungsZahlen | null;
	/** Alle ungelesenen Meldungen des Projekts, nicht nur die angezeigten fünf. */
	ungelesen: number;
	/** Karte „Wartet auf dich“. */
	wartet: {
		zahl: number;
		/** Ohne Anmeldung nur die Zahl der Aufgaben. */
		aufgabenZahl: number;
		aufgaben: AnstehendEintrag[];
		fragen: AnstehendEintrag[];
		arbeiten: AnstehendEintrag[];
	};
	/** Karte „Arbeiten“: nur Gruppen mit Einträgen oder einer Zahl. */
	arbeitsGruppen: ArbeitsGruppeZeile[];
	/** Karte „Fristen“: die Einträge, unter denen die Frist steht. */
	fristKandidaten: AnstehendEintrag[];
	/** Karte „Aktualisierungen“: Titel mit Anmeldung, sonst nur die Zahlen. */
	hatAktualisierungen: boolean;
	/** Ob die Reihe überhaupt etwas zu zeigen hat (Notizen kommen nur mit dem Stand). */
	zeigen: boolean;
}

const ARBEITS_GRUPPEN: readonly (readonly [string, string, keyof StandAnsicht['arbeiten']])[] = [
	['laeuft', 'läuft', 'laufen'],
	['wartet', 'wartet', 'wartet'],
	['als-naechstes', 'als Nächstes', 'als_naechstes']
];

/**
 * Das Unter-Dashboard aus den Antworten des Servers: `/stand` (oder
 * `/stand/voll`), mit Anmeldung dazu die Einträge des Projekts und seine
 * Aktualisierungen aus `/api/anstehend?projekt=` (jeweils alle, roh). Ohne
 * Einträge (`roh` null und `stand.sichtbar` falsch) zeigt es nur die Zahlen
 * des Servers. Hier wird ausgewählt, gezählt und gruppiert; die Komponente
 * zeigt nur an.
 */
export function unterDashboardAnsicht(
	stand: ProjektStand | null,
	roh: readonly AnstehendEintrag[] | null = null,
	akt: readonly Aktualisierung[] | null = null
): UnterDashboardAnsicht {
	const eintraegeRoh = roh ?? (stand?.sichtbar === true ? stand.eintraege : null);
	const aktRoh = akt ?? (stand?.sichtbar === true ? stand.aktualisierungen : null);
	const ansicht = stand === null ? null : standAnsicht(stand, eintraegeRoh, aktRoh);
	const angemeldet = eintraegeRoh !== null;
	const eintraege = eintraegeRoh === null ? [] : ordnen(auswaehlen(eintraegeRoh));
	const aktualisierungen = aktRoh === null ? [] : aktualisierungenWaehlen(aktRoh);
	// Archiviert: ruhig, ohne Zähler „neu“ (Entscheidung archiv-status).
	const ruhig = ansicht?.gesamt.ruhig === true;
	const zahlenRoh = angemeldet ? null : (ansicht?.meldungen ?? null);
	const zahlenFrei = zahlenRoh !== null && ruhig ? { ...zahlenRoh, neu: 0 } : zahlenRoh;
	// Alle ungelesenen des Projekts (aus allen Aktualisierungen), nicht nur die fünf angezeigten.
	const ungelesen = ruhig
		? 0
		: (zahlenFrei?.neu ?? (aktRoh === null ? 0 : meldungsZahlen(aktRoh).neu));
	const gezeigt = new Set(aktualisierungen.filter((a) => a.quelle === 'meldung').map((a) => a.id));
	const weitereMeldungen = eintraege.filter((e) => e.art === 'meldung' && !gezeigt.has(e.id));

	const art = (a: EintragsArt): AnstehendEintrag[] => eintraege.filter((e) => e.art === a);
	// Aufgaben beim Chat (eigene Worte geschickt) warten nicht auf dich.
	const aufgaben = angemeldet
		? art('aufgabe').filter((e) => e.wartet_auf_dich)
		: ansicht?.aufgaben.sichtbar === true
			? ansicht.aufgaben.offen
			: [];
	const fragen = art('frage');
	const arbeitenFuerDich = eintraege.filter((e) => e.art === 'arbeit' && e.wartet_auf_dich);
	const wartetZahl =
		ansicht?.kurz.wartet_auf_dich ?? aufgaben.length + fragen.length + arbeitenFuerDich.length;
	const aufgabenZahl = angemeldet ? aufgaben.length : (ansicht?.aufgaben.anzahl ?? 0);

	const arbeitsGruppen = ARBEITS_GRUPPEN.map(([zustand, wort, feld]): ArbeitsGruppeZeile => {
		if (angemeldet)
			return {
				wort,
				eintraege: eintraege.filter((e) => e.art === 'arbeit' && e.zustand === zustand),
				weitere: 0
			};
		const g = ansicht?.arbeiten[feld];
		return { wort, eintraege: g?.eintraege ?? [], weitere: g?.weitere ?? 0 };
	}).filter((g) => g.eintraege.length > 0 || g.weitere > 0);

	const fristKandidaten = angemeldet
		? eintraege
		: ansicht === null
			? []
			: [
					...ansicht.aufgaben.offen,
					...ansicht.arbeiten.laufen.eintraege,
					...ansicht.arbeiten.wartet.eintraege,
					...ansicht.arbeiten.als_naechstes.eintraege
				];
	const hatAktualisierungen =
		aktualisierungen.length > 0 || weitereMeldungen.length > 0 || (zahlenFrei?.gesamt ?? 0) > 0;
	return {
		stand: ansicht,
		angemeldet,
		eintraege,
		aktualisierungen,
		weitereMeldungen,
		meldungsZahlen: zahlenFrei,
		ungelesen,
		wartet: {
			zahl: wartetZahl,
			aufgabenZahl,
			aufgaben,
			fragen,
			arbeiten: arbeitenFuerDich
		},
		arbeitsGruppen,
		fristKandidaten,
		hatAktualisierungen,
		zeigen: ansicht !== null || wartetZahl > 0 || arbeitsGruppen.length > 0 || hatAktualisierungen
	};
}
