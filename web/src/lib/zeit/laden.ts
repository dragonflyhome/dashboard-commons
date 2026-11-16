/**
 * Den Verlauf des PCs laden, über den geschützten Weg `/api/pc/verlauf/voll`
 * (Hotfix 3a-1): Edge schickt die Windows-Anmeldung nur auf einer
 * Verbindung, die vorher 401 bekam. Ein Fehler, auch 401 nach Ablauf der
 * Sitzung, darf die Seite nicht verhindern: dann ist `verlauf` `null` und
 * `fehler` der Satz dazu. Einen freien Rückfall gibt es seit Schritt E3 nicht
 * mehr (der freie Weg ist seit E2 ebenfalls geschützt).
 */
import type { Verlauf } from '$vertrag';
import { api, fehlerText } from '../api/client.ts';
import { mitZugang } from '../zugang.ts';
import { tagPlus, tagText } from './tage.ts';

export interface VerlaufLadung {
	verlauf: Verlauf | null;
	fehler: string | null;
}

/** `tage`: nur so viele Tage bis heute; ohne Angabe der Standard des Servers (90). */
export async function ladeVerlauf(
	f: typeof fetch,
	tage?: number,
	jetzt: Date = new Date()
): Promise<VerlaufLadung> {
	const seit = tage === undefined ? undefined : tagPlus(tagText(jetzt), -(tage - 1));
	try {
		return { verlauf: await api.verlaufVoll(seit, mitZugang(f)), fehler: null };
	} catch (e) {
		return { verlauf: null, fehler: fehlerText(e) };
	}
}
