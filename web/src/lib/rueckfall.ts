/**
 * Rückfall auf den freien Weg merken (Auflage aus Hotfix 3a-1, Etappe 3a-4,
 * ergänzt in 3a+).
 *
 * Die Seite liest den Verlauf und den Stand eines Projekts zuerst über die
 * geschützten Wege `…/voll`. Kommt dort 401 oder 403 (keine Anmeldung
 * möglich, etwa ein anderes Windows-Konto ohne Passkey), nimmt sie den
 * freien Weg. Damit nicht jede weitere Ladung (Live-Kanal, Seitenwechsel)
 * wieder erst `/voll` versucht, merkt sie sich das hier.
 *
 * Seit 3a+ nicht mehr bis zum Neuladen (Auflage aus 3a-4: ein kurzes 401 auf
 * dem eigenen Konto ließ die Seite bis dahin auf dem freien Weg): Der
 * Rückfall gilt höchstens `FRIST_MS` (5 Minuten); danach versucht die
 * nächste Ladung wieder `/voll`. Und er fällt sofort weg, sobald eine
 * Anmeldung da ist (`vergessen`): Der Live-Kanal öffnet nur mit Anmeldung
 * (`live.svelte.ts`, `onopen`) und lädt dann neu.
 *
 * Bewusst im Speicher des Moduls und nicht in `sessionStorage`: Die Anmeldung
 * mit Passkey lädt die Seite neu (`location.assign`), danach soll `/voll`
 * sofort wieder gelten. In `sessionStorage` bliebe der Rückfall über die
 * Anmeldung hinweg stehen.
 */

/** So lange gilt ein Rückfall, in Millisekunden. */
export const FRIST_MS = 5 * 60_000;

export class Rueckfall {
	/** Weg → Zeitpunkt des Rückfalls (`Date.now()`). */
	#gemerkt = new Map<string, number>();

	/** Ob für `weg` der Rückfall gilt (gemerkt und noch keine `FRIST_MS` alt). */
	gemerkt(weg: string): boolean {
		const seit = this.#gemerkt.get(weg);
		if (seit === undefined) return false;
		if (Date.now() - seit < FRIST_MS) return true;
		this.#gemerkt.delete(weg);
		return false;
	}

	/** Nach 401 oder 403 auf `…/voll`. */
	merken(weg: string): void {
		this.#gemerkt.set(weg, Date.now());
	}

	/**
	 * Alles vergessen: bei einem Anmelde-Ereignis (Live-Kanal offen) und in
	 * Tests. Gibt zurück, ob ein Rückfall galt (dann lohnt neues Laden).
	 */
	vergessen(): boolean {
		const galt = [...this.#gemerkt.keys()].some((w) => this.gemerkt(w));
		this.#gemerkt.clear();
		return galt;
	}
}

/** Der eine Rückfall für die ganze Seite. */
export const rueckfall = new Rueckfall();
