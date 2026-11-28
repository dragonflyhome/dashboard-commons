/**
 * Tageswechsel am Blickort (Stufe 7b): wie tageswechsel.ts, aber nach dem
 * Tag in der Anzeigezone. Nutzer ist `LaufendesJetzt`.
 */
import { msBisMitternacht, PUFFER_MS } from '../tageswechsel.ts';
import { anzeigeZone, tagInZone, teileInZone } from './zone.svelte.ts';

/** Millisekunden seit Mitternacht in der Anzeigezone (ohne Millisekunden). */
function seitMitternacht(d: Date): number {
	const t = teileInZone(d);
	return ((t.stunde * 60 + t.minute) * 60 + t.sekunde) * 1000;
}

/**
 * Millisekunden von `jetzt` bis zur nächsten Mitternacht in der Anzeigezone
 * (nie 0), auch an Tagen mit 23 oder 25 Stunden.
 */
export function msBisMitternachtInZone(jetzt: Date): number {
	if (anzeigeZone() === null) return msBisMitternacht(jetzt);
	// Erst 24 Stunden ab Mitternacht; eine Umstellung dazwischen verschiebt das Ziel um
	// ihren Betrag, den die Uhrzeit am Ziel zeigt: vor 12 Uhr zu spät, danach zu früh.
	const schaetzung =
		jetzt.getTime() + 86_400_000 - seitMitternacht(jetzt) - jetzt.getMilliseconds();
	const rest = seitMitternacht(new Date(schaetzung));
	const ziel = rest < 43_200_000 ? schaetzung - rest : schaetzung + 86_400_000 - rest;
	// Fällt 0:00 selbst aus (Umstellung um Mitternacht), landet die Korrektur im alten Tag;
	// dann ist die Schätzung schon der erste Augenblick des neuen.
	const neuerTag = ziel > jetzt.getTime() && tagInZone(new Date(ziel)) !== tagInZone(jetzt);
	return (neuerTag ? ziel : schaetzung) - jetzt.getTime();
}

type Dokument = Pick<Document, 'addEventListener' | 'removeEventListener' | 'visibilityState'>;

/**
 * Wie `beiTageswechsel`, aber für den Tag in der Anzeigezone. Gibt die
 * Abmeldung zurück. `uhr` und `dokument` nur für Tests.
 */
export function beiTageswechselInZone(
	rufen: () => void,
	uhr: () => Date = () => new Date(),
	dokument: Dokument | null = typeof document === 'undefined' ? null : document
): () => void {
	let tag = tagInZone(uhr());
	let zeitgeber: ReturnType<typeof setTimeout> | undefined;

	function pruefen(): void {
		const neu = tagInZone(uhr());
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
			msBisMitternachtInZone(uhr()) + PUFFER_MS
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
