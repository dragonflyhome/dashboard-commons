/**
 * Front-Office-Logik des Verlaufs (Etappe 3a-1, Zeitachse Fassung 1).
 *
 * Der Server liefert Ereignisse unsortiert und ohne Gruppen (`/api/pc/verlauf`,
 * `abgeleitet.ereignisse` des Zeitstrahls). Hier
 * entsteht, wie es für den Inhaber aussieht: sortieren, nach Tag, Woche, Monat und
 * Jahr gruppieren, Routine eines Tages zusammenfassen, nach Quelle filtern,
 * Zeit lesbar machen. Die einzige Bewertung ist die Zuordnung von `art` zu
 * einer Bedeutung (Farbe); alles Fachliche steht schon im Ereignis.
 */
import type { Bedeutung, VerlaufArt, VerlaufEreignis, VerlaufQuelle } from '$vertrag';
import {
	MONATE,
	MONATE_KURZ,
	jahrVon,
	kurzDatum,
	monatTitel,
	monatVon,
	tagPlus,
	tagTitel,
	tagText,
	tagZahl,
	wochenanfang
} from './tage.ts';

// ----- Bedeutung und Wörter -----

/** `art` zu Bedeutung (Farbe). Die einzige Auswertung im Web. */
export function bedeutungAusArt(art: VerlaufArt): Bedeutung {
	switch (art) {
		case 'erledigt':
			return 'ok';
		case 'begonnen':
			return 'laeuft';
		case 'geplant':
			return 'info';
		case 'problem':
			return 'problem';
		case 'abgebrochen':
		case 'routine':
			return 'neutral';
	}
}

/** Wort zur Art für Bildschirmleser. */
export function artWort(art: VerlaufArt): string {
	switch (art) {
		case 'erledigt':
			return 'erledigt';
		case 'begonnen':
			return 'begonnen';
		case 'geplant':
			return 'geplant';
		case 'abgebrochen':
			return 'abgebrochen';
		case 'routine':
			return 'Routine';
		case 'problem':
			return 'Problem';
	}
}

export const QUELLEN_NAME: Record<VerlaufQuelle, string> = {
	hand: 'von Hand',
	'windows-installiert': 'Windows',
	update: 'Updates',
	start: 'Starts',
	meldung: 'Meldungen',
	sicherung: 'Sicherungen',
	vorgang: 'Vorgänge'
};

/** Kurz für die kleine Angabe rechts in der Zeile. */
export const QUELLE_KURZ: Record<VerlaufQuelle, string> = {
	hand: 'Notiz',
	'windows-installiert': 'Windows',
	update: 'Update',
	start: 'Start',
	meldung: 'Meldung',
	sicherung: 'Sicherung',
	vorgang: 'Vorgang'
};

// ----- Einträge -----

/** Ein Ereignis mit dem, was die Oberfläche zum Ordnen braucht. */
export interface Eintrag {
	e: VerlaufEreignis;
	/** Tag des Beginns in Ortszeit, `JJJJ-MM-TT`. */
	tag: string;
	/** Letzter Tag (bei Spannen das Ende des letzten Abschnitts), `JJJJ-MM-TT`. */
	endTag: string;
	/** Zum Sortieren: Millisekunden des Beginns. */
	ms: number;
}

function letzterTagDes(tag: string, genauigkeit: VerlaufEreignis['genauigkeit']): string {
	const j = jahrVon(tag);
	const m = monatVon(tag);
	if (genauigkeit === 'jahr') return `${String(j)}-12-31`;
	if (genauigkeit === 'monat') {
		const naechster =
			m === 12 ? `${String(j + 1)}-01-01` : `${String(j)}-${String(m + 1).padStart(2, '0')}-01`;
		return tagPlus(naechster, -1);
	}
	return tag;
}

/** Tag eines Zeitpunkts: bei `minute` in Ortszeit, sonst das Datum, wie es dasteht. */
function tagDes(zeit: string, genauigkeit: VerlaufEreignis['genauigkeit']): string | null {
	if (genauigkeit === 'minute') {
		const d = new Date(zeit);
		return Number.isNaN(d.getTime()) ? null : tagText(d);
	}
	return /^\d{4}-\d{2}-\d{2}/.test(zeit) ? zeit.slice(0, 10) : null;
}

/** `null`, wenn `zeit` keine lesbare Zeit ist. */
export function eintrag(e: VerlaufEreignis): Eintrag | null {
	const tag = tagDes(e.zeit, e.genauigkeit);
	if (tag === null) return null;
	const bisTag = e.bis === null ? null : tagDes(e.bis, e.genauigkeit);
	const ms = new Date(e.zeit).getTime();
	return {
		e,
		tag,
		endTag: letzterTagDes(bisTag ?? tag, e.genauigkeit),
		ms: Number.isNaN(ms) ? tagZahl(tag) * 86_400_000 : ms
	};
}

/** Neueste zuerst; bei gleicher Zeit nach Titel. */
export function neuesteZuerst(a: Eintrag, b: Eintrag): number {
	return b.ms - a.ms || a.e.titel.localeCompare(b.e.titel, 'de');
}

function aelteste(a: Eintrag, b: Eintrag): number {
	return a.ms - b.ms || a.e.titel.localeCompare(b.e.titel, 'de');
}

function uhrzeit(zeit: string): string {
	const d = new Date(zeit);
	return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

function monatKurz(tag: string, mitJahr: boolean): string {
	const m = MONATE_KURZ[monatVon(tag) - 1] ?? '';
	return mitJahr ? `${m} ${tag.slice(0, 4)}` : m;
}

/**
 * Die Zeit in der Zeile, wenn der Tag schon in der Überschrift steht:
 * Uhrzeit bei `minute`, sonst „ganzer Tag“, der Monat oder das Jahr;
 * Spannen von–bis.
 */
export function zeitText(x: Eintrag): string {
	const { e } = x;
	const spanne = e.bis !== null && x.endTag !== x.tag;
	switch (e.genauigkeit) {
		case 'minute':
			return uhrzeit(e.zeit);
		case 'tag':
			return spanne
				? `${kurzDatum(x.tag)} – ${kurzDatum(x.endTag, jahrVon(x.tag) !== jahrVon(x.endTag))}`
				: 'ganzer Tag';
		case 'monat':
			if (!spanne) return `${MONATE[monatVon(x.tag) - 1] ?? ''} ${x.tag.slice(0, 4)}`;
			return jahrVon(x.tag) === jahrVon(x.endTag)
				? `${monatKurz(x.tag, false)} – ${monatKurz(x.endTag, true)}`
				: `${monatKurz(x.tag, true)} – ${monatKurz(x.endTag, true)}`;
		case 'jahr':
			return spanne ? `${x.tag.slice(0, 4)} – ${x.endTag.slice(0, 4)}` : x.tag.slice(0, 4);
	}
}

/** Die Zeit in der Zeile, wenn keine Überschrift den Tag nennt (Projektseiten). */
export function datumText(x: Eintrag): string {
	const { e } = x;
	if (e.genauigkeit === 'minute') return `${kurzDatum(x.tag, true)}, ${uhrzeit(e.zeit)}`;
	if (e.genauigkeit === 'tag') {
		return e.bis !== null && x.endTag !== x.tag
			? `${kurzDatum(x.tag, jahrVon(x.tag) !== jahrVon(x.endTag))} – ${kurzDatum(x.endTag, true)}`
			: kurzDatum(x.tag, true);
	}
	return zeitText(x);
}

/** Lange Texte stehen zuerst in zwei Zeilen, dann „mehr“. */
export function istLang(text: string | null): boolean {
	return text !== null && (text.length > 140 || text.includes('\n'));
}

// ----- Filter -----

export interface QuellenZahl {
	quelle: VerlaufQuelle;
	name: string;
	anzahl: number;
}

/** Je Quelle die Anzahl, die häufigste zuerst. */
export function quellenZaehlen(ereignisse: readonly VerlaufEreignis[]): QuellenZahl[] {
	const n = new Map<VerlaufQuelle, number>();
	for (const e of ereignisse) n.set(e.quelle, (n.get(e.quelle) ?? 0) + 1);
	return [...n]
		.map(([quelle, anzahl]) => ({ quelle, name: QUELLEN_NAME[quelle], anzahl }))
		.sort((a, b) => b.anzahl - a.anzahl || a.name.localeCompare(b.name, 'de'));
}

export interface Zeitraum {
	/** Erster Tag, `JJJJ-MM-TT`. */
	von: string;
	/** Letzter Tag (eingeschlossen). */
	bis: string;
}

/** Ohne die ausgeblendeten Quellen und, wenn angegeben, nur was den Zeitraum berührt. */
export function filtern(
	ereignisse: readonly VerlaufEreignis[],
	aus: ReadonlySet<VerlaufQuelle>,
	zeitraum: Zeitraum | null = null
): VerlaufEreignis[] {
	return ereignisse.filter((e) => {
		if (aus.has(e.quelle)) return false;
		if (zeitraum === null) return true;
		const x = eintrag(e);
		return x !== null && x.tag <= zeitraum.bis && x.endTag >= zeitraum.von;
	});
}

// ----- Routine zusammenfassen -----

export type Zeile =
	| { art: 'eintrag'; schluessel: string; eintrag: Eintrag }
	| {
			art: 'routine';
			schluessel: string;
			quelle: VerlaufQuelle;
			titel: string;
			/** Neueste zuerst. */
			eintraege: Eintrag[];
	  };

/** „4 Updates installiert“, „3-mal gestartet“ … */
export function routineTitel(quelle: VerlaufQuelle, n: number): string {
	switch (quelle) {
		case 'update':
			return `${String(n)} Updates installiert`;
		case 'start':
			return `${String(n)}-mal gestartet`;
		case 'sicherung':
			return `${String(n)} Sicherungen`;
		case 'meldung':
			return `${String(n)} Hinweise`;
		case 'hand':
		case 'vorgang':
		case 'windows-installiert':
			return `${String(n)} Routine-Einträge`;
	}
}

/**
 * Routine eines Tages je Quelle in eine Zeile („4 Updates installiert ▸“),
 * ab zwei Einträgen. Die Zeile steht dort, wo ihr neuester Eintrag stand.
 * `liste` ist neueste zuerst sortiert.
 */
export function routineZusammenfassen(liste: readonly Eintrag[], schluessel: string): Zeile[] {
	const jeQuelle = new Map<VerlaufQuelle, Eintrag[]>();
	for (const x of liste) {
		if (x.e.art !== 'routine') continue;
		const l = jeQuelle.get(x.e.quelle) ?? [];
		l.push(x);
		jeQuelle.set(x.e.quelle, l);
	}
	const aus: Zeile[] = [];
	const erledigt = new Set<VerlaufQuelle>();
	for (const x of liste) {
		const gruppe = x.e.art === 'routine' ? jeQuelle.get(x.e.quelle) : undefined;
		if (gruppe !== undefined && gruppe.length >= 2) {
			if (erledigt.has(x.e.quelle)) continue;
			erledigt.add(x.e.quelle);
			aus.push({
				art: 'routine',
				schluessel: `${schluessel}:routine:${x.e.quelle}`,
				quelle: x.e.quelle,
				titel: routineTitel(x.e.quelle, gruppe.length),
				eintraege: gruppe
			});
		} else {
			aus.push({ art: 'eintrag', schluessel: x.e.kennung, eintrag: x });
		}
	}
	return aus;
}

// ----- Gruppen (PC, Startseite) -----

export type GruppenStufe = 'kommend' | 'tag' | 'woche' | 'monat' | 'jahr';

export interface Tagesgruppe {
	tag: string;
	titel: string;
	zeilen: Zeile[];
	anzahl: number;
}

export interface Gruppe {
	schluessel: string;
	stufe: GruppenStufe;
	titel: string;
	/** Am Anfang offen (Heute, Gestern, Kommendes). */
	offen: boolean;
	anzahl: number;
	/** Bei `tag` genau ein Tag, sonst die Tage darin, neueste zuerst. */
	tage: Tagesgruppe[];
	/** Einträge, die nur Monat oder Jahr kennen. */
	ohneTag: Zeile[];
	/** Erster und letzter Tag der Gruppe (zum Springen). */
	von: string;
	bis: string;
	heute: boolean;
}

interface Ziel {
	schluessel: string;
	stufe: GruppenStufe;
	titel: string;
	offen: boolean;
	von: string;
	bis: string;
}

/** In welche Gruppe ein Tag gehört (siehe `gruppieren`). */
export function zielFuer(
	tag: string,
	genauigkeit: VerlaufEreignis['genauigkeit'],
	heute: string
): Ziel {
	const gestern = tagPlus(heute, -1);
	const woche = wochenanfang(heute);
	const letzteWoche = tagPlus(woche, -7);
	// Monate: die letzten 12 einschließlich des laufenden.
	const monatsGrenze = (() => {
		let j = jahrVon(heute);
		let m = monatVon(heute) - 11;
		if (m < 1) {
			m += 12;
			j -= 1;
		}
		return `${String(j)}-${String(m).padStart(2, '0')}-01`;
	})();
	// Gruppen überdecken sich nicht: Ein Monat endet vor der letzten Woche,
	// ein Jahr vor dem ersten der zwölf Monate.
	const hoechstens = (a: string, b: string): string => (a < b ? a : b);
	// Reicht ein Monat oder Jahr in die feineren Gruppen hinein (das laufende
	// Jahr, der Monat der letzten Woche), steht er nur noch für seine Einträge
	// ohne genauen Tag da: an seinem Anfang, von = bis.
	const bisHoechstens = (von: string, ende: string, grenze: string): string => {
		const bis = hoechstens(ende, grenze);
		return bis < von ? von : bis;
	};
	const jahrZiel = (): Ziel => {
		const j = tag.slice(0, 4);
		const von = `${j}-01-01`;
		return {
			schluessel: `jahr:${j}`,
			stufe: 'jahr',
			titel: j,
			offen: false,
			von,
			bis: bisHoechstens(von, `${j}-12-31`, tagPlus(monatsGrenze, -1))
		};
	};
	const monatZiel = (): Ziel => {
		const m = tag.slice(0, 7);
		const von = `${m}-01`;
		return {
			schluessel: `monat:${m}`,
			stufe: 'monat',
			titel: monatTitel(tag),
			offen: false,
			von,
			bis: bisHoechstens(von, letzterTagDes(von, 'monat'), tagPlus(letzteWoche, -1))
		};
	};
	// Was erst nach heute beginnt (auch ein Monat oder Jahr), kommt noch.
	if (tag > heute) {
		return {
			schluessel: 'kommend',
			stufe: 'kommend',
			titel: 'Demnächst',
			offen: true,
			von: tagPlus(heute, 1),
			bis: '9999-12-31'
		};
	}
	if (genauigkeit === 'jahr') return jahrZiel();
	if (genauigkeit === 'monat') return tag >= monatsGrenze ? monatZiel() : jahrZiel();
	if (tag === heute)
		return {
			schluessel: `tag:${tag}`,
			stufe: 'tag',
			titel: 'Heute',
			offen: true,
			von: tag,
			bis: tag
		};
	if (tag === gestern)
		return {
			schluessel: `tag:${tag}`,
			stufe: 'tag',
			titel: 'Gestern',
			offen: true,
			von: tag,
			bis: tag
		};
	if (tag >= woche) {
		return {
			schluessel: `tag:${tag}`,
			stufe: 'tag',
			titel: tagTitel(tag, heute),
			offen: false,
			von: tag,
			bis: tag
		};
	}
	if (tag >= letzteWoche) {
		// Ist heute Montag, ist gestern der Sonntag der letzten Woche und steht
		// als „Gestern“ für sich; die Woche endet dann davor (Auflage aus 3a-1).
		const ende = hoechstens(tagPlus(woche, -1), tagPlus(gestern, -1));
		return {
			schluessel: `woche:${letzteWoche}`,
			stufe: 'woche',
			titel: `Letzte Woche (${kurzDatum(letzteWoche)} bis ${kurzDatum(ende)})`,
			offen: false,
			von: letzteWoche,
			bis: ende
		};
	}
	if (tag >= monatsGrenze) return monatZiel();
	return jahrZiel();
}

/**
 * Der Verlauf in Gruppen, neueste oben: Heute und Gestern offen, die übrige
 * laufende Woche als Tageszeilen, die Woche davor als eine Zeile, die
 * letzten zwölf Monate als Monatszeilen, Älteres als Jahre. In jeder Gruppe
 * die Tage neueste zuerst, Routine je Tag zusammengefasst; „Demnächst“
 * aufsteigend (Tage und Einträge ohne genauen Tag). Jahre stehen unter allen
 * Monaten, auch das laufende Jahr (Auflage aus 3a-1). Heute steht immer da
 * (auch leer), damit Pos1 und ein Sprung auf heute ein Ziel haben; nur diese
 * Gruppe trägt `heute`. Ist gar nichts da, zeigt der Verlauf statt der
 * Gruppen nur den Leer-Satz, und Pos1 bleibt ohne Ziel.
 */
export function gruppieren(ereignisse: readonly VerlaufEreignis[], jetzt: Date): Gruppe[] {
	const heute = tagText(jetzt);
	const gruppen = new Map<
		string,
		{ ziel: Ziel; jeTag: Map<string, Eintrag[]>; ohneTag: Eintrag[] }
	>();
	const holen = (ziel: Ziel): { ziel: Ziel; jeTag: Map<string, Eintrag[]>; ohneTag: Eintrag[] } => {
		let g = gruppen.get(ziel.schluessel);
		if (g === undefined) {
			g = { ziel, jeTag: new Map(), ohneTag: [] };
			gruppen.set(ziel.schluessel, g);
		}
		return g;
	};
	const heuteZiel = zielFuer(heute, 'tag', heute);
	holen(heuteZiel);
	for (const e of ereignisse) {
		const x = eintrag(e);
		if (x === null) continue;
		const g = holen(zielFuer(x.tag, e.genauigkeit, heute));
		if (e.genauigkeit === 'monat' || e.genauigkeit === 'jahr') {
			g.ohneTag.push(x);
		} else {
			const l = g.jeTag.get(x.tag) ?? [];
			l.push(x);
			g.jeTag.set(x.tag, l);
		}
	}
	const aus: Gruppe[] = [];
	for (const { ziel, jeTag, ohneTag } of gruppen.values()) {
		const kommend = ziel.stufe === 'kommend';
		const tage: Tagesgruppe[] = [...jeTag]
			.sort(([a], [b]) => (kommend ? a.localeCompare(b) : b.localeCompare(a)))
			.map(([tag, liste]) => {
				const sortiert = [...liste].sort(kommend ? aelteste : neuesteZuerst);
				return {
					tag,
					titel:
						tag === heute ? 'Heute' : tag === tagPlus(heute, -1) ? 'Gestern' : tagTitel(tag, heute),
					zeilen: routineZusammenfassen(sortiert, `tag:${tag}`),
					anzahl: liste.length
				};
			});
		const ohne = [...ohneTag].sort(kommend ? aelteste : neuesteZuerst);
		aus.push({
			schluessel: ziel.schluessel,
			stufe: ziel.stufe,
			titel: ziel.titel,
			offen: ziel.offen,
			anzahl: tage.reduce((n, t) => n + t.anzahl, 0) + ohne.length,
			tage,
			ohneTag: ohne.map((x) => ({ art: 'eintrag', schluessel: x.e.kennung, eintrag: x })),
			von: ziel.von,
			bis: ziel.bis,
			// Nur die Gruppe Heute: Am Monatsersten oder am 1.1. beginnen auch der
			// laufende Monat und das laufende Jahr heute (Auflage aus 3a-1).
			heute: ziel.schluessel === heuteZiel.schluessel
		});
	}
	// Der laufende Monat reicht bis zum Monatsende, steht aber unter Heute:
	// verglichen wird das Ende, höchstens heute, dann der Anfang. Jahre stehen
	// unter allen übrigen Gruppen; sonst rutschte das laufende Jahr (von = bis
	// = 1.1.) zwischen Januar und Dezember des Vorjahres. Bei gleichem Ende und
	// Anfang (Monatserster) steht Heute über dem Monat.
	const ende = (g: Gruppe): string => (g.bis < heute ? g.bis : heute);
	const jahr = (g: Gruppe): number => (g.stufe === 'jahr' ? 1 : 0);
	return aus.sort(
		(a, b) =>
			jahr(a) - jahr(b) ||
			ende(b).localeCompare(ende(a)) ||
			b.von.localeCompare(a.von) ||
			Number(b.heute) - Number(a.heute)
	);
}

/** Die Gruppe, in der ein Tag liegt; sonst `null`. */
export function gruppeMitTag(gruppen: readonly Gruppe[], tag: string): Gruppe | null {
	return (
		gruppen.find((g) => g.tage.some((t) => t.tag === tag)) ??
		gruppen.find((g) => g.von <= tag && tag <= g.bis) ??
		null
	);
}

// ----- Projektseiten -----

export interface Abschnitt {
	schluessel: string;
	titel: string;
	eintraege: Eintrag[];
}

export interface ProjektVerlauf {
	/** Was noch kommt (beginnt nach heute, oder geplant und noch nicht vorbei), aufsteigend. */
	kommend: Eintrag[];
	/** Das Übrige je Jahr, neueste oben. */
	vergangen: Abschnitt[];
}

/**
 * Für Projektseiten: Kommendes über der Heute-Linie aufsteigend, darunter
 * Vergangenes absteigend, je Jahr eine Überschrift. Spannen und reine
 * Jahreszahlen stehen als eigene Zeile wie jeder Eintrag.
 */
export function projektVerlauf(
	ereignisse: readonly VerlaufEreignis[],
	jetzt: Date
): ProjektVerlauf {
	const heute = tagText(jetzt);
	const kommend: Eintrag[] = [];
	const jeJahr = new Map<string, Eintrag[]>();
	for (const e of ereignisse) {
		const x = eintrag(e);
		if (x === null) continue;
		if (x.tag > heute || (e.art === 'geplant' && x.endTag >= heute)) {
			kommend.push(x);
		} else {
			const j = x.tag.slice(0, 4);
			const l = jeJahr.get(j) ?? [];
			l.push(x);
			jeJahr.set(j, l);
		}
	}
	return {
		kommend: kommend.sort(aelteste),
		vergangen: [...jeJahr]
			.sort(([a], [b]) => b.localeCompare(a))
			.map(([j, l]) => ({ schluessel: `jahr:${j}`, titel: j, eintraege: l.sort(neuesteZuerst) }))
	};
}

// ----- Teile im Zeitstrahl -----

/** Wie der Server (`verlauf::kennung_aus_text`): Kleinbuchstaben, Ziffern, Bindestriche. */
export function kennungAusText(text: string): string {
	let aus = '';
	for (const c of text.trim().toLowerCase()) {
		const ersatz = c === 'ä' ? 'ae' : c === 'ö' ? 'oe' : c === 'ü' ? 'ue' : c === 'ß' ? 'ss' : '';
		if (ersatz !== '') aus += ersatz;
		else if (/[a-z0-9]/.test(c)) aus += c;
		else if (!aus.endsWith('-')) aus += '-';
	}
	return aus
		.replace(/^-+|-+$/g, '')
		.slice(0, 60)
		.replace(/-+$/, '');
}

interface HandEintrag {
	kennung?: string;
	datum: string;
	titel: string;
}

function handKennung(e: HandEintrag): string {
	return e.kennung !== undefined && e.kennung.trim() !== ''
		? `hand:${e.kennung.trim()}`
		: `hand:${kennungAusText(e.datum)}:${kennungAusText(e.titel)}`;
}

/**
 * Die Ereignisse eines Teils: Der Server liefert sie für alle Teile
 * zusammen; die Kennung (`hand:<kennung>` oder `hand:<datum>:<titel>`, bei
 * Doppelten mit -2, -3) ordnet sie den Einträgen des Teils zu.
 *
 * Die Endung -2, -3 fällt nur weg, wenn kein Eintrag sie selbst als
 * Kennung trägt (Auflage aus 3a-1): Hat ein anderer Teil wirklich die
 * Kennung `schritt-2`, gehört `hand:schritt-2` zu ihm und nicht zu
 * `schritt`. `alle`: die Einträge aller Teile (ohne Angabe nur dieser).
 */
export function ereignisseDesTeils(
	ereignisse: readonly VerlaufEreignis[],
	eintraege: readonly HandEintrag[],
	alle: readonly HandEintrag[] = eintraege
): VerlaufEreignis[] {
	const eigene = new Set(eintraege.map(handKennung));
	const echte = new Set(alle.map(handKennung));
	return ereignisse.filter((e) => {
		if (eigene.has(e.kennung)) return true;
		if (echte.has(e.kennung)) return false;
		return eigene.has(e.kennung.replace(/-\d+$/, ''));
	});
}
