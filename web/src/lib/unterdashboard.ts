/**
 * Daten des Unter-Dashboards einer Projektseite (Etappe A1, Konzept
 * [unter_dashboard]): `/api/projekte/<slug>/stand` (Ampel, Grund, nächste
 * Frist, Notizen, Arbeiten in Kürze; die Seite liest ihn über `/stand/voll`,
 * siehe `ladeStand`); die Einträge mit Titeln kommen aus
 * `/api/anstehend?projekt=<slug>`, ebenso die Karte „Aktualisierungen“
 * (Etappe A3, mit Vorgängen und allen Titeln). Seit Schritt E2 verlangen alle
 * diese Wege die Anmeldung; ohne sie lädt die Projektseite nicht.
 *
 * Etappe 3a+: Der Server liefert Einträge und Aktualisierungen ungeordnet
 * und ohne Zahlen; `standAnsicht` und
 * `auswerten` aus anstehend.ts machen daraus, was das Unter-Dashboard zeigt.
 */
import { API } from './api/client';
import { ladeAnstehend, unterDashboardAnsicht, type UnterDashboardAnsicht } from './anstehend.ts';
import { mitZugang } from './zugang.ts';
import { teileInZone } from './zeit/zone.svelte.ts';
import type { ProjektStand } from '$vertrag';

/** Was das Unter-Dashboard zeigt, fertig ausgewertet in anstehend.ts (`unterDashboardAnsicht`). */
export type UnterDaten = UnterDashboardAnsicht;

/**
 * Holt den Stand eines Projekts. Wirft nie. Über den geschützten Weg
 * `/stand/voll` (Hotfix 3a-1): Edge schickt die Windows-Anmeldung nur auf
 * einer Verbindung, die vorher 401 bekam. Einen freien Rückfall gibt es seit
 * Schritt E3 nicht mehr; ohne Anmeldung (auch nach Ablauf) `null`.
 */
export async function ladeStand(
	slug: string,
	fetchFn: typeof fetch = fetch
): Promise<ProjektStand | null> {
	const weg = `${API}/projekte/${encodeURIComponent(slug)}/stand/voll`;
	try {
		// Etappe 3b: ohne mögliche Anmeldung (zugang.ts) ohne Anfrage gleich 401.
		const res = await mitZugang(fetchFn)(weg, {
			credentials: 'same-origin',
			headers: { accept: 'application/json' }
		});
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
		[x.slug, x.schwerpunkt, g.titel, g.status, g.bedeutung, g.ampel, g.grund].every(
			(t) => typeof t === 'string'
		) &&
		(g.zuletzt === null || typeof g.zuletzt === 'string') &&
		typeof x.frist_bald_tage === 'number' &&
		typeof x.sichtbar === 'boolean' &&
		typeof x.speicher_fehlt === 'boolean' &&
		[x.eintraege, x.aktualisierungen, x.notizen].every(Array.isArray) &&
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
	// Aus der Liste; sonst nimmt die Auswertung, was der Stand selbst hat.
	const roh = liste.stand === 'da' ? liste.liste.eintraege.filter((e) => e.projekt === slug) : null;
	const akt = liste.stand === 'da' ? liste.liste.aktualisierungen : null;
	// Was die Auswertung trotz Prüfung nicht lesen kann, gilt als nicht geladen (wirft nie).
	try {
		return unterDashboardAnsicht(stand, roh, akt);
	} catch {
		return unterDashboardAnsicht(null);
	}
}

/** `2026-10-30T14:03:00+01:00` → `30.10. 14:03` (in der Anzeigezone). Unlesbares bleibt, wie es ist. */
export function kurzZeit(zeit: string): string {
	const d = new Date(zeit);
	if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/.test(zeit) || Number.isNaN(d.getTime())) return zeit;
	const t = teileInZone(d);
	const zwei = (n: number): string => String(n).padStart(2, '0');
	return `${zwei(t.tag)}.${zwei(t.monat)}. ${zwei(t.stunde)}:${zwei(t.minute)}`;
}

/** Erste `n` und der Rest (für „und N weitere“ zum Aufklappen). */
export function teilen<T>(liste: T[], n: number): { vorn: T[]; rest: T[] } {
	return { vorn: liste.slice(0, n), rest: liste.slice(n) };
}
