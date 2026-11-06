/**
 * Legt Neuladen aus mehreren Ereignissen zusammen (Kleinigkeit aus der
 * Abnahme live-luecken: PasskeyVerwaltung lud doppelt).
 *
 * Ein Vorgang schickt erst `vorgang`, dann, nach der Bündelzeit des Servers
 * (250 ms), `speicher` mit Bereich für dieselbe Schreibung. Darum:
 *
 * - `spaeter()`: für das Ereignis, dem meist ein genaueres folgt (`vorgang`).
 *   Lädt nach `fensterMs`, wenn bis dahin nichts anderes geladen hat.
 * - `jetzt()`: für das genaue Ereignis (`speicher` mit dem eigenen Bereich,
 *   `nachholen`). Lädt sofort und streicht ein geplantes `spaeter()`.
 *
 * Vorgang und Speicher innerhalb des Fensters ergeben so eine Ladung; ein
 * Vorgang ohne Speicher-Ereignis (anderer Bereich) lädt zur Sicherung nach
 * dem Fenster trotzdem.
 */

/** Etwas länger als die Bündelzeit des Servers (250 ms) plus Laufzeit. */
export const NACHLADEN_FENSTER_MS = 400;

export class Nachlader {
	readonly #laden: () => void;
	readonly #fensterMs: number;
	#timer: ReturnType<typeof setTimeout> | null = null;

	constructor(laden: () => void, fensterMs: number = NACHLADEN_FENSTER_MS) {
		this.#laden = laden;
		this.#fensterMs = fensterMs;
	}

	/** Lädt nach dem Fenster, wenn bis dahin nicht schon geladen wurde; mehrfach: einmal. */
	spaeter(): void {
		if (this.#timer !== null) return;
		this.#timer = setTimeout(() => {
			this.#timer = null;
			this.#laden();
		}, this.#fensterMs);
	}

	/** Lädt sofort; ein geplantes `spaeter()` entfällt. */
	jetzt(): void {
		this.stop();
		this.#laden();
	}

	/** Streicht ein geplantes Laden (beim Abbau der Komponente). */
	stop(): void {
		if (this.#timer !== null) clearTimeout(this.#timer);
		this.#timer = null;
	}
}
