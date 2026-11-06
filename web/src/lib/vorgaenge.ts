/** Kleine Helfer für die Seite /vorgaenge: Farben, Wörter, Zeiten. */
import type { Bedeutung, Vorgang, VorgangsArt, VorgangsZustand } from '$vertrag';

const MINUTE = 60_000;
const STUNDE = 60 * MINUTE;
const TAG = 24 * STUNDE;

/** Bedeutung des farbigen Punkts je Zustand. */
export function zustandBedeutung(z: VorgangsZustand): Bedeutung {
	switch (z) {
		case 'offen':
			return 'wartet';
		case 'zugestellt':
			return 'neutral';
		case 'uebernommen':
			return 'laeuft';
		case 'erledigt':
			return 'ok';
	}
}

/** Zustand in einfachen Worten. */
export function zustandWort(z: VorgangsZustand): string {
	switch (z) {
		case 'offen':
			return 'offen';
		case 'zugestellt':
			return 'zugestellt';
		case 'uebernommen':
			return 'übernommen';
		case 'erledigt':
			return 'erledigt';
	}
}

/** Art als kurzes Etikett (`oeffnen` und `schritt` legt nur der Server an: beim Knopf „Öffnen“, beim Anbieten und Zurückwecken eines Teilschritts). */
export function artWort(a: VorgangsArt): string {
	switch (a) {
		case 'antwort':
			return 'Antwort';
		case 'frage':
			return 'Frage';
		case 'wunsch':
			return 'Wunsch';
		case 'bau':
			return 'Seite erneuern';
		case 'abnahme':
			return 'Abnahme';
		case 'merge':
			return 'Merge-Request';
		case 'passkey-antrag':
			return 'Passkey-Antrag';
		case 'oeffnen':
			return 'Geöffnet';
		case 'schritt':
			return 'Teilschritt';
	}
}

/** Herkunft eines Schritts: „browser“ → „dir (Browser)“ (nach „von“), „chat:x“ → „Chat x“. */
export function herkunftWort(von: string): string {
	if (von === 'browser') return 'dir (Browser)';
	if (von === 'server') return 'Server';
	if (von === 'baumeister') return 'Baumeister';
	if (von.startsWith('chat:')) return `Chat ${von.slice(5)}`;
	return von;
}

function gleicherTag(a: Date, b: Date): boolean {
	return (
		a.getFullYear() === b.getFullYear() &&
		a.getMonth() === b.getMonth() &&
		a.getDate() === b.getDate()
	);
}

function istGestern(d: Date, now: Date): boolean {
	const gestern = new Date(now);
	gestern.setDate(now.getDate() - 1);
	return gleicherTag(d, gestern);
}

function datum(d: Date): string {
	return d.toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' });
}

function uhrzeit(d: Date): string {
	return d.toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' });
}

/**
 * Relative Zeit, kurz: „gerade eben“, „vor 3 Min.“, „vor 2 Std.“, „gestern“,
 * sonst das Datum „24.10.2026“. Unlesbares bleibt wie es ist.
 */
export function fmtRelativ(iso: string | null | undefined, now: Date = new Date()): string {
	if (iso === null || iso === undefined || iso === '') return '–';
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return iso;
	const ms = Math.max(0, now.getTime() - d.getTime());
	if (ms < MINUTE) return 'gerade eben';
	if (ms < STUNDE) return `vor ${Math.floor(ms / MINUTE)} Min.`;
	if (ms < TAG) return `vor ${Math.floor(ms / STUNDE)} Std.`;
	if (istGestern(d, now)) return 'gestern';
	return datum(d);
}

/** Zeitpunkt, kurz: „09:14 Uhr“ (heute), „gestern 09:14 Uhr“, sonst „24.10.2026, 09:14 Uhr“. */
export function fmtZeitpunkt(iso: string | null | undefined, now: Date = new Date()): string {
	if (iso === null || iso === undefined || iso === '') return '–';
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return iso;
	if (gleicherTag(d, now)) return `${uhrzeit(d)} Uhr`;
	if (istGestern(d, now)) return `gestern ${uhrzeit(d)} Uhr`;
	return `${datum(d)}, ${uhrzeit(d)} Uhr`;
}

/** „zugestellt vor 3 Min.“, „entstanden gestern“, „erledigt am 24.10.2026“. */
export function seitText(v: Vorgang, now: Date = new Date()): string {
	const verb = v.zustand === 'offen' ? 'entstanden' : zustandWort(v.zustand);
	const wann = fmtRelativ(v.geaendert, now);
	return /^\d/.test(wann) ? `${verb} am ${wann}` : `${verb} ${wann}`;
}

/** Rot: länger als einen Tag zugestellt, ohne übernommen zu sein. */
export function istUeberfaellig(v: Vorgang, now: Date = new Date()): boolean {
	if (v.zustand !== 'zugestellt') return false;
	const t = new Date(v.geaendert).getTime();
	if (Number.isNaN(t)) return false;
	return now.getTime() - t > TAG;
}
