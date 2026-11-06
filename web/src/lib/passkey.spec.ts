import { describe, expect, it, vi } from 'vitest';
import {
	anlegen,
	anmeldeArt,
	anmelden,
	aufLocalhost,
	b64,
	b64Lesen,
	browserFehlerSatz,
	entscheiden,
	ladeAnzahl,
	vonWort,
	zielPruefen,
	type Umgebung
} from './passkey.ts';
import { ApiError } from './api/client.ts';

function antwort(status: number, body: unknown): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

function bytes(...b: number[]): ArrayBuffer {
	return new Uint8Array(b).buffer;
}

interface Aufruf {
	url: string;
	body: unknown;
}

/** Ein `fetch`, das jeden Aufruf merkt und `antworte` fragt. */
function fakeFetch(aufrufe: Aufruf[], antworte: (url: string) => Response): typeof fetch {
	return (url: string | URL | Request, init?: RequestInit) => {
		const adresse = typeof url === 'string' ? url : url instanceof URL ? url.href : url.url;
		const body: unknown = typeof init?.body === 'string' ? JSON.parse(init.body) : null;
		aufrufe.push({ url: adresse, body });
		return Promise.resolve(antworte(adresse));
	};
}

type Optionen = CredentialRequestOptions | CredentialCreationOptions | undefined;

/** Ein Authenticator, der genau `cred` liefert und die Optionen merkt. */
function fakeCredentials(cred: unknown): {
	credentials: Umgebung['credentials'];
	optionen: Optionen[];
} {
	const optionen: Optionen[] = [];
	const liefern = (o: Optionen): Promise<Credential | null> => {
		optionen.push(o);
		return Promise.resolve(cred as Credential | null);
	};
	return { optionen, credentials: { get: liefern, create: liefern } };
}

describe('Base64url', () => {
	it('kodiert ohne Füllzeichen und mit - und _', () => {
		expect(b64(new Uint8Array([0xfb, 0xff]))).toBe('-_8');
		expect(b64(bytes(1, 2, 3))).toBe('AQID');
		expect(b64(new Uint8Array([]))).toBe('');
	});
	it('liest hin und zurück', () => {
		const roh = new Uint8Array([0, 1, 2, 250, 251, 252, 253, 254, 255]);
		expect(Array.from(b64Lesen(b64(roh)))).toEqual(Array.from(roh));
		expect(Array.from(b64Lesen('AQ=='))).toEqual([1]);
	});
	it('wirft bei fremden Zeichen', () => {
		expect(() => b64Lesen('a+b/')).toThrow();
		expect(() => b64Lesen('a b')).toThrow();
	});
});

describe('Ziel nach der Anmeldung', () => {
	it('nur Pfade auf dem Dashboard selbst', () => {
		expect(zielPruefen('/vorgaenge?x=1')).toBe('/vorgaenge?x=1');
		expect(zielPruefen('/schluessel')).toBe('/schluessel');
		for (const fremd of [
			null,
			'',
			'https://boese.example/',
			'//boese.example',
			'/\\boese.example',
			'javascript:alert(1)',
			'vorgaenge',
			'/anmelden',
			'/anmelden?ziel=/x',
			'/a\nb'
		]) {
			expect(zielPruefen(fremd), String(fremd)).toBe('/');
		}
	});
	it('erkennt localhost', () => {
		expect(aufLocalhost('http://localhost:8787/anmelden')).toBe(true);
		expect(aufLocalhost('http://127.0.0.1:8787/anmelden')).toBe(false);
		expect(aufLocalhost('kaputt')).toBe(false);
	});
});

describe('Sätze', () => {
	it('für Fehler des Browsers', () => {
		expect(browserFehlerSatz(new DOMException('x', 'NotAllowedError'))).toMatch(/Abgebrochen/);
		expect(browserFehlerSatz(new DOMException('x', 'SecurityError'))).toMatch(/localhost/);
		expect(browserFehlerSatz(new DOMException('x', 'InvalidStateError'))).toMatch(/schon/);
		expect(browserFehlerSatz(new Error('x'))).toMatch(/nicht benutzen/);
	});
	it('wer entschieden hat', () => {
		expect(vonWort('hauptkonto')).toBe('Hauptkonto');
		expect(vonWort('passkey:Handy')).toBe('Passkey „Handy“');
		expect(vonWort('server')).toMatch(/24 Stunden/);
		expect(vonWort(null)).toBeNull();
	});
});

describe('Anmeldung auf /anmelden (Auflage aus Hotfix 3a-1)', () => {
	const stand = (wie: 'hauptkonto' | 'passkey' | 'keine') => ({
		wie,
		passkey: wie === 'passkey' ? 'Handy' : null,
		sitzung_bis: null,
		frisch_bis: null,
		verfuegbar: true
	});
	const eigene = (hauptkonto: boolean) => ({
		konto: 'Konto',
		hauptkonto,
		passkeys: [],
		anmeldung: hauptkonto ? ('angemeldet' as const) : ('nicht_registriert' as const)
	});

	it('frische Verbindung: Stand sagt keine, die geschützten Anträge sagen Hauptkonto', () => {
		expect(anmeldeArt(stand('keine'), eigene(true))).toBe('hauptkonto');
		// Die Anträge kommen auch ohne Stand an.
		expect(anmeldeArt(null, eigene(true))).toBe('hauptkonto');
	});

	it('sonst gilt der Stand; vor dem Laden nichts', () => {
		expect(anmeldeArt(stand('passkey'), eigene(false))).toBe('passkey');
		expect(anmeldeArt(stand('keine'), eigene(false))).toBe('keine');
		expect(anmeldeArt(stand('hauptkonto'), null)).toBe('hauptkonto');
		expect(anmeldeArt(null, null)).toBeNull();
	});
});

describe('Anmelden mit Passkey', () => {
	it('holt die Challenge, fragt den Authenticator und schickt die Antwort', async () => {
		const aufrufe: Aufruf[] = [];
		const f = fakeFetch(aufrufe, (url) =>
			url.endsWith('/beginn')
				? antwort(200, { challenge: 'AQID', rp_id: 'localhost', zeitlimit_ms: 300000 })
				: antwort(200, {
						wie: 'passkey',
						passkey: 'Handy',
						sitzung_bis: null,
						frisch_bis: null,
						verfuegbar: true
					})
		);
		const { credentials, optionen } = fakeCredentials({
			rawId: bytes(9, 9),
			response: {
				clientDataJSON: bytes(1),
				authenticatorData: bytes(2),
				signature: bytes(3),
				userHandle: bytes(4)
			}
		});
		const s = await anmelden({ fetch: f, credentials });
		expect(s.wie).toBe('passkey');
		const pk = (optionen[0] as CredentialRequestOptions | undefined)?.publicKey;
		expect(Array.from(new Uint8Array(pk?.challenge as ArrayBuffer))).toEqual([1, 2, 3]);
		expect(pk?.userVerification).toBe('required');
		expect(pk?.rpId).toBe('localhost');
		expect(aufrufe.map((a) => a.url)).toEqual([
			'/api/passkey/anmelden/beginn',
			'/api/passkey/anmelden/ende'
		]);
		expect(aufrufe[1]?.body).toEqual({
			id: 'CQk',
			client_daten: 'AQ',
			auth_daten: 'Ag',
			signatur: 'Aw',
			benutzer: 'BA'
		});
	});

	it('Abbruch im Dialog wird ein Fehler des Browsers', async () => {
		const f = fakeFetch([], () =>
			antwort(200, { challenge: 'AQID', rp_id: 'localhost', zeitlimit_ms: 1 })
		);
		const { credentials } = fakeCredentials(null);
		await expect(anmelden({ fetch: f, credentials })).rejects.toMatchObject({
			name: 'NotAllowedError'
		});
	});

	it('Fehler des Servers kommen als ApiError mit Satz', async () => {
		const f = fakeFetch([], () =>
			antwort(503, { fehler: 'Nicht eingerichtet.', art: 'speicher-fehlt' })
		);
		const { credentials } = fakeCredentials(null);
		await expect(anmelden({ fetch: f, credentials })).rejects.toBeInstanceOf(ApiError);
	});
});

describe('Passkey anlegen', () => {
	it('verlangt Nutzerprüfung, keine Bescheinigung und schickt die Antwort', async () => {
		const aufrufe: Aufruf[] = [];
		const f = fakeFetch(aufrufe, (url) =>
			url.endsWith('/beginn')
				? antwort(200, {
						challenge: 'AQID',
						rp_id: 'localhost',
						rp_name: 'Dashboard',
						benutzer_id: 'BAU',
						benutzer_name: 'PC\\anders',
						anzeige_name: 'Dashboard (Handy)',
						verfahren: [-7, -257],
						ausschliessen: ['CQk'],
						zeitlimit_ms: 300000,
						hauptkonto: false
					})
				: antwort(201, { status: 'ausstehend', vergleichszahl: '0427', ablauf: null })
		);
		const { credentials, optionen } = fakeCredentials({
			rawId: bytes(9, 9),
			response: { clientDataJSON: bytes(1), attestationObject: bytes(5) }
		});
		const e = await anlegen('Handy', { fetch: f, credentials });
		expect(e).toEqual({ status: 'ausstehend', vergleichszahl: '0427', ablauf: null });
		const pk = (optionen[0] as CredentialCreationOptions | undefined)?.publicKey;
		expect(pk?.attestation).toBe('none');
		expect(pk?.authenticatorSelection?.userVerification).toBe('required');
		expect(pk?.authenticatorSelection?.residentKey).toBe('required');
		expect(pk?.pubKeyCredParams.map((p) => p.alg)).toEqual([-7, -257]);
		expect(pk?.user.name).toBe('PC\\anders');
		expect(pk?.excludeCredentials).toHaveLength(1);
		expect(aufrufe.map((a) => a.body)).toEqual([
			{ name: 'Handy' },
			{ id: 'CQk', client_daten: 'AQ', attestation: 'BQ' }
		]);
	});
});

describe('Verwaltung', () => {
	it('freigeben schickt die verglichene Zahl, ablehnen nichts', async () => {
		const aufrufe: Aufruf[] = [];
		const f = fakeFetch(aufrufe, () => antwort(200, { id: 'x', name: 'Handy' }));
		await entscheiden('a-b_c', 'freigeben', '0427', f);
		await entscheiden('a-b_c', 'ablehnen', null, f);
		expect(aufrufe).toEqual([
			{ url: '/api/schluessel/passkeys/a-b_c/freigeben', body: { vergleichszahl: '0427' } },
			{ url: '/api/schluessel/passkeys/a-b_c/ablehnen', body: {} }
		]);
	});

	it('die Zahl der Anträge ist ohne Anmeldung null', async () => {
		const f = vi.fn(() =>
			Promise.resolve(antwort(401, { fehler: 'Bitte anmelden', art: 'passkey' }))
		);
		expect(await ladeAnzahl(f)).toBeNull();
		const g = vi.fn(() => Promise.resolve(antwort(200, { offen: 2 })));
		expect(await ladeAnzahl(g)).toBe(2);
	});
});
