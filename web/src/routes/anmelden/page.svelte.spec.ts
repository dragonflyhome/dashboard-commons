import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type * as PasskeyModul from '#lib/passkey.ts';
import { ApiError } from '#lib/api/client.ts';
import { schlossZiel } from '#lib/verbindung.ts';
import type { PasskeyStand } from '$vertrag';

// Schritt E2: Ohne Anmeldung leitet jede Seite nach /anmelden?ziel=<pfad>&umgeleitet=1. Die Seite
// fragt /api/passkey/stand (gilt schon eine Anmeldung, geht es nach so einer Umleitung sofort weiter) und
// /api/anmeldung/wer (Windows: der Satz zu Edge; sonst der Satz zum Passkey).
const { ort, passkey } = vi.hoisted(() => ({
	ort: { url: new URL('http://localhost:8787/anmelden') },
	passkey: {
		stand: vi.fn(),
		eigeneAntraege: vi.fn(),
		weiter: vi.fn(),
		anlegen: vi.fn(),
		anlegenMitCode: vi.fn()
	}
}));
vi.mock('$app/state', () => ({ page: ort }));
vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
vi.mock('#lib/freikanal.svelte.ts', () => ({
	freierKanal: { zustand: 'nicht_angemeldet', abgemeldet: vi.fn() }
}));
vi.mock('#lib/passkey.ts', async (original) => ({
	...(await original<typeof PasskeyModul>()),
	stand: passkey.stand,
	eigeneAntraege: passkey.eigeneAntraege,
	weiter: passkey.weiter,
	anlegen: passkey.anlegen,
	anlegenMitCode: passkey.anlegenMitCode
}));
const { default: Page } = await import('./+page.svelte');

const KEINE: PasskeyStand = {
	wie: 'keine',
	passkey: null,
	sitzung_bis: null,
	frisch_bis: null,
	verfuegbar: true
};
const MIT_PASSKEY: PasskeyStand = {
	...KEINE,
	wie: 'passkey',
	passkey: 'probekonto',
	sitzung_bis: '2026-11-15T18:00:00+01:00'
};

/** `/api/anmeldung/wer` antwortet mit `status`; jede andere Adresse ist ein Fehler im Test. */
function wer(status: number) {
	const abruf = vi.fn((pfad: string) => {
		if (pfad !== '/api/anmeldung/wer') throw new Error(`unerwartet: ${pfad}`);
		return Promise.resolve(new Response('{}', { status }));
	});
	vi.stubGlobal('fetch', abruf);
	return abruf;
}

beforeEach(() => {
	passkey.eigeneAntraege.mockRejectedValue(new Error('kein Konto'));
	passkey.stand.mockResolvedValue(KEINE);
	passkey.weiter.mockReset();
	passkey.eigeneAntraege.mockClear();
	passkey.anlegen.mockReset();
	passkey.anlegenMitCode.mockReset();
	ort.url = new URL('http://localhost:8787/anmelden');
});
afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
});

describe('/anmelden (Schritt E2)', () => {
	// Unter Windows folgt Edge dem Link „Zur Anmeldung“ der 401-Seite und kommt als Hauptkonto an.
	it.each([
		['Passkey', MIT_PASSKEY],
		['Windows (Hauptkonto)', { ...KEINE, wie: 'hauptkonto' } satisfies PasskeyStand]
	])('umgeleitet, mit gültiger Anmeldung (%s): geht sofort weiter zum Ziel', async (_, gilt) => {
		wer(200);
		passkey.stand.mockResolvedValue(gilt);
		ort.url = new URL(
			'http://localhost:8787/anmelden?ziel=%2Fprojects%2Fbeispiel%3Fx%3D1&umgeleitet=1'
		);
		render(Page);
		await vi.waitFor(() => {
			expect(passkey.weiter).toHaveBeenCalledWith('/projects/beispiel?x=1');
		});
		expect(passkey.stand).toHaveBeenCalled();
	});

	// Das Schloss trägt den Rückweg (?ziel=), aber nie die Markierung: Die Seite bleibt zum Anlegen.
	it.each([
		['Passkey', MIT_PASSKEY],
		['Windows (Hauptkonto)', { ...KEINE, wie: 'hauptkonto' } satisfies PasskeyStand]
	])(
		'angemeldet (%s) über das Schloss: bleibt, Passkey anlegen, Weiter zum Ziel',
		async (_, gilt) => {
			wer(200);
			passkey.stand.mockResolvedValue(gilt);
			// Wie ein echter Sprung zum Ziel: Die Seite ist danach weg, der Antrag unten nicht mehr da.
			passkey.weiter.mockImplementation(() => {
				cleanup();
			});
			const ziel = schlossZiel('angemeldet', '/anstehend?stufe=jetzt');
			ort.url = new URL(ziel, 'http://localhost:8787');
			render(Page);
			const weiter = await screen.findByRole('link', { name: 'Weiter' });
			expect(weiter).toHaveAttribute('href', '/anstehend?stufe=jetzt');
			// Das Laden beim Öffnen ist durch (stand geholt, Entscheidung gefallen), der Antrag steht noch da.
			await vi.waitFor(() => {
				expect(passkey.stand).toHaveBeenCalled();
			});
			await new Promise((fertig) => setTimeout(fertig, 0));
			// toBeInTheDocument statt not.toBeNull: `?.` ergäbe ohne Seite undefined und ginge durch.
			expect(document.querySelector('#passkey-anlegen #geraet')).toBeInTheDocument();
			expect(passkey.weiter).not.toHaveBeenCalled();
		}
	);

	it.each(['https://fremd.example/', '//fremd.example/x', '/anmelden?ziel=%2Fpc&umgeleitet=1'])(
		'umgeleitet mit fremdem Ziel %s: nur zur Startseite (zielPruefen), keine offene Umleitung',
		async (fremd) => {
			wer(200);
			passkey.stand.mockResolvedValue(MIT_PASSKEY);
			ort.url = new URL(
				`http://localhost:8787/anmelden?ziel=${encodeURIComponent(fremd)}&umgeleitet=1`
			);
			render(Page);
			await vi.waitFor(() => {
				expect(passkey.weiter).toHaveBeenCalledWith('/');
			});
		}
	);

	it('mit gültigem Passkey ohne Ziel (Schloss oben): bleibt zum Verwalten und Abmelden', async () => {
		wer(200);
		passkey.stand.mockResolvedValue(MIT_PASSKEY);
		render(Page);
		expect(await screen.findByRole('button', { name: 'Abmelden' })).toBeInTheDocument();
		expect(passkey.weiter).not.toHaveBeenCalled();
	});

	it('umgeleitet ohne Anmeldung: kein Weiter, der Satz zur Anmeldung', async () => {
		wer(501);
		ort.url = new URL('http://localhost:8787/anmelden?ziel=%2Fpc&umgeleitet=1');
		render(Page);
		expect(
			await screen.findByText(
				'Anmeldung nötig. Ohne Anmeldung zeigt das Dashboard nur die Anmeldeseite.'
			)
		).toBeInTheDocument();
		await vi.waitFor(() => {
			expect(passkey.stand).toHaveBeenCalled();
		});
		expect(passkey.weiter).not.toHaveBeenCalled();
	});

	it('Windows (wer 200): der Satz zu Edge mit der Adresse über localhost, Link aufs Ziel', async () => {
		const abruf = wer(200);
		ort.url = new URL('http://127.0.0.1:8788/anmelden?ziel=%2Fpc');
		render(Page);
		const link = await screen.findByRole('link', { name: 'http://localhost:8788/' });
		expect(link).toHaveAttribute('href', 'http://localhost:8788/pc');
		expect(link.closest('p')?.textContent).toMatch(/^\s*Mit deiner Windows-Anmeldung .*Edge/);
		expect(screen.queryByText('Anmelden geht hier mit einem Passkey.')).toBeNull();
		expect(abruf).toHaveBeenCalledWith('/api/anmeldung/wer', expect.anything());
	});

	it('anderes System (wer 501): der neutrale Satz zum Passkey, kein Edge', async () => {
		wer(501);
		render(Page);
		expect(await screen.findByText('Anmelden geht hier mit einem Passkey.')).toBeInTheDocument();
		expect(document.body.textContent).not.toContain('Edge');
	});

	it('kein Server (wer wirft): weder der eine noch der andere Satz', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(() => Promise.reject(new TypeError('Failed to fetch')))
		);
		render(Page);
		await vi.waitFor(() => {
			expect(passkey.stand).toHaveBeenCalled();
		});
		expect(screen.queryByText('Anmelden geht hier mit einem Passkey.')).toBeNull();
		expect(document.body.textContent).not.toContain('Windows-Anmeldung öffnet');
	});
});

// Schritt 7c der macOS-Folge: außerhalb von Windows (Probe 501) ein Passkey mit Einmal-Code.
describe('/anmelden mit Einmal-Code (Schritt 7c)', () => {
	const SATZ_KARTE =
		'Einen Passkey legst du hier mit einem Einmal-Code an. Den Code zeigt dir der Befehl dashboard einmal-code im Terminal. Er gilt 10 Minuten und nur einmal.';
	const SATZ_GILT_NICHT = 'Dieser Einmal-Code gilt nicht (mehr). Bitte einen neuen holen.';
	const CODE = 'ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-23';

	/** Der Text eines Elements ohne den Knopf und mit einfachen Leerzeichen. */
	function satzOhneKnopf(el: Element): string {
		const kopie = el.cloneNode(true) as Element;
		kopie.querySelectorAll('button').forEach((b) => {
			b.remove();
		});
		return kopie.textContent.replace(/\s+/g, ' ').trim();
	}

	/** Eine Zwischenablage, die `writeText` nimmt. */
	function zwischenablage(writeText: (text: string) => Promise<void>): void {
		Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
	}

	async function codeFeld(): Promise<HTMLInputElement> {
		return screen.findByLabelText<HTMLInputElement>('Einmal-Code');
	}

	async function absenden(code: string, name: string): Promise<void> {
		await fireEvent.input(await codeFeld(), { target: { value: code } });
		await fireEvent.input(screen.getByLabelText('Name des Geräts'), { target: { value: name } });
		await fireEvent.click(screen.getByRole('button', { name: 'Passkey anlegen' }));
	}

	it('außerhalb von Windows: die neuen Sätze, ohne Hauptkonto und Antrag', async () => {
		wer(501);
		render(Page);
		const karte = (await screen.findByText(/Einen Passkey legst du hier/)).closest('p');
		expect(karte).not.toBeNull();
		expect(satzOhneKnopf(karte as Element)).toBe(SATZ_KARTE);
		expect(karte?.querySelector('code')?.textContent).toBe('dashboard einmal-code');
		expect(
			screen.getByText(
				'Mit einem Passkey auf diesem Rechner, dem Handy oder einem Sicherheitsschlüssel.'
			)
		).toBeInTheDocument();
		const text = document.body.textContent;
		for (const weg of [
			'Der Passkey muss vorher freigegeben sein.',
			'Für dich in einem anderen Windows-Konto',
			'Du bist im Hauptkonto.',
			'Aus einem anderen Windows-Konto wird das ein'
		]) {
			expect(text).not.toContain(weg);
		}
	});

	it('außerhalb von Windows: die eigenen Anträge werden nicht gefragt', async () => {
		wer(501);
		render(Page);
		await vi.waitFor(() => {
			expect(passkey.stand).toHaveBeenCalled();
		});
		expect(passkey.eigeneAntraege).not.toHaveBeenCalled();
	});

	it('das Feld „Einmal-Code“ steht vor dem Namen, mit den drei Attributen', async () => {
		wer(501);
		render(Page);
		const feld = await codeFeld();
		expect(feld.type).toBe('text');
		expect(feld).toHaveAttribute('autocomplete', 'off');
		expect(feld).toHaveAttribute('spellcheck', 'false');
		expect(feld).toHaveAttribute('autocapitalize', 'characters');
		const name = screen.getByLabelText('Name des Geräts');
		expect(feld.compareDocumentPosition(name) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Passkey anlegen' })).toBeInTheDocument();
	});

	it('Erfolg: der Satz, das Feld ist leer, die Seite bleibt stehen', async () => {
		wer(501);
		passkey.anlegenMitCode.mockResolvedValue({
			status: 'registriert',
			vergleichszahl: null,
			ablauf: null
		});
		render(Page);
		await absenden(CODE, 'Laptop');
		expect(
			await screen.findByText('Der Passkey ist angelegt. Jetzt oben „Mit Passkey anmelden“.')
		).toBeInTheDocument();
		expect(passkey.anlegenMitCode).toHaveBeenCalledWith(CODE, 'Laptop');
		expect(passkey.anlegen).not.toHaveBeenCalled();
		expect((await codeFeld()).value).toBe('');
		expect(passkey.weiter).not.toHaveBeenCalled();
	});

	it('bei 403 der Satz des Servers, und das Feld ist auch nach dem Fehler leer', async () => {
		wer(501);
		passkey.anlegenMitCode.mockRejectedValue(new ApiError(403, SATZ_GILT_NICHT, 'einmal-code'));
		render(Page);
		await absenden(CODE, 'Laptop');
		expect((await screen.findByRole('alert')).textContent).toBe(SATZ_GILT_NICHT);
		expect((await codeFeld()).value).toBe('');
		// Der Name bleibt für den nächsten Versuch.
		expect(screen.getByLabelText<HTMLInputElement>('Name des Geräts').value).toBe('Laptop');
	});

	it('der Code kommt nie aus der Adresse', async () => {
		wer(501);
		passkey.anlegenMitCode.mockResolvedValue({
			status: 'registriert',
			vergleichszahl: null,
			ablauf: null
		});
		ort.url = new URL(`http://localhost:8787/anmelden?code=${CODE}#code=${CODE}`);
		render(Page);
		expect((await codeFeld()).value).toBe('');
		await fireEvent.input(screen.getByLabelText('Name des Geräts'), {
			target: { value: 'Laptop' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Passkey anlegen' }));
		await vi.waitFor(() => {
			expect(passkey.anlegenMitCode).toHaveBeenCalledWith('', 'Laptop');
		});
	});

	it('angemeldet mit Passkey: das Feld für den Code steht auch dann da', async () => {
		wer(501);
		passkey.stand.mockResolvedValue(MIT_PASSKEY);
		render(Page);
		expect(await screen.findByRole('button', { name: 'Abmelden' })).toBeInTheDocument();
		expect(await codeFeld()).toBeInTheDocument();
	});

	it('„Kopieren“ kopiert genau den Befehl', async () => {
		wer(501);
		const writeText = vi.fn(() => Promise.resolve());
		zwischenablage(writeText);
		render(Page);
		await fireEvent.click(await screen.findByRole('button', { name: 'Kopieren' }));
		expect(writeText).toHaveBeenCalledWith('dashboard einmal-code');
	});

	it('scheitert das Kopieren, erscheint kein Fehler und der Befehl bleibt markierbar', async () => {
		wer(501);
		const writeText = vi.fn(() => Promise.reject(new DOMException('nein', 'NotAllowedError')));
		zwischenablage(writeText);
		render(Page);
		await fireEvent.click(await screen.findByRole('button', { name: 'Kopieren' }));
		await vi.waitFor(() => {
			expect(writeText).toHaveBeenCalled();
		});
		await new Promise((fertig) => setTimeout(fertig, 0));
		expect(screen.queryByRole('alert')).toBeNull();
		expect(document.querySelector('code')?.textContent).toBe('dashboard einmal-code');
	});

	it('unter Windows (wer 200): alles wie vorher, ohne Feld für den Code', async () => {
		wer(200);
		passkey.anlegen.mockResolvedValue({
			status: 'ausstehend',
			vergleichszahl: '0427',
			ablauf: null
		});
		render(Page);
		expect(
			await screen.findByText(/Aus einem anderen Windows-Konto wird das ein/)
		).toBeInTheDocument();
		expect(document.body.textContent).toContain('Der Passkey muss vorher freigegeben sein.');
		expect(document.body.textContent).not.toContain('Einmal-Code');
		expect(screen.queryByLabelText('Einmal-Code')).toBeNull();
		await vi.waitFor(() => {
			expect(passkey.eigeneAntraege).toHaveBeenCalled();
		});
		await fireEvent.input(screen.getByLabelText('Name des Geräts'), {
			target: { value: 'Handy' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Passkey anlegen' }));
		await vi.waitFor(() => {
			expect(passkey.anlegen).toHaveBeenCalledWith('Handy');
		});
		expect(passkey.anlegenMitCode).not.toHaveBeenCalled();
	});
});
