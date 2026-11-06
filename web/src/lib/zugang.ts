/**
 * Ist eine Anmeldung möglich? (Etappe 3b, Fund des Prüfers „127.0.0.1: 6× 401
 * je Laden“.)
 *
 * Über 127.0.0.1 schickt Edge die Windows-Anmeldung nicht mit; ohne Passkey
 * bekam dort jede geschützte Adresse, die Layout, Speicher und Seiten fragen,
 * ein 401 (Live-Kanal, Aufgaben, Anstehend, Passkey-Anträge, Benutzer-Daten,
 * Verlauf, Meldungen …), je Laden sechs bis acht. Jetzt fragt die Seite einmal
 * beim Start (`hooks.client.ts`, `init`) eine geschützte Adresse
 * (`/api/vorgaenge/uebersicht`) und merkt sich die Antwort:
 *
 * - `da`: angemeldet (Windows oder Passkey); alles wie bisher.
 * - `fehlt` (401) oder `fremd` (403 `fremdes-konto`): Die geschützten
 *   Abfragen gehen nicht mehr ins Netz. `mitZugang` antwortet an ihrer Stelle
 *   mit derselben Antwort, die der Server gäbe (401 oder 403 mit Satz und
 *   `art`); die Speicher und Seiten zeigen also genau ihren bisherigen Satz.
 * - `unbekannt` (Netzfehler, Server startet, anderer Code): Alles geht ins
 *   Netz wie bisher.
 *
 * Die Schleusen im Server bleiben unverändert; das hier spart nur Anfragen.
 * Wie der Rückfall (`rueckfall.ts`) gilt `fehlt` oder `fremd` höchstens
 * `FRIST_MS`; danach fragt die nächste geschützte Abfrage wieder echt. Öffnet
 * der Live-Kanal (dann ist die Verbindung angemeldet), gilt `da`. Bewusst im
 * Speicher des Moduls: Die Anmeldung mit Passkey lädt die Seite neu.
 */
import { SATZ_FREMDES_KONTO } from './anmeldung.ts';
import { FRIST_MS } from './rueckfall.ts';

export type Zugangsstand = 'da' | 'fehlt' | 'fremd' | 'unbekannt';

/** Die Adresse, an der die Seite einmal fragt (geschützt, klein, ohne Nebenwirkung). */
export const ZUGANG_ADRESSE = '/api/vorgaenge/uebersicht';

/** Wie lange die Frage beim Start höchstens warten darf, in Millisekunden. */
export const ZUGANG_FRIST_MS = 3000;

export class Zugang {
	#stand: Zugangsstand = 'unbekannt';
	#seit = 0;
	#laeuft: Promise<Zugangsstand> | null = null;
	#art: string | null = null;

	/**
	 * Die Art der letzten 401 (`anmeldung` oder `passkey`), sonst `null`.
	 * `passkey` heißt: Windows hat ein fremdes Konto angemeldet (Kopfleiste,
	 * Fassung 2; die Seite fragt dann nach dem Wort für dieses Konto).
	 */
	get art(): string | null {
		return this.stand === 'fehlt' ? this.#art : null;
	}

	/** Der gemerkte Stand; `fehlt` und `fremd` verfallen nach `FRIST_MS`. */
	get stand(): Zugangsstand {
		if (
			(this.#stand === 'fehlt' || this.#stand === 'fremd') &&
			Date.now() - this.#seit >= FRIST_MS
		) {
			this.#stand = 'unbekannt';
		}
		return this.#stand;
	}

	/** Ohne Anmeldung (oder fremdes Konto): geschützte Abfragen bleiben lokal. */
	get gesperrt(): boolean {
		const s = this.stand;
		return s === 'fehlt' || s === 'fremd';
	}

	setzen(stand: Zugangsstand): void {
		this.#stand = stand;
		this.#seit = Date.now();
	}

	/** Einmal fragen; läuft schon eine Frage, wartet diese mit. Wirft nie. */
	pruefen(fetchFn: typeof fetch = (...a) => fetch(...a)): Promise<Zugangsstand> {
		this.#laeuft ??= (async () => {
			let stand: Zugangsstand = 'unbekannt';
			let art: string | null = null;
			try {
				const res = await fetchFn(ZUGANG_ADRESSE, {
					credentials: 'same-origin',
					headers: { accept: 'application/json' },
					signal: AbortSignal.timeout(ZUGANG_FRIST_MS)
				});
				if (res.ok) stand = 'da';
				else if (res.status === 401) {
					stand = 'fehlt';
					const body = (await res.json().catch(() => null)) as { art?: unknown } | null;
					art = typeof body?.art === 'string' ? body.art : null;
				} else if (res.status === 403) {
					const body = (await res.json().catch(() => null)) as { art?: unknown } | null;
					if (body?.art === 'fremdes-konto') stand = 'fremd';
				}
				await res.body?.cancel().catch(() => undefined);
			} catch {
				// Netzfehler oder Zeitablauf: unbekannt, alles geht ins Netz.
			}
			this.setzen(stand);
			this.#art = art;
			return stand;
		})().finally(() => {
			this.#laeuft = null;
		});
		return this.#laeuft;
	}

	/** Vergisst den Stand: für Tests und beim Wechsel zum freien Kanal (Abmelden, Widerruf; Kopfleiste 2). */
	vergessen(): void {
		this.#stand = 'unbekannt';
		this.#seit = 0;
		this.#laeuft = null;
		this.#art = null;
	}
}

/** Der eine Zugang für die ganze Seite. */
export const zugang = new Zugang();

/** Die Antwort, die der Server ohne Anmeldung gäbe (Körper wie `AppError`). */
export function antwortOhneZugang(stand: 'fehlt' | 'fremd'): Response {
	const [status, body] =
		stand === 'fehlt'
			? [401, { fehler: 'Bitte mit dem Windows-Konto anmelden.', art: 'anmeldung' }]
			: [403, { fehler: SATZ_FREMDES_KONTO, art: 'fremdes-konto' }];
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

/**
 * `fetch` für geschützte Adressen: Ist keine Anmeldung möglich (`zugang`),
 * kommt die Antwort des Servers ohne Anfrage; sonst geht es ins Netz.
 */
export function mitZugang(f: typeof fetch, z: Zugang = zugang): typeof fetch {
	return (...a) => {
		const s = z.stand;
		if (s === 'fehlt' || s === 'fremd') return Promise.resolve(antwortOhneZugang(s));
		return f(...a);
	};
}
