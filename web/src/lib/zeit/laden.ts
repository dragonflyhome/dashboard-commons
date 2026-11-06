/**
 * Den Verlauf des PCs laden. Zuerst über den geschützten Weg
 * `/api/pc/verlauf/voll` (Hotfix 3a-1): Edge schickt die Windows-Anmeldung
 * nur auf einer Verbindung, die vorher 401 bekam, darum fehlten auf dem
 * freien Weg die Starts oft zufällig. Geht die Anmeldung nicht (401 oder 403,
 * etwa ein anderes Windows-Konto ohne Passkey), dann der freie Weg
 * `/api/pc/verlauf` mit `ohneAnmeldung`. Ein Fehler darf die Seite nicht
 * verhindern: dann ist `verlauf` `null` und `fehler` der Satz dazu.
 *
 * Etappe 3a-4: Nach einem 401 oder 403 merkt sich die Seite den Rückfall
 * (`rueckfall`); weitere Ladungen gehen gleich den freien Weg, statt jedes
 * Mal `/voll` zu versuchen. Seit 3a+ höchstens 5 Minuten oder bis der
 * Live-Kanal angemeldet öffnet; dann wieder `/voll`. Eine Anmeldung mit
 * Passkey lädt die Seite ohnehin neu (`/anmelden` mit `location.assign`).
 */
import type { Verlauf, VerlaufHinweis } from '$vertrag';
import { ApiError, api, fehlerText } from '../api/client.ts';
import { rueckfall } from '../rueckfall.ts';
import { mitZugang } from '../zugang.ts';
import { tagPlus, tagText } from './tage.ts';

export interface VerlaufLadung {
	verlauf: Verlauf | null;
	fehler: string | null;
	/** Ohne Anmeldung geladen (freier Weg): Starts, Meldungen und Vorgänge fehlen. */
	ohneAnmeldung: boolean;
}

/** Ruhiger Hinweis, wenn der Verlauf ohne Anmeldung kam. */
export const SATZ_VERLAUF_OHNE_ANMELDUNG =
	'Starts, Meldungen und Vorgänge zeigt der Verlauf nur mit Anmeldung.';

/** Name des Rückfalls (`rueckfall.ts`) für den Verlauf. */
export const RUECKFALL_VERLAUF = 'pc-verlauf';

/**
 * Die Hinweise des Servers für den Kasten „Hinweise zur Vollständigkeit“,
 * als Sätze. Kam der Verlauf ohne Anmeldung, steht SATZ_VERLAUF_OHNE_ANMELDUNG
 * schon sichtbar da; der Hinweis der Art `ohne-anmeldung` entfällt dann,
 * damit er nicht zweimal erscheint. Jede andere Art, auch eine, die die
 * Oberfläche nicht kennt, bleibt mit ihrem Satz unverändert stehen.
 */
export function hinweiseFuerKasten(hinweise: VerlaufHinweis[], ohneAnmeldung: boolean): string[] {
	return hinweise.filter((h) => !(ohneAnmeldung && h.art === 'ohne-anmeldung')).map((h) => h.text);
}

/** `tage`: nur so viele Tage bis heute; ohne Angabe der Standard des Servers (90). */
export async function ladeVerlauf(
	f: typeof fetch,
	tage?: number,
	jetzt: Date = new Date()
): Promise<VerlaufLadung> {
	const seit = tage === undefined ? undefined : tagPlus(tagText(jetzt), -(tage - 1));
	if (!rueckfall.gemerkt(RUECKFALL_VERLAUF)) {
		try {
			return {
				verlauf: await api.verlaufVoll(seit, mitZugang(f)),
				fehler: null,
				ohneAnmeldung: false
			};
		} catch (e) {
			if (!(e instanceof ApiError && (e.status === 401 || e.status === 403))) {
				return { verlauf: null, fehler: fehlerText(e), ohneAnmeldung: false };
			}
			rueckfall.merken(RUECKFALL_VERLAUF);
		}
	}
	try {
		return { verlauf: await api.verlauf(seit, f), fehler: null, ohneAnmeldung: true };
	} catch (e) {
		return { verlauf: null, fehler: fehlerText(e), ohneAnmeldung: true };
	}
}
