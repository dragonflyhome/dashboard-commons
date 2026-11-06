/**
 * Antwortet die API? (Zustand 1 am Live-Punkt, Vorgabe: „Live-Kanal
 * getrennt oder die API antwortet nicht“.) `false`, sobald ein Abruf ohne
 * Antwort scheitert (Netzfehler), bis wieder eine Antwort kommt. Eine Antwort
 * mit 5xx ist eine Antwort: Der Server meldet so auch Lagen der App
 * (503 `speicher-fehlt`, `bote-fehlt`), keine fehlende Verbindung.
 */
export const apiStand = $state({ antwortet: true });

/** Ruft `fetchFn` und merkt, ob eine Antwort kam. Ein Abbruch (`AbortError`) zählt nicht. */
export async function abrufen(
	fetchFn: typeof fetch,
	pfad: string,
	init?: RequestInit
): Promise<Response> {
	try {
		const res = await fetchFn(pfad, init);
		apiStand.antwortet = true;
		return res;
	} catch (e) {
		if (!(e instanceof DOMException && e.name === 'AbortError')) apiStand.antwortet = false;
		throw e;
	}
}
