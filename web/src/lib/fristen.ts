/**
 * Modul Fristen (Etappe B3): Worte für die Fristenliste und das Laden der
 * Fristen aller Projekte (`GET /api/fristen`, geschützt). Gerechnet ist alles
 * im Server (aufgelöste Daten, Tage bis, Frist-Stufe, Ordnung); hier entstehen
 * nur Sätze.
 */
import type { EreignisStand, FristBezug, FristenListe, FristQuelle, FristZeile } from '$vertrag';
import { API } from './api/client.ts';

/** Woher eine Zeile kommt, als Wort. */
export const QUELLE_WORT: Readonly<Record<FristQuelle, string>> = {
	datei: 'Frist',
	aufgabe: 'Aufgabe',
	frage: 'Frage',
	arbeit: 'Arbeit'
};

/**
 * „hängt an „Zusage ist da“ (+14 Tage)“ für eine bewegliche Frist ohne
 * Datum; `null`, wenn die Frist ein Datum hat oder nicht beweglich ist. Gibt
 * es das Ereignis nicht (mehr), steht seine Kennung da.
 */
export function haengtSatz(z: {
	frist: string | null;
	frist_bezug?: FristBezug | null | undefined;
	ereignis?: EreignisStand | null | undefined;
}): string | null {
	if (z.frist !== null || !z.frist_bezug) return null;
	const name = z.ereignis?.titel ?? null;
	const abstand = z.frist_bezug.abstand;
	return name === null
		? `hängt an „${z.frist_bezug.ereignis}“ (${abstand}; dieses Ereignis fehlt in fristen.toml)`
		: `hängt an „${name}“ (${abstand})`;
}

/** „aus „Zusage ist da“ +1 Monat“ für eine aufgelöste bewegliche Frist, sonst `null`. */
export function bezugSatz(
	z: Pick<FristZeile, 'frist' | 'frist_bezug' | 'ereignis'>
): string | null {
	if (z.frist === null || z.frist_bezug === null || z.ereignis?.datum == null) return null;
	const [, m, t] = z.ereignis.datum.split('-');
	return `aus „${z.ereignis.titel ?? z.frist_bezug.ereignis}“ (${t ?? ''}.${m ?? ''}.) ${z.frist_bezug.abstand}`;
}

/** Holt die Fristen aller sichtbaren Projekte; `null` ohne Anmeldung oder bei Fehlern. Wirft nie. */
export async function ladeFristen(fetchFn: typeof fetch = fetch): Promise<FristenListe | null> {
	try {
		const res = await fetchFn(`${API}/fristen`, {
			credentials: 'same-origin',
			headers: { accept: 'application/json' }
		});
		if (!res.ok) return null;
		const l = (await res.json()) as Partial<FristenListe> | null;
		// Nur eine Antwort in der Form der Fristenliste; sonst nichts zeigen.
		return l !== null && Array.isArray(l.zeilen) && Array.isArray(l.hinweise)
			? (l as FristenListe)
			: null;
	} catch {
		return null;
	}
}
