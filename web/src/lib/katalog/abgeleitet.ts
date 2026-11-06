/**
 * Kleine Hilfen für die Katalog-Bausteine: Datei<T> in Teile zerlegen, den
 * Ladezustand erkennen und aus `abgeleitet` (vom Server gerechnet) einen
 * kurzen Satz machen. Rechnet nichts selbst nach.
 */
import type { Abgeleitet, Datei, ListeAbgeleitet } from '$vertrag';
import type { Geladen } from '../projekt/laden.svelte.ts';

/** Was ein Daten-Baustein annimmt: den Inhalt selbst oder das Objekt aus `ladeDaten`. */
export type Quelle<T extends object> = Datei<T> | Geladen<Datei<T>>;

/** Ist das ein Objekt aus `ladeDaten` (und nicht schon der Inhalt)? */
export function istGeladen<T extends object>(q: Quelle<T>): q is Geladen<Datei<T>> {
	return 'zustand' in q && 'neuLaden' in q;
}

/** Pfad einer Liste in `abgeleitet.listen`, bei Teilen mit `teil.<name>.` davor. */
export function listenPfad(liste: string, teil: string | null): string {
	return teil === null ? liste : `teil.${teil}.${liste}`;
}

/** Das Abgeleitete einer Liste, wenn der Server es geschickt hat. */
export function liste(
	abgeleitet: Abgeleitet | null | undefined,
	pfad: string
): ListeAbgeleitet | null {
	return abgeleitet?.listen[pfad] ?? null;
}

/** „3 von 5 erledigt“, wenn der Server Erledigtes zählen konnte; sonst `null`. */
export function erledigtSatz(l: ListeAbgeleitet | null): string | null {
	if (l?.erledigt == null || l.anzahl === 0) return null;
	return `${l.erledigt.anzahl} von ${l.anzahl} erledigt`;
}

/** „heute“, „morgen“, „in 12 Tagen“, „vor 3 Tagen“ aus Tagen bis zu einem Datum. */
export function tageSatz(tageBis: number): string {
	if (tageBis === 0) return 'heute';
	if (tageBis === 1) return 'morgen';
	if (tageBis === -1) return 'gestern';
	return tageBis > 0 ? `in ${tageBis} Tagen` : `vor ${-tageBis} Tagen`;
}

/**
 * „Als Nächstes: Abgabe, in 12 Tagen“ für eine Datumsspalte; `titel` liefert
 * den Namen des Eintrags an einer Stelle der Liste. `null`, wenn nichts mehr
 * kommt oder der Server die Spalte nicht kennt.
 */
export function naechsterSatz(
	l: ListeAbgeleitet | null,
	spalte: string,
	titel: (index: number) => string | undefined
): string | null {
	const d = l?.datum[spalte];
	if (d?.naechster == null) return null;
	const tage = d.tage_bis[d.naechster];
	const name = titel(d.naechster);
	if (tage == null || name === undefined) return null;
	return `Als Nächstes: ${name}, ${tageSatz(tage)}`;
}
