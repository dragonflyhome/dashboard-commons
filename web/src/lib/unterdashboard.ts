/**
 * Daten des Unter-Dashboards einer Projektseite (Etappe A1, Konzept
 * [unter_dashboard]): `/api/projekte/<slug>/stand` ist frei lesbar (Ampel,
 * Grund, Zahlen, nächste Frist, Notizen, Arbeiten in Kürze; die Seite liest
 * ihn über den geschützten Weg `/stand/voll`, siehe `ladeStand`); die Einträge
 * mit Titeln kommen aus `/api/anstehend?projekt=<slug>` und nur mit
 * Windows-Anmeldung. Ohne Anmeldung zeigt die Reihe nur, was `/stand` hat.
 * Die Karte „Aktualisierungen“ (Etappe A3) kommt nur mit Anmeldung, aus
 * `/api/anstehend?projekt=` (mit Vorgängen und allen Titeln). Ohne Anmeldung
 * hat `/stand` nur die Zahlen der Meldungen (`meldungen`: neu, gesamt).
 *
 * Etappe 3a+: Der Server liefert Einträge und Aktualisierungen ungeordnet
 * und ohne Zahlen (ohne Anmeldung nur Zahlen); `standAnsicht` und
 * `auswerten` aus anstehend.ts machen daraus, was das Unter-Dashboard zeigt.
 */
import { API } from './api/client';
import { ladeAnstehend, unterDashboardAnsicht, type UnterDashboardAnsicht } from './anstehend.ts';
import { rueckfall } from './rueckfall.ts';
import { mitZugang } from './zugang.ts';
import type { ProjektStand } from '$vertrag';

/** Was das Unter-Dashboard zeigt, fertig ausgewertet in anstehend.ts (`unterDashboardAnsicht`). */
export type UnterDaten = UnterDashboardAnsicht;

/** Name des Rückfalls (`rueckfall.ts`) für `/stand/voll`, für alle Projekte. */
export const RUECKFALL_STAND = 'projekt-stand';

/**
 * Holt den Stand eines Projekts. Wirft nie. Zuerst über den geschützten Weg
 * `/stand/voll` (Hotfix 3a-1): Edge schickt die Windows-Anmeldung nur auf
 * einer Verbindung, die vorher 401 bekam; auf dem freien `/stand` fehlten
 * die Titel darum oft zufällig. Bei 401 oder 403 (keine Anmeldung möglich)
 * der freie Weg; das merkt sich die Seite (Etappe 3a-4, seit 3a+ höchstens
 * 5 Minuten oder bis der Live-Kanal angemeldet öffnet, `rueckfall.ts`),
 * weitere Ladungen gehen solange gleich den freien Weg.
 */
export async function ladeStand(
	slug: string,
	fetchFn: typeof fetch = fetch
): Promise<ProjektStand | null> {
	const weg = `${API}/projekte/${encodeURIComponent(slug)}/stand`;
	const holen = (pfad: string): Promise<Response> =>
		fetchFn(pfad, { credentials: 'same-origin', headers: { accept: 'application/json' } });
	try {
		let res: Response;
		if (rueckfall.gemerkt(RUECKFALL_STAND)) {
			res = await holen(weg);
		} else {
			// Etappe 3b: ohne mögliche Anmeldung (zugang.ts) ohne Anfrage gleich 401.
			res = await mitZugang(fetchFn)(`${weg}/voll`, {
				credentials: 'same-origin',
				headers: { accept: 'application/json' }
			});
			if (res.status === 401 || res.status === 403) {
				rueckfall.merken(RUECKFALL_STAND);
				res = await holen(weg);
			}
		}
		if (!res.ok) return null;
		const roh: unknown = await res.json();
		return istProjektStand(roh) ? roh : null;
	} catch {
		return null;
	}
}

/**
 * Hat die Antwort von `/stand` die Felder, die das Unter-Dashboard ohne
 * Rückfrage liest? Eine andere Fassung des Dienstes kann andere schicken; dann
 * gilt der Stand als nicht geladen statt halb.
 */
export function istProjektStand(x: unknown): x is ProjektStand {
	const obj = (o: unknown): o is Record<string, unknown> =>
		typeof o === 'object' && o !== null && !Array.isArray(o);
	if (!obj(x) || !obj(x.gesamt)) return false;
	const g = x.gesamt;
	return (
		[x.slug, x.schwerpunkt, g.titel, g.status, g.bedeutung, g.ampel, g.grund, g.zuletzt].every(
			(t) => typeof t === 'string'
		) &&
		typeof x.frist_bald_tage === 'number' &&
		typeof x.sichtbar === 'boolean' &&
		typeof x.speicher_fehlt === 'boolean' &&
		[x.eintraege, x.aktualisierungen, x.notizen].every(Array.isArray) &&
		(x.zahlen === null || obj(x.zahlen)) &&
		(x.fristen === undefined || Array.isArray(x.fristen))
	);
}

/** Beides zugleich, ausgewertet in anstehend.ts. */
export async function ladeUnterDaten(
	slug: string,
	fetchFn: typeof fetch = fetch
): Promise<UnterDaten> {
	const [stand, liste] = await Promise.all([
		ladeStand(slug, fetchFn),
		ladeAnstehend(mitZugang(fetchFn), slug)
	]);
	// Mit Anmeldung aus der Liste; sonst, was der Stand mit Anmeldung hat (`sichtbar`).
	const roh =
		liste.stand === 'da'
			? liste.liste.eintraege.filter((e) => e.projekt === slug)
			: stand?.sichtbar === true
				? stand.eintraege
				: null;
	const akt =
		liste.stand === 'da'
			? liste.liste.aktualisierungen
			: stand?.sichtbar === true
				? stand.aktualisierungen
				: null;
	// Was die Auswertung trotz Prüfung nicht lesen kann, gilt als nicht geladen (wirft nie).
	try {
		return unterDashboardAnsicht(stand, roh, akt);
	} catch {
		return unterDashboardAnsicht(null);
	}
}

/** `2026-10-30T14:03:00+01:00` → `30.10. 14:03` (die Ortszeit, wie der Server sie schreibt). */
export function kurzZeit(zeit: string): string {
	const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(zeit);
	return m ? `${m[3] ?? ''}.${m[2] ?? ''}. ${m[4] ?? ''}:${m[5] ?? ''}` : zeit;
}

/** Erste `n` und der Rest (für „und N weitere“ zum Aufklappen). */
export function teilen<T>(liste: T[], n: number): { vorn: T[]; rest: T[] } {
	return { vorn: liste.slice(0, n), rest: liste.slice(n) };
}
