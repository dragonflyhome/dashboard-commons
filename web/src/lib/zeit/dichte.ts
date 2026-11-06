/**
 * Der Dichtestreifen über dem Verlauf: je Tag ein Balken mit der Zahl der
 * Einträge, Zoom auf 30 oder 14 Tage, ein Tag als Cursor.
 *
 * Wie viele Tage: Der Server liefert ohne Angabe den Verlauf ab dem
 * Tagesanfang vor 90 Tagen (`VERLAUF_TAGE` in src/api/pc.rs), also 91
 * Kalendertage einschließlich heute. Der Streifen zeigt sie alle
 * (Auflage aus 3a-1: der älteste der 91 Tage fehlte).
 */
import type { VerlaufEreignis } from '$vertrag';
import { eintrag } from './verlauf.ts';
import { tagPlus, tagZahl } from './tage.ts';

export interface TagesBalken {
	tag: string;
	anzahl: number;
	/** Davon keine Routine. */
	wichtig: number;
}

/** Tage, die der Streifen ohne Angabe zeigt: heute und die 90 Tage davor, wie der Server. */
export const DICHTE_TAGE = 91;

/** Die engeren Zoomstufen; die weiteste ist immer der ganze Streifen. */
export const DICHTE_ENGER = [30, 14] as const;

/** Je Tag von `heute - tage + 1` bis `heute` die Zahl der Einträge (nach Beginn). */
export function tageZaehlen(
	ereignisse: readonly VerlaufEreignis[],
	heute: string,
	tage = DICHTE_TAGE
): TagesBalken[] {
	const erster = tagPlus(heute, -(tage - 1));
	const n = new Map<string, TagesBalken>();
	const aus: TagesBalken[] = [];
	for (let i = 0; i < tage; i++) {
		const b = { tag: tagPlus(erster, i), anzahl: 0, wichtig: 0 };
		n.set(b.tag, b);
		aus.push(b);
	}
	for (const e of ereignisse) {
		if (e.genauigkeit === 'monat' || e.genauigkeit === 'jahr') continue;
		const x = eintrag(e);
		const b = x === null ? undefined : n.get(x.tag);
		if (b === undefined) continue;
		b.anzahl += 1;
		if (e.art !== 'routine') b.wichtig += 1;
	}
	return aus;
}

/** Ein Ausschnitt in ganzen Tagen (beide eingeschlossen). */
export interface TagFenster {
	von: string;
	bis: string;
}

/** Tage im Ausschnitt. */
export function laenge(f: TagFenster): number {
	return tagZahl(f.bis) - tagZahl(f.von) + 1;
}

/** Den Ausschnitt so schieben, dass `cursor` darin liegt, innerhalb von `grenzen`. */
export function fensterMitCursor(f: TagFenster, cursor: string, grenzen: TagFenster): TagFenster {
	const n = laenge(f);
	let von = f.von;
	if (cursor < von) von = cursor;
	if (cursor > f.bis) von = tagPlus(cursor, -(n - 1));
	if (von < grenzen.von) von = grenzen.von;
	if (tagPlus(von, n - 1) > grenzen.bis) von = tagPlus(grenzen.bis, -(n - 1));
	return { von, bis: tagPlus(von, n - 1) };
}

/** Neue Länge, um den Cursor herum (so weit es die Grenzen erlauben). */
export function fensterZoomen(n: number, cursor: string, grenzen: TagFenster): TagFenster {
	const gesamt = laenge(grenzen);
	if (n >= gesamt) return { ...grenzen };
	const von = tagPlus(cursor, -Math.floor(n / 2));
	return fensterMitCursor({ von, bis: tagPlus(von, n - 1) }, cursor, grenzen);
}

/** Die Zoomstufen von weit nach eng: der ganze Streifen, dann 30 und 14 Tage. */
export function dichteSpannen(gesamt: number): number[] {
	return [gesamt, ...DICHTE_ENGER.filter((s) => s < gesamt)];
}

/**
 * Die nächste Spanne beim Zoomen (`richtung` 1 = enger, -1 = weiter).
 * `gesamt`: Tage des ganzen Streifens (die weiteste Stufe).
 */
export function naechsteSpanne(jetzt: number, richtung: 1 | -1, gesamt = DICHTE_TAGE): number {
	const stufen = dichteSpannen(gesamt);
	const i = stufen.findIndex((s) => s === jetzt);
	const j = Math.min(Math.max((i < 0 ? 0 : i) + richtung, 0), stufen.length - 1);
	return stufen[j] ?? gesamt;
}

/**
 * Was der Schieberegler Bildschirmlesern über den Ausschnitt sagt: „die
 * letzten 30 Tage“, solange er bis heute reicht, sonst die Tage mit Datum
 * (Auflage aus 3a-1: gezoomt stimmte „letzte … Tage“ nicht mehr).
 */
export function ausschnittText(
	f: TagFenster,
	heute: string,
	datum: (tag: string) => string
): string {
	const n = laenge(f);
	if (f.bis === heute) return `die letzten ${String(n)} Tage bis heute`;
	return `${String(n)} Tage vom ${datum(f.von)} bis ${datum(f.bis)}`;
}
