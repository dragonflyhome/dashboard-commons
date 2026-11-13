/**
 * Fassungen des Dashboards (Entscheidung „Freigabe in einem Schritt“): die
 * Regeln für die Seite /fassungen, die Fußzeile und „Neu in …“, ohne Svelte.
 * Die Daten kommen aus dem Changelog, das der Dienst beim Bau einbettet
 * (`/api/fassungen`, `/api/betrieb`, `/api/fassungen/gezeigt`).
 */
import type { FassungsZeilen } from '$vertrag';

/** Die Unterabschnitte in der Reihenfolge des Changelogs, mit Überschrift. */
export const ABSCHNITTE = [
	['hinzugefuegt', 'Hinzugefügt'],
	['geaendert', 'Geändert'],
	['entfernt', 'Entfernt'],
	['behoben', 'Behoben']
] as const satisfies readonly (readonly [keyof FassungsZeilen, string])[];

/** Nur die Unterabschnitte mit Zeilen. */
export function abschnitteMitZeilen(
	z: FassungsZeilen
): { titel: string; zeilen: readonly string[] }[] {
	return ABSCHNITTE.filter(([k]) => z[k].length > 0).map(([k, titel]) => ({
		titel,
		zeilen: z[k]
	}));
}

/** Hat der Abschnitt überhaupt Zeilen? */
export function hatZeilen(z: FassungsZeilen): boolean {
	return abschnitteMitZeilen(z).length > 0;
}

/** Der Anfang einer Zeile, die einen Bruch nennt (Regel im Changelog). */
export const BRUCH = 'Bruch: ';

/**
 * Eine Zeile mit „Bruch: …“ geteilt, damit das Wort hervorgehoben stehen kann;
 * nur, wenn der Dienst sie als Bruch nennt (unter Geändert oder Entfernt).
 */
export function bruchTeilen(
	zeile: string,
	brueche: readonly string[]
): { bruch: boolean; rest: string } {
	return zeile.startsWith(BRUCH) && brueche.includes(zeile)
		? { bruch: true, rest: zeile.slice(BRUCH.length) }
		: { bruch: false, rest: zeile };
}

/** Die laufende Fassung in der Fußzeile; zwischen zwei Fassungen mit „+ Unveröffentlicht“. */
export function fussText(version: string, unveroeffentlicht: boolean): string {
	return `Fassung ${version}${unveroeffentlicht ? ' + Unveröffentlicht' : ''}`;
}
