/**
 * „Etwas anderes …“: Bei jeder Klick-Antwort
 * (Fragen der Arten wahl, ja-nein und bestaetigen, Aufgaben mit Knöpfen oder
 * Haken) setzt die Oberfläche selbst einen letzten Knopf dazu. Er klappt ein
 * Feld für eigene Worte auf; die Antwort geht denselben Weg wie ein Klick,
 * mit `antwort = "etwas-anderes"` und dem Text in `eigene`.
 *
 * Der Entwurf im Feld steht je Tab in sessionStorage: Er überlebt Neuladen,
 * Live-Änderungen und Seitenwechsel, bleibt aber in diesem Browser. Fällt der
 * Speicher aus (privates Fenster, gesperrt), gilt er nur, solange das Feld
 * offen ist; die Seite läuft trotzdem.
 */
import { frageSchluessel, type FrageEintrag } from './antworten.ts';

/** Der feste Wert in `antwort` (wie im Server, `vertrag::daten::ETWAS_ANDERES`). */
export const ETWAS_ANDERES = 'etwas-anderes';
/** Technische Obergrenze der eigenen Worte (wie im Server); beim Schreiben nie im Weg. */
export const EIGENE_HOECHSTENS = 20_000;
/** Die Beschriftung des Knopfs. */
export const KNOPF_TEXT = 'Etwas anderes …';
/** Satz, wenn das Feld beim Senden leer ist (wie im Server). */
export const SATZ_LEER = 'Das Feld ist leer. Bitte schreib in eigenen Worten, was du meinst.';
/** Hinweis unter dem Feld. */
export const SATZ_HINWEIS =
	'Schreib in eigenen Worten, was du meinst. Es geht wie ein Klick als deine Antwort an den Chat.';

const PRAEFIX = 'dashboard:etwas-anderes:';

/** Was der Entwurf zum Lesen und Schreiben braucht (in Tests ersetzbar). */
export type EntwurfSpeicher = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

function sitzung(): EntwurfSpeicher | null {
	try {
		return typeof sessionStorage === 'undefined' ? null : sessionStorage;
	} catch {
		return null;
	}
}

/** Schlüssel des Entwurfs zu einer Frage (je Projekt, Kennung und `gestellt`). */
export function frageEntwurf(e: Pick<FrageEintrag, 'projekt' | 'id' | 'gestellt'>): string {
	return `frage:${frageSchluessel(e)}`;
}

/** Schlüssel des Entwurfs zu einer Aufgabe. */
export function aufgabeEntwurf(id: string): string {
	return `aufgabe:${id}`;
}

/** Der gemerkte Entwurf (leer, wenn keiner da ist oder der Speicher ausfällt). */
export function entwurfLesen(schluessel: string, speicher = sitzung()): string {
	try {
		return speicher?.getItem(PRAEFIX + schluessel) ?? '';
	} catch {
		return '';
	}
}

/** Merkt den Entwurf; ein leerer Text löscht ihn. Wirft nie. */
export function entwurfSchreiben(schluessel: string, text: string, speicher = sitzung()): void {
	try {
		if (text === '') speicher?.removeItem(PRAEFIX + schluessel);
		else speicher?.setItem(PRAEFIX + schluessel, text);
	} catch {
		// Speicher voll oder gesperrt: dann gilt der Entwurf nur, solange das Feld offen ist.
	}
}
