/**
 * Kopfleiste, Fassung 2 (`kopfleiste`,
 * `[fassung_2_schloss]` und `[fassung_2_live_zustaende]`): der Live-Punkt
 * mit fünf Zuständen und das Schloss daneben. Hier steht nur die Zuordnung,
 * ohne Svelte, damit sie getestet ist; den Kanal hält `freikanal.svelte.ts`.
 *
 * Alles kommt vom Server, nichts wird geraten:
 *
 * - ob der freie Kanal (`/api/events/frei`) steht, nie stand oder weg ist;
 * - das Wort zum Anmeldestand (`angemeldet`, `nicht_angemeldet`,
 *   `nicht_registriert`), das der freie Kanal gleich nach dem Verbinden
 *   schickt und das `/api/passkey/antrag` für ein fremdes Windows-Konto
 *   nennt;
 * - ob eine geschützte Anfrage 200 gab (die Frage beim Start in `zugang.ts`
 *   oder der offene geschützte Kanal).
 *
 * Der freie Kanal sieht die Windows-Anmeldung nur auf einer Verbindung, die
 * schon angemeldet ist (Edge meldet erst nach einer 401 an). Darum zählt
 * jede der drei Angaben: Sagt eine „angemeldet“, gilt angemeldet; sonst
 * sagt eine „nicht registriert“, gilt das; sonst „nicht angemeldet“.
 */
import type { Anmeldewort } from '$vertrag';

/**
 * Der freie Kanal: der erste Versuch läuft noch (`sucht`, höchstens
 * `SUCHEN_MS`), kam nie zustande, steht, oder stand und ist weg.
 */
export type KanalStand = 'sucht' | 'nie' | 'offen' | 'weg';

/**
 * So lange gilt der erste Versuch als laufend. Bis dahin zeigt der Punkt
 * keinen der fünf Zustände (er wäre geraten), danach „nicht gefunden“.
 */
export const SUCHEN_MS = 3000;

/** Die fünf Zustände des Live-Punkts. */
export type LiveZustand =
	'angemeldet' | 'nicht_angemeldet' | 'nicht_registriert' | 'nicht_gefunden' | 'unterbrochen';

/** Was neben dem Punkt steht. */
export const LIVE_TEXT: Record<LiveZustand, string> = {
	angemeldet: 'live, angemeldet',
	nicht_angemeldet: 'live, nicht angemeldet',
	nicht_registriert: 'live, nicht registriert',
	nicht_gefunden: 'Verbindung nicht gefunden',
	unterbrochen: 'Verbindung unterbrochen'
};

/** Der Abschnitt „Dashboard“ auf /pc: Ziel des Live-Punkts. */
export const STAND_ANKER = 'dashboard';

/** Der Anmeldestand aus allen Angaben des Servers (siehe oben). */
export function anmeldewort(
	geschuetztOk: boolean,
	...woerter: (Anmeldewort | null | undefined)[]
): Anmeldewort {
	if (geschuetztOk || woerter.includes('angemeldet')) return 'angemeldet';
	if (woerter.includes('nicht_registriert')) return 'nicht_registriert';
	return 'nicht_angemeldet';
}

/** Der Zustand des Live-Punkts; `null`, solange der erste Versuch läuft. */
export function liveZustand(kanal: KanalStand, wort: Anmeldewort): LiveZustand | null {
	if (kanal === 'sucht') return null;
	if (kanal === 'nie') return 'nicht_gefunden';
	if (kanal === 'weg') return 'unterbrochen';
	return wort;
}

/** Steht eine Verbindung („live“)? Nur dann gibt es Passkey-Handlungen. */
export function istLive(z: LiveZustand): boolean {
	return z === 'angemeldet' || z === 'nicht_angemeldet' || z === 'nicht_registriert';
}

/** Name des Live-Punkts für Vorlese-Programme, etwa „Verbindung: live, angemeldet, Stand ansehen“. */
export function liveName(z: LiveZustand): string {
	return istLive(z)
		? `Verbindung: ${LIVE_TEXT[z]}, Stand ansehen`
		: `${LIVE_TEXT[z]}, Stand ansehen`;
}

/**
 * Bedeutung der Farbe des Punkts: grün, solange live; gelb, wenn die
 * Verbindung weg ist und die Seite es neu versucht; rot, wenn sie nie stand.
 */
export function liveFarbe(z: LiveZustand): 'ok' | 'wartet' | 'problem' {
	if (istLive(z)) return 'ok';
	return z === 'unterbrochen' ? 'wartet' : 'problem';
}

/** Offen heißt angemeldet; in allen anderen Zuständen ist das Schloss zu. */
export function schlossOffen(z: LiveZustand): boolean {
	return z === 'angemeldet';
}

/** Name des Schlosses für Vorlese-Programme. */
export function schlossName(z: LiveZustand): string {
	if (z === 'angemeldet') return 'Angemeldet, Passkey verwalten';
	if (z === 'nicht_registriert') return 'Nicht registriert, Passkey beantragen';
	return 'Nicht angemeldet, mit Passkey anmelden';
}

/** Sprungmarken auf der Passkey-Seite (/anmelden). */
export const PASSKEY_ANKER = {
	anmelden: 'mit-passkey-anmelden',
	anlegen: 'passkey-anlegen'
} as const;

/**
 * Wohin das Schloss führt: immer zur Passkey-Seite aus 7c. Angemeldet zum
 * Anlegen eines Passkeys (und zur Liste), nicht registriert zum Antrag,
 * sonst zum Anmelden. `ziel` ist die Seite, auf die es nach dem Anmelden
 * zurückgeht (nur ein Pfad im Dashboard; sonst ohne).
 */
export function schlossZiel(z: LiveZustand, ziel: string | null = null): string {
	const anker =
		z === 'angemeldet' || z === 'nicht_registriert'
			? PASSKEY_ANKER.anlegen
			: PASSKEY_ANKER.anmelden;
	const zurueck =
		ziel !== null && ziel.startsWith('/') && !ziel.startsWith('//') && !ziel.startsWith('/anmelden')
			? `?ziel=${encodeURIComponent(ziel)}`
			: '';
	return `/anmelden${zurueck}#${anker}`;
}
