/**
 * Tageswechsel im Browser ([klaerungen_a1] 1): Eine Frist für morgen ist
 * Bald, um Mitternacht (lokal) wird daraus heute und damit Jetzt. Eine
 * offene Seite lädt dann neu, damit die Stufen stimmen, ohne dass der Inhaber
 * etwas tun muss.
 *
 * Ein Zeitgeber steht auf der nächsten lokalen Mitternacht (plus eine
 * Sekunde, damit auch der Server schon das neue Datum hat) und wird danach
 * neu gestellt. Schläft der PC über Mitternacht, kann der Zeitgeber zu spät
 * kommen; darum prüft auch `visibilitychange` (Seite wieder sichtbar), ob
 * sich der Tag geändert hat.
 */

/** So viel nach Mitternacht feuert der Zeitgeber. */
export const PUFFER_MS = 1000;

/** Millisekunden von `jetzt` bis zur nächsten lokalen Mitternacht (nie 0). */
export function msBisMitternacht(jetzt: Date): number {
	const naechste = new Date(jetzt.getFullYear(), jetzt.getMonth(), jetzt.getDate() + 1);
	return naechste.getTime() - jetzt.getTime();
}

/** Das lokale Datum als JJJJ-MM-TT. */
export function lokalerTag(d: Date): string {
	const zwei = (n: number): string => String(n).padStart(2, '0');
	return `${String(d.getFullYear())}-${zwei(d.getMonth() + 1)}-${zwei(d.getDate())}`;
}

type Dokument = Pick<Document, 'addEventListener' | 'removeEventListener' | 'visibilityState'>;

/**
 * Ruft `rufen`, sobald ein neuer lokaler Tag beginnt (Zeitgeber und beim
 * Wiederkehren der Seite). Gibt die Abmeldung zurück (Zeitgeber und
 * Ereignis weg). `uhr` und `dokument` nur für Tests; ohne Dokument (etwa
 * in Tests ohne DOM) nur der Zeitgeber.
 */
export function beiTageswechsel(
	rufen: () => void,
	uhr: () => Date = () => new Date(),
	dokument: Dokument | null = typeof document === 'undefined' ? null : document
): () => void {
	let tag = lokalerTag(uhr());
	let zeitgeber: ReturnType<typeof setTimeout> | undefined;

	function pruefen(): void {
		const neu = lokalerTag(uhr());
		if (neu === tag) return;
		tag = neu;
		rufen();
	}
	function stellen(): void {
		clearTimeout(zeitgeber);
		zeitgeber = setTimeout(
			() => {
				pruefen();
				stellen();
			},
			msBisMitternacht(uhr()) + PUFFER_MS
		);
	}
	function sichtbar(): void {
		if (dokument?.visibilityState !== 'visible') return;
		pruefen();
		stellen();
	}

	stellen();
	dokument?.addEventListener('visibilitychange', sichtbar);
	return () => {
		clearTimeout(zeitgeber);
		dokument?.removeEventListener('visibilitychange', sichtbar);
	};
}
