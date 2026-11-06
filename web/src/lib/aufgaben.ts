/**
 * Aufgaben für den Inhaber (Etappe 5a): laden, abhaken, Frist in Worten.
 *
 * `/api/aufgaben` ist geschützt (Windows-Anmeldung), die Startseite aber
 * frei. Darum lädt der Kasten „Für dich“ mit eigenem `fetch` (mit
 * Anmeldedaten) und behandelt 401 als „nicht angemeldet“ statt als Fehler.
 */
import { API, readError } from './api/client';
import { BROWSER_ADRESSE, SATZ_FREMDES_KONTO } from './anmeldung.ts';
import type {
	AufgabeAnsicht,
	AufgabeGeaendert,
	AufgabenAnzahl,
	AufgabenListe,
	FristStufe
} from '$vertrag';

/** Wie es um die Liste steht. */
export type AufgabenStand = 'laedt' | 'da' | 'ohne-anmeldung' | 'fremdes-konto' | 'fehler';

export type AufgabenLadung =
	| { stand: 'da'; aufgaben: AufgabeAnsicht[]; fehler: null }
	| { stand: 'ohne-anmeldung' | 'fremdes-konto' | 'fehler'; aufgaben: []; fehler: string };

export const SATZ_OHNE_ANMELDUNG = `Aufgaben siehst du in Microsoft Edge unter ${BROWSER_ADRESSE}`;

/**
 * Holt die offenen Aufgaben. Wirft nie: 401, 403 und andere Fehler kommen als Stand zurück.
 * Ohne `projekt` fehlen die Aufgaben archivierter Projekte (Entscheidung
 * archiv-status); mit `projekt` nur die dieses Projekts, auch archiviert.
 */
export async function ladeAufgaben(
	fetchFn: typeof fetch = fetch,
	projekt: string | null = null
): Promise<AufgabenLadung> {
	const nur = projekt === null ? '' : `&projekt=${encodeURIComponent(projekt)}`;
	try {
		const res = await fetchFn(`${API}/aufgaben?offen=1${nur}`, {
			credentials: 'same-origin',
			headers: { accept: 'application/json' }
		});
		if (res.status === 401)
			return { stand: 'ohne-anmeldung', aufgaben: [], fehler: SATZ_OHNE_ANMELDUNG };
		if (!res.ok) {
			const e = await readError(res);
			if (e.art === 'fremdes-konto')
				return { stand: 'fremdes-konto', aufgaben: [], fehler: SATZ_FREMDES_KONTO };
			return { stand: 'fehler', aufgaben: [], fehler: e.message };
		}
		const liste = (await res.json()) as AufgabenListe;
		return { stand: 'da', aufgaben: liste.aufgaben, fehler: null };
	} catch {
		return {
			stand: 'fehler',
			aufgaben: [],
			fehler: 'Die Aufgaben lassen sich gerade nicht laden.'
		};
	}
}

/**
 * Nur die Zahl der offenen Aufgaben (für den Seitenkopf), über
 * `/api/aufgaben/anzahl`: Der Server zählt, ohne einen Link zu entschlüsseln.
 * Wirft nie; ohne Anmeldung, bei Fehlern oder ohne Netz `null`.
 */
export async function ladeAnzahl(fetchFn: typeof fetch = fetch): Promise<number | null> {
	try {
		const res = await fetchFn(`${API}/aufgaben/anzahl`, {
			credentials: 'same-origin',
			headers: { accept: 'application/json' }
		});
		if (!res.ok) return null;
		const z = (await res.json()) as AufgabenAnzahl;
		return typeof z.offen === 'number' ? z.offen : null;
	} catch {
		return null;
	}
}

/** Haken des Inhabers, wahlweise mit der Antwort eines Knopfs. Wirft `ApiError`. */
export async function abhaken(
	id: string,
	antwort: string | null,
	fetchFn: typeof fetch = fetch,
	eigene: string | null = null
): Promise<AufgabeGeaendert> {
	const res = await fetchFn(`${API}/aufgaben/${encodeURIComponent(id)}/erledigt`, {
		method: 'POST',
		credentials: 'same-origin',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify(
			antwort === null ? {} : eigene === null ? { antwort } : { antwort, eigene }
		)
	});
	if (!res.ok) throw await readError(res);
	return (await res.json()) as AufgabeGeaendert;
}

/** Unsichtbare Formatzeichen (Unicode-Kategorie Cf), Leer- und Steuerzeichen. */
const UNSICHTBAR = /[\p{Cf}\p{Cc}\s]/u;

/** Port hinter der Adresse: fehlt er, gut; sonst 1 bis 5 Ziffern bis 65535. Gibt den Host zurück. */
function hostOhnePort(adresse: string): string | null {
	let host = adresse;
	let port: string | null = null;
	if (adresse.startsWith('[')) {
		const i = adresse.indexOf(']');
		if (i < 0) return null;
		const rest = adresse.slice(i + 1);
		host = adresse.slice(0, i + 1);
		if (rest !== '') {
			if (!rest.startsWith(':')) return null;
			port = rest.slice(1);
		}
	} else {
		const i = adresse.lastIndexOf(':');
		if (i >= 0) {
			host = adresse.slice(0, i);
			port = adresse.slice(i + 1);
		}
	}
	if (port !== null && !(/^\d{1,5}$/.test(port) && Number(port) <= 65535)) return null;
	return host === '' ? null : host;
}

/**
 * Nur Links, die der Server auch annimmt (dieselbe Regel wie `link_pruefen`
 * in src/api/aufgaben.rs): https oder http://localhost, ohne unsichtbare
 * Zeichen (Kategorie Cf, etwa U+200B oder eine Richtungsumkehr), ohne Leer-
 * und Steuerzeichen, ohne Benutzernamen vor der Adresse, Port höchstens
 * 65535. Alles andere zeigt die Seite nicht als Link, auch wenn es irgendwie
 * hereinkäme.
 */
export function sichererLink(link: string | null): string | null {
	if (link === null || UNSICHTBAR.test(link)) return null;
	const m = /^(https?):\/\/([^/?#]*)(.*)$/i.exec(link);
	if (!m) return null;
	const art = (m[1] ?? '').toLowerCase();
	const adresse = m[2] ?? '';
	if (adresse === '' || adresse.includes('@') || adresse.includes('\\')) return null;
	const host = hostOhnePort(adresse);
	if (host === null) return null;
	if (art === 'http' && host.toLowerCase() !== 'localhost') return null;
	try {
		// Was der Browser nicht auflösen kann, ist kein Link.
		if (new URL(link).host === '') return null;
	} catch {
		return null;
	}
	return link;
}

/**
 * Das Ziel eines Links zum Anzeigen neben „Öffnen“: der Host, wie der Browser
 * ihn auflöst (`new URL(link).host`, mit Port, wenn einer dasteht). Ein Name
 * mit Umlauten oder fremden Schriftzeichen erscheint in der ASCII-Form
 * (Punycode, `xn--…`), so wie der Browser ihn wirklich aufruft; so fällt ein
 * nachgemachter Name auf. Ohne sicheren Link `null`.
 */
export function linkZiel(link: string | null): string | null {
	const sicher = sichererLink(link);
	if (sicher === null) return null;
	try {
		return new URL(sicher).host || null;
	} catch {
		return null;
	}
}

/**
 * Frist heute oder vorbei: in der Farbe für „Achtung“. Die Stufe rechnet der
 * Server (`frist_stufe`, Etappe 3a+), der Browser vergleicht keine Daten.
 */
export function fristAchtung(stufe: FristStufe | null): boolean {
	return stufe === 'heute' || stufe === 'ueberfaellig';
}

/** `2026-10-30` → `30.10.` */
function kurz(frist: string): string {
	const [, m, t] = frist.split('-');
	return `${t ?? ''}.${m ?? ''}.`;
}

/** Frist in Worten aus der Stufe des Servers: „heute fällig“, „überfällig seit 27.10.“, „bis 30.10.“. */
export function fristWort(frist: string | null, stufe: FristStufe | null): string | null {
	if (frist === null) return null;
	switch (stufe) {
		case 'heute':
			return 'heute fällig';
		case 'ueberfaellig':
			return `überfällig seit ${kurz(frist)}`;
		case 'bald':
		case 'spaeter':
		case null:
			return `bis ${kurz(frist)}`;
	}
}

/** Der Chat, der die Aufgabe angelegt hat: „chat:dashboard“ → „Chat dashboard“. */
export function chatWort(von: string): string {
	if (von.startsWith('chat:')) return `Chat ${von.slice(5)}`;
	if (von === 'chat') return 'ein Chat';
	if (von === 'browser') return 'du selbst';
	return von;
}

/**
 * Was der Kasten „Für dich“ zählt, in Worten (Etappe 3b-2, Abnahme 3b,
 * `entscheidung_startseite`): nur offene Aufgaben, keine Fragen und keine
 * Arbeiten. Vorher stand dort eine nackte Zahl neben der Überschrift.
 */
export function aufgabenZahlWort(n: number): string {
	return n === 1 ? '1 offene Aufgabe' : `${String(n)} offene Aufgaben`;
}
