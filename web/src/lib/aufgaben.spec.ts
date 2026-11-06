import { describe, expect, it, vi } from 'vitest';
import {
	SATZ_OHNE_ANMELDUNG,
	abhaken,
	aufgabenZahlWort,
	chatWort,
	fristAchtung,
	fristWort,
	ladeAnzahl,
	ladeAufgaben,
	linkZiel,
	sichererLink
} from './aufgaben.ts';
import { Aufgaben, statusKannSichAendern } from './aufgaben.svelte.ts';
import { live } from './live.svelte.ts';
import { PUFFER_MS } from './tageswechsel.ts';
import type { AufgabeAnsicht, ChangeEvent } from '$vertrag';

function aufgabe(teil: Partial<AufgabeAnsicht> = {}): AufgabeAnsicht {
	return {
		id: 'a-20261028-0001',
		projekt: 'dashboard',
		von: 'chat:dashboard',
		titel: 'Bei Probe-Dienst anmelden',
		knopf: null,
		frist: null,
		ablauf: null,
		zustand: 'offen',
		antwort: null,
		eigene: null,
		erstellt: '2026-10-28T08:00:00+01:00',
		geaendert: '2026-10-28T08:00:00+01:00',
		hat_link: false,
		vorgang: null,
		link: null,
		frist_stufe: null,
		...teil
	};
}

function antwort(status: number, body: unknown): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

describe('Frist (Etappe 3a+: Stufe vom Server)', () => {
	it('heute, überfällig, bald, später, ohne: Worte und Achtung nur aus der Stufe', () => {
		expect(fristWort('2026-10-28', 'heute')).toBe('heute fällig');
		expect(fristWort('2026-10-27', 'ueberfaellig')).toBe('überfällig seit 27.10.');
		expect(fristWort('2026-11-02', 'bald')).toBe('bis 02.11.');
		expect(fristWort('2026-12-24', 'spaeter')).toBe('bis 24.12.');
		expect(fristWort(null, null)).toBeNull();
		expect([fristAchtung('heute'), fristAchtung('ueberfaellig')]).toEqual([true, true]);
		expect([fristAchtung('bald'), fristAchtung('spaeter'), fristAchtung(null)]).toEqual([
			false,
			false,
			false
		]);
	});
});

describe('sichererLink', () => {
	it('nur https und http://localhost', () => {
		expect(sichererLink('https://beispiel.invalid/a?b=1')).toBe('https://beispiel.invalid/a?b=1');
		expect(sichererLink('http://localhost:8787/daten')).toBe('http://localhost:8787/daten');
		expect(sichererLink('http://localhost')).toBe('http://localhost');
		for (const schlecht of [
			'javascript:alert(1)',
			'data:text/html,x',
			'file:///C:/x',
			'http://127.0.0.1:8787/',
			'http://beispiel.invalid/',
			'https://nutzer@beispiel.invalid/',
			'http://localhost.beispiel.invalid/'
		]) {
			expect(sichererLink(schlecht), schlecht).toBeNull();
		}
		expect(sichererLink(null)).toBeNull();
	});

	it('lehnt unsichtbare Zeichen (Kategorie Cf) ab, wie der Server', () => {
		const z = (n: number) => String.fromCodePoint(n);
		for (const n of [
			0x200b, 0x200e, 0x200f, 0x202a, 0x202e, 0x2066, 0x2069, 0xfeff, 0x00ad, 0xe0041
		]) {
			const link = `https://konto.beispiel.invalid${z(n)}.fremd.invalid/`;
			expect(sichererLink(link), n.toString(16)).toBeNull();
			expect(linkZiel(link), n.toString(16)).toBeNull();
		}
		expect(sichererLink(`https://beispiel.invalid/a${z(0x202e)}fdp.exe`)).toBeNull();
		expect(sichererLink('https://beispiel.invalid/a\tb')).toBeNull();
	});

	it('Ports nur bis 65535, wie der Server', () => {
		expect(sichererLink('https://beispiel.invalid:65535/x')).toBe(
			'https://beispiel.invalid:65535/x'
		);
		expect(sichererLink('http://localhost:8787/')).toBe('http://localhost:8787/');
		expect(sichererLink('https://[::1]:443/')).toBe('https://[::1]:443/');
		for (const schlecht of [
			'https://beispiel.invalid:65536/',
			'https://beispiel.invalid:99999999/',
			'https://beispiel.invalid:abc/',
			'https://beispiel.invalid:/',
			'http://localhost:70000/',
			'https://[::1]:65536/'
		]) {
			expect(sichererLink(schlecht), schlecht).toBeNull();
		}
	});
});

describe('linkZiel', () => {
	it('zeigt den Host, wie der Browser ihn aufruft', () => {
		expect(linkZiel('https://account.example.org/login?token=erfunden')).toBe(
			'account.example.org'
		);
		expect(linkZiel('http://localhost:8787/daten')).toBe('localhost:8787');
		expect(linkZiel('https://Beispiel.INVALID/')).toBe('beispiel.invalid');
		expect(linkZiel(null)).toBeNull();
		expect(linkZiel('javascript:alert(1)')).toBeNull();
	});

	it('Umlaute und fremde Schrift in ASCII-Form (Punycode)', () => {
		expect(linkZiel('https://bücher.beispiel.invalid/')).toBe('xn--bcher-kva.beispiel.invalid');
		// Kyrillisches „а“ statt lateinischem „a“: fällt als xn-- auf.
		const nachgemacht = `https://${String.fromCodePoint(0x0430)}ccount.example.org/`;
		expect(linkZiel(nachgemacht)?.startsWith('xn--')).toBe(true);
	});
});

describe('chatWort', () => {
	it('macht aus der Herkunft Wörter', () => {
		expect(chatWort('chat:beispiel')).toBe('Chat beispiel');
		expect(chatWort('chat')).toBe('ein Chat');
	});
});

describe('aufgabenZahlWort (Etappe 3b-2)', () => {
	it('sagt in Worten, was „Für dich“ zählt', () => {
		expect(aufgabenZahlWort(1)).toBe('1 offene Aufgabe');
		expect(aufgabenZahlWort(16)).toBe('16 offene Aufgaben');
	});
});

describe('ladeAufgaben', () => {
	it('401 heißt nicht angemeldet, mit dem Satz für Edge', async () => {
		const f = vi.fn(() => Promise.resolve(antwort(401, { fehler: 'x', art: 'anmeldung' })));
		const l = await ladeAufgaben(f);
		expect(l).toEqual({ stand: 'ohne-anmeldung', aufgaben: [], fehler: SATZ_OHNE_ANMELDUNG });
		expect(SATZ_OHNE_ANMELDUNG).toBe(
			'Aufgaben siehst du in Microsoft Edge unter http://localhost:8787'
		);
		const [adresse, init] = f.mock.calls[0] as unknown as [string, RequestInit];
		expect(adresse).toBe('/api/aufgaben?offen=1');
		expect(init.credentials).toBe('same-origin');
	});

	it('fremdes Konto, Fehler und Netzfehler werfen nicht', async () => {
		const fremd = () =>
			Promise.resolve(
				antwort(403, { fehler: 'Dieses Windows-Konto darf hier nicht.', art: 'fremdes-konto' })
			);
		expect((await ladeAufgaben(fremd as unknown as typeof fetch)).stand).toBe('fremdes-konto');
		const kaputt = () => Promise.resolve(antwort(503, { fehler: 'Speicher fehlt.', art: 'x' }));
		expect(await ladeAufgaben(kaputt as unknown as typeof fetch)).toEqual({
			stand: 'fehler',
			aufgaben: [],
			fehler: 'Speicher fehlt.'
		});
		const netz = () => Promise.reject(new TypeError('weg'));
		expect((await ladeAufgaben(netz as unknown as typeof fetch)).stand).toBe('fehler');
	});

	it('liefert die Liste', async () => {
		const a = aufgabe();
		const f = () => Promise.resolve(antwort(200, { aufgaben: [a] }));
		expect(await ladeAufgaben(f as unknown as typeof fetch)).toEqual({
			stand: 'da',
			aufgaben: [a],
			fehler: null
		});
	});
});

describe('ladeAnzahl', () => {
	it('fragt nur die Zahl ab; Fehler und 401 werden null', async () => {
		const f = vi.fn(() => Promise.resolve(antwort(200, { offen: 3 })));
		expect(await ladeAnzahl(f)).toBe(3);
		const [adresse, init] = f.mock.calls[0] as unknown as [string, RequestInit];
		expect(adresse).toBe('/api/aufgaben/anzahl');
		expect(init.credentials).toBe('same-origin');
		const ohne = () => Promise.resolve(antwort(401, { fehler: 'x', art: 'anmeldung' }));
		expect(await ladeAnzahl(ohne as unknown as typeof fetch)).toBeNull();
		const netz = () => Promise.reject(new TypeError('weg'));
		expect(await ladeAnzahl(netz as unknown as typeof fetch)).toBeNull();
	});
});

describe('abhaken', () => {
	it('schickt JSON mit Antwort oder leer', async () => {
		const f = vi.fn(() =>
			Promise.resolve(antwort(200, { aufgabe: aufgabe({ zustand: 'erledigt' }), geaendert: true }))
		);
		await abhaken('a-1', 'ja', f);
		await abhaken('a-1', null, f);
		const aufrufe = f.mock.calls as unknown as [string, RequestInit][];
		expect(aufrufe[0]?.[0]).toBe('/api/aufgaben/a-1/erledigt');
		expect(aufrufe[0]?.[1].method).toBe('POST');
		expect(aufrufe[0]?.[1].body).toBe('{"antwort":"ja"}');
		expect(aufrufe[1]?.[1].body).toBe('{}');
		expect((aufrufe[0]?.[1].headers as Record<string, string>)['content-type']).toBe(
			'application/json'
		);
	});
});

describe('Aufgaben (gemeinsamer Stand)', () => {
	/** Antworten wie der Server: Liste, Zahl, Haken (422). */
	function server(liste: AufgabeAnsicht[], offen: number) {
		return vi.fn((adresse: string) =>
			Promise.resolve(
				adresse.endsWith('/erledigt')
					? antwort(422, { fehler: 'Diese Antwort passt nicht zur Aufgabe.', art: 'unlesbar' })
					: adresse.endsWith('/anzahl')
						? antwort(200, { offen })
						: antwort(200, { aufgaben: liste })
			)
		);
	}

	const adressen = (f: ReturnType<typeof server>) =>
		(f.mock.calls as unknown as [string][]).map((c) => c[0]);

	it('die Zahl kommt aus /anzahl; die Liste erst, wenn ein Kasten sie benutzt', async () => {
		const liste = [
			aufgabe({ link: 'https://beispiel.invalid/x' }),
			aufgabe({ id: 'a-20261028-0002', projekt: 'beispiel' })
		];
		const f = server(liste, 2);
		const a = new Aufgaben(f as unknown as typeof fetch);
		expect(a.anzahl).toBe(0);
		await a.zaehlen();
		expect(a.anzahl).toBe(2);
		expect(adressen(f)).toEqual(['/api/aufgaben/anzahl']);
		expect(a.stand).toBe('laedt');
		expect(a.liste).toEqual([]);
		const ab = a.benutzen();
		await vi.waitFor(() => {
			expect(a.stand).toBe('da');
		});
		// Ein zweiter Kasten lädt nicht noch einmal.
		const ab2 = a.benutzen();
		ab2();
		expect(adressen(f).filter((x) => x.includes('offen=1')).length).toBe(1);
		expect(a.stand).toBe('da');
		expect(a.fuer('beispiel').map((x) => x.id)).toEqual(['a-20261028-0002']);
		ab();
		ab();
		expect(a.benutzt).toBe(false);
	});

	it('nach dem Haken: Liste und Zahl neu', async () => {
		const f = server([aufgabe()], 1);
		const a = new Aufgaben(f as unknown as typeof fetch);
		await a.haken('a-20261028-0001', 'vielleicht');
		expect(a.meldung).toBe('Diese Antwort passt nicht zur Aufgabe.');
		expect(a.beschaeftigt).toBeNull();
		expect(a.anzahl).toBe(1);
		expect(adressen(f)).toEqual([
			'/api/aufgaben/a-20261028-0001/erledigt',
			'/api/aufgaben?offen=1',
			'/api/aufgaben/anzahl'
		]);
	});

	it('archiviertes Projekt (archiv-status): die Seite holt seine Aufgaben eigens', async () => {
		const eigene = aufgabe({ id: 'a-20261101-0001', projekt: 'stiller-umzug' });
		const f = server([eigene], 0);
		const a = new Aufgaben(f as unknown as typeof fetch);
		expect(a.fuer('stiller-umzug', true)).toEqual([]);
		await a.ladenFuer('stiller-umzug');
		expect(adressen(f)).toEqual(['/api/aufgaben?offen=1&projekt=stiller-umzug']);
		expect(a.fuer('stiller-umzug', true).map((x) => x.id)).toEqual(['a-20261101-0001']);
		// Nicht archiviert: wieder aus der allgemeinen Liste (die hier leer ist).
		expect(a.fuer('stiller-umzug')).toEqual([]);
		// Nach einem Haken kommt auch die eigene Liste neu.
		await a.haken('a-20261101-0001');
		expect(adressen(f).slice(1)).toEqual([
			'/api/aufgaben/a-20261101-0001/erledigt',
			'/api/aufgaben?offen=1',
			'/api/aufgaben/anzahl',
			'/api/aufgaben?offen=1&projekt=stiller-umzug'
		]);
	});

	it('ohne Anmeldung keine Zahl', async () => {
		const f = () => Promise.resolve(antwort(401, { fehler: 'x', art: 'anmeldung' }));
		const a = new Aufgaben(f);
		await a.zaehlen();
		await a.laden();
		expect(a.stand).toBe('ohne-anmeldung');
		expect(a.anzahl).toBe(0);
		expect(a.meldung).toBe(SATZ_OHNE_ANMELDUNG);
	});

	it('Etappe 3a+: um Mitternacht lädt die Liste neu und bekommt die neue Frist-Stufe', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		try {
			vi.setSystemTime(new Date(2026, 9, 29, 23, 59, 0));
			const f = vi.fn((adresse: string) => {
				const neu = new Date().getDate() === 30;
				return Promise.resolve(
					adresse.endsWith('/anzahl')
						? antwort(200, { offen: 1 })
						: antwort(200, {
								aufgaben: [aufgabe({ frist: '2026-10-30', frist_stufe: neu ? 'heute' : 'bald' })]
							})
				);
			});
			const a = new Aufgaben(f as unknown as typeof fetch);
			const ab = a.start();
			const weg = a.benutzen();
			await vi.waitFor(() => {
				expect(a.liste[0]?.frist_stufe).toBe('bald');
			});
			await vi.advanceTimersByTimeAsync(60_000 + PUFFER_MS);
			await vi.waitFor(() => {
				expect(a.liste[0]?.frist_stufe).toBe('heute');
			});
			weg();
			ab();
		} finally {
			vi.useRealTimers();
		}
	});
});

describe('Aufgaben folgen dem Archiv (Entscheidung archiv-status)', () => {
	const at = '2026-11-01T10:00:00+01:00';
	const ereignis = (area: ChangeEvent['area'], path: string): ChangeEvent => ({ area, path, at });

	it('ein geänderter Kopf oder „anstehend“ ohne Projekt kann den Status ändern', () => {
		expect(statusKannSichAendern(ereignis('projects', 'projects/fall/projekt.toml'))).toBe(true);
		expect(statusKannSichAendern(ereignis('projects', 'projects/fall/page.md'))).toBe(false);
		expect(statusKannSichAendern(ereignis('anstehend', 'anstehend'))).toBe(true);
		expect(statusKannSichAendern(ereignis('anstehend', 'anstehend/fall'))).toBe(false);
		expect(statusKannSichAendern(ereignis('projects', 'projects/fall/daten/fragen.toml'))).toBe(
			false
		);
		expect(statusKannSichAendern(ereignis('data', 'data/schwerpunkte.toml'))).toBe(false);
	});

	it('nach dem Archivieren und zurück: Liste und Zahl laden ohne Neuladen der Seite neu', async () => {
		let hoerer: (c: ChangeEvent) => void = () => undefined;
		const spion = vi.spyOn(live, 'onChange').mockImplementation((l) => {
			hoerer = l;
			return () => undefined;
		});
		let archiviert = false;
		const f = vi.fn((adresse: string) =>
			Promise.resolve(
				adresse.endsWith('/anzahl')
					? new Response(JSON.stringify({ offen: archiviert ? 0 : 1 }))
					: new Response(JSON.stringify({ aufgaben: archiviert ? [] : [aufgabe()] }))
			)
		);
		try {
			const a = new Aufgaben(f as unknown as typeof fetch);
			const ab = a.start();
			const weg = a.benutzen();
			await vi.waitFor(() => {
				expect(a.liste).toHaveLength(1);
				expect(a.anzahl).toBe(1);
			});
			archiviert = true;
			hoerer(ereignis('anstehend', 'anstehend'));
			await vi.waitFor(() => {
				expect(a.liste).toHaveLength(0);
				expect(a.anzahl).toBe(0);
			});
			archiviert = false;
			hoerer(ereignis('projects', 'projects/dashboard/projekt.toml'));
			await vi.waitFor(() => {
				expect(a.liste).toHaveLength(1);
				expect(a.anzahl).toBe(1);
			});
			weg();
			ab();
		} finally {
			spion.mockRestore();
		}
	});
});
