/**
 * Passkey für andere Konten (Etappe 7c): Anmelden mit Passkey, Antrag aus
 * einem anderen Windows-Konto, Verwaltung auf /schluessel.
 *
 * Der Browser spricht mit dem Authenticator über `navigator.credentials`
 * (Windows Hello, Handy, Sicherheitsschlüssel); alle binären Werte gehen als
 * Base64url an den Rust-Server, der sie selbst prüft. Der Ausweis danach
 * steckt in einem HttpOnly-Cookie: Diese Datei sieht ihn nie.
 */
import { API, readError } from './api/client.ts';
import type {
	AnmeldeArt,
	AnmeldeOptionen,
	AntragErgebnis,
	AntragOptionen,
	EigeneAntraege,
	PasskeyAnzahl,
	PasskeyListe,
	PasskeyStand,
	PasskeyStatus,
	PasskeyZeile
} from '$vertrag';

/** Passkeys gehen nur über diesen Namen, nicht über 127.0.0.1. */
export const PASSKEY_HOST = 'localhost';

/** Bytes als Base64url ohne Füllzeichen. */
export function b64(bytes: ArrayBuffer | Uint8Array): string {
	const b = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
	let s = '';
	for (const x of b) s += String.fromCharCode(x);
	return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/** Base64url (mit oder ohne Füllzeichen) als Bytes; unlesbar wirft. */
export function b64Lesen(text: string): Uint8Array<ArrayBuffer> {
	if (!/^[A-Za-z0-9_-]*={0,2}$/.test(text)) throw new Error('Kein Base64url');
	const roh = atob(text.replace(/-/g, '+').replace(/_/g, '/').replace(/=+$/, ''));
	const out = new Uint8Array(roh.length);
	for (let i = 0; i < roh.length; i++) out[i] = roh.charCodeAt(i);
	return out;
}

/**
 * Wohin nach der Anmeldung: nur ein Pfad auf dem Dashboard selbst (beginnt
 * mit genau einem `/`), sonst die Startseite. So führt `?ziel=` nie auf eine
 * fremde Seite.
 */
export function zielPruefen(ziel: string | null | undefined): string {
	if (!ziel || !ziel.startsWith('/') || ziel.startsWith('//') || ziel.includes('\\')) return '/';
	for (let i = 0; i < ziel.length; i++) if (ziel.charCodeAt(i) < 32) return '/';
	if (ziel === '/anmelden' || ziel.startsWith('/anmelden?')) return '/';
	return ziel;
}

/** Ist die Seite über localhost offen (sonst gehen Passkeys nicht)? */
export function aufLocalhost(href: string): boolean {
	try {
		return new URL(href).hostname === PASSKEY_HOST;
	} catch {
		return false;
	}
}

/** Satz zu einem Fehler des Browsers bei `navigator.credentials`. */
export function browserFehlerSatz(e: unknown): string {
	const name = e instanceof DOMException || e instanceof Error ? e.name : '';
	switch (name) {
		case 'NotAllowedError':
			return 'Abgebrochen oder die Zeit ist abgelaufen. Bitte noch einmal versuchen.';
		case 'InvalidStateError':
			return 'Auf diesem Gerät gibt es für dieses Konto schon einen Passkey.';
		case 'SecurityError':
			return 'Passkeys gehen nur unter http://localhost, nicht unter 127.0.0.1.';
		case 'NotSupportedError':
			return 'Dieser Browser oder dieses Gerät kann keine Passkeys.';
		case 'AbortError':
			return 'Abgebrochen.';
		default:
			return 'Der Passkey ließ sich nicht benutzen. Bitte noch einmal versuchen.';
	}
}

/** Was die Seiten brauchen: `fetch` und `navigator.credentials` (für Tests austauschbar). */
export interface Umgebung {
	fetch: typeof fetch;
	credentials: Pick<CredentialsContainer, 'create' | 'get'>;
}

function standard(): Umgebung {
	return { fetch: (...a) => fetch(...a), credentials: navigator.credentials };
}

async function senden<T>(f: typeof fetch, pfad: string, koerper: unknown): Promise<T> {
	const res = await f(`${API}${pfad}`, {
		method: 'POST',
		credentials: 'same-origin',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify(koerper)
	});
	if (!res.ok) throw await readError(res);
	if (res.status === 204) return undefined as T;
	return (await res.json()) as T;
}

async function holen<T>(f: typeof fetch, pfad: string): Promise<T> {
	const res = await f(`${API}${pfad}`, { headers: { accept: 'application/json' } });
	if (!res.ok) throw await readError(res);
	return (await res.json()) as T;
}

/** Ist diese Sitzung angemeldet, und wie? */
export function stand(f: typeof fetch = fetch): Promise<PasskeyStand> {
	return holen<PasskeyStand>(f, '/passkey/stand');
}

/** Die Passkeys des eigenen Windows-Kontos (meldet bei Bedarf bei Windows an). */
export function eigeneAntraege(f: typeof fetch = fetch): Promise<EigeneAntraege> {
	return holen<EigeneAntraege>(f, '/passkey/antrag');
}

/**
 * Wie die Seite /anmelden die Anmeldung beschreibt (Auflage aus Hotfix 3a-1).
 * `/api/passkey/stand` ist frei und sieht die Windows-Anmeldung nur auf einer
 * schon angemeldeten Verbindung (Edge meldet erst nach einer 401 an); auf
 * einer frischen Verbindung sagt er `keine`, auch im Hauptkonto. Die eigenen
 * Anträge (`/api/passkey/antrag`) sind geschützt und kommen darum immer mit
 * Anmeldung: `hauptkonto` dort ist sicher. `null`: noch nichts geladen.
 */
export function anmeldeArt(
	stand: PasskeyStand | null,
	eigene: EigeneAntraege | null
): AnmeldeArt | null {
	if (eigene?.hauptkonto === true) return 'hauptkonto';
	return stand?.wie ?? null;
}

/**
 * Mit einem registrierten Passkey anmelden. Danach setzt der Server das
 * Cookie; die Antwort sagt, bis wann die Sitzung geht.
 */
export async function anmelden(u: Umgebung = standard()): Promise<PasskeyStand> {
	const o = await senden<AnmeldeOptionen>(u.fetch, '/passkey/anmelden/beginn', {});
	const cred = (await u.credentials.get({
		publicKey: {
			challenge: b64Lesen(o.challenge),
			rpId: o.rp_id,
			timeout: o.zeitlimit_ms,
			userVerification: 'required',
			allowCredentials: []
		}
	})) as PublicKeyCredential | null;
	if (cred === null) throw new DOMException('Kein Passkey gewählt', 'NotAllowedError');
	const r = cred.response as AuthenticatorAssertionResponse;
	const koerper: Record<string, string> = {
		id: b64(cred.rawId),
		client_daten: b64(r.clientDataJSON),
		auth_daten: b64(r.authenticatorData),
		signatur: b64(r.signature)
	};
	if (r.userHandle !== null && r.userHandle.byteLength > 0) koerper.benutzer = b64(r.userHandle);
	return senden<PasskeyStand>(u.fetch, '/passkey/anmelden/ende', koerper);
}

/**
 * Einen Passkey anlegen. Im Hauptkonto ist er sofort registriert, aus einem
 * anderen Konto ein Antrag mit Vergleichszahl.
 */
export async function anlegen(name: string, u: Umgebung = standard()): Promise<AntragErgebnis> {
	const o = await senden<AntragOptionen>(u.fetch, '/passkey/antrag/beginn', { name });
	const cred = (await u.credentials.create({
		publicKey: {
			challenge: b64Lesen(o.challenge),
			rp: { id: o.rp_id, name: o.rp_name },
			user: { id: b64Lesen(o.benutzer_id), name: o.benutzer_name, displayName: o.anzeige_name },
			pubKeyCredParams: o.verfahren.map((alg) => ({ type: 'public-key' as const, alg })),
			timeout: o.zeitlimit_ms,
			attestation: 'none',
			authenticatorSelection: {
				residentKey: 'required',
				requireResidentKey: true,
				userVerification: 'required'
			},
			excludeCredentials: o.ausschliessen.map((id) => ({
				type: 'public-key' as const,
				id: b64Lesen(id)
			}))
		}
	})) as PublicKeyCredential | null;
	if (cred === null) throw new DOMException('Kein Passkey angelegt', 'NotAllowedError');
	const r = cred.response as AuthenticatorAttestationResponse;
	return senden<AntragErgebnis>(u.fetch, '/passkey/antrag/ende', {
		id: b64(cred.rawId),
		client_daten: b64(r.clientDataJSON),
		attestation: b64(r.attestationObject)
	});
}

/** Abmelden: Ausweis sperren, Cookie löschen. */
export async function abmelden(f: typeof fetch = fetch): Promise<void> {
	await senden<unknown>(f, '/passkey/abmelden', {});
}

// ----- Verwaltung (nur als Inhaber) -----

export function liste(f: typeof fetch = fetch): Promise<PasskeyListe> {
	return holen<PasskeyListe>(f, '/schluessel/passkeys');
}

/** Zahl der offenen Anträge; ohne Anmeldung oder bei Fehlern `null`. */
export async function ladeAnzahl(f: typeof fetch = fetch): Promise<number | null> {
	try {
		return (await holen<PasskeyAnzahl>(f, '/schluessel/passkeys/anzahl')).offen;
	} catch {
		return null;
	}
}

export type Entscheidung = 'freigeben' | 'ablehnen' | 'widerrufen';

/** Freigeben (mit der verglichenen Zahl), ablehnen oder widerrufen. Wirft `ApiError`. */
export function entscheiden(
	id: string,
	was: Entscheidung,
	vergleichszahl: string | null,
	f: typeof fetch = fetch
): Promise<PasskeyZeile> {
	const koerper = was === 'freigeben' ? { vergleichszahl } : {};
	return senden<PasskeyZeile>(f, `/schluessel/passkeys/${encodeURIComponent(id)}/${was}`, koerper);
}

/** Alle Sitzungen mit Passkey beenden (auch die eigene). */
export async function sitzungenBeenden(f: typeof fetch = fetch): Promise<void> {
	await senden<unknown>(f, '/schluessel/sitzungen-beenden', {});
}

/** Stand in einfachen Worten. */
export const STATUS_WORT: Record<PasskeyStatus, string> = {
	ausstehend: 'wartet auf Freigabe',
	registriert: 'registriert',
	widerrufen: 'widerrufen',
	abgelehnt: 'abgelehnt',
	abgelaufen: 'abgelaufen'
};

/** Wer entschieden hat, lesbar: `hauptkonto` → „Hauptkonto“, `passkey:Handy` → „Passkey Handy“. */
export function vonWort(von: string | null): string | null {
	if (von === null) return null;
	if (von === 'hauptkonto') return 'Hauptkonto';
	if (von === 'server') return 'Ablauf nach 24 Stunden';
	if (von.startsWith('passkey:')) return `Passkey „${von.slice(8)}“`;
	return von;
}
