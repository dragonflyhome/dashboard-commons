import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Betrieb } from '$vertrag';
import DashboardStand from './DashboardStand.svelte';

describe('Abschnitt „Dashboard“ (Schritt E1)', () => {
	it('zeigt die Einstellungsdatei aus /api/betrieb/voll im Klartext', async () => {
		// Nur der volle Bericht (geschützt) nennt sie; erfundenes Profil.
		const betrieb = {
			modus: 'dienst',
			version: '0.0.0',
			commit: null,
			gestartet: '2026-10-26T08:00:00+01:00',
			einstellungen: 'C:\\Users\\beispiel\\dienst.toml',
			bote: 'verbunden',
			bote_seit: null,
			sitzung: {
				geht: null,
				inhaber_angemeldet: null,
				quelle: null,
				fehler: null,
				geprueft: null
			},
			fassung: 'build',
			fassung_gebaut: null,
			ordner: [{ name: 'projects', stand: 'lesbar' }]
		} as unknown as Betrieb;
		render(DashboardStand, { laden: () => Promise.resolve(betrieb), zustand: null });
		expect(await screen.findByText('C:\\Users\\beispiel\\dienst.toml')).toBeInTheDocument();
		expect(document.body.textContent).not.toContain('%USERPROFILE%');
	});
});

describe('Abschnitt „Dashboard“ (Schritt E2)', () => {
	it('fragt nur /api/betrieb/voll, auch solange der geschützte Kanal noch nicht steht', async () => {
		const abruf = vi.fn(() => Promise.resolve(new Response('{}', { status: 500 })));
		vi.stubGlobal('fetch', abruf);
		try {
			render(DashboardStand, { zustand: null });
			await vi.waitFor(() => {
				expect(abruf).toHaveBeenCalled();
			});
			expect(abruf.mock.calls.map((c: unknown[]) => c[0])).toEqual(['/api/betrieb/voll']);
		} finally {
			vi.unstubAllGlobals();
		}
	});
});

describe('Abschnitt „Dashboard“ (Schritt E3)', () => {
	it('eine späte Antwort überschreibt keine neuere: angezeigt die zweite Ladung', async () => {
		const bericht = (version: string) =>
			({
				version,
				commit: null,
				gestartet: '2026-10-26T08:00:00+01:00',
				einstellungen: null,
				bote: 'nicht vorgesehen',
				bote_seit: null,
				sitzung: {
					geht: null,
					inhaber_angemeldet: true,
					quelle: null,
					fehler: null,
					geprueft: null
				},
				fassung: 'build',
				fassung_gebaut: null,
				ordner: []
			}) as unknown as Betrieb;
		const offen: ((b: Betrieb) => void)[] = [];
		const laden = vi.fn(
			() =>
				new Promise<Betrieb>((fertig) => {
					offen.push(fertig);
				})
		);
		// Die zweite Ladung kommt vom Minutentakt.
		vi.useFakeTimers({ toFake: ['setInterval'] });
		render(DashboardStand, { laden, zustand: null });
		vi.advanceTimersByTime(60_000);
		vi.useRealTimers();
		expect(laden).toHaveBeenCalledTimes(2);
		offen[1]?.(bericht('2.0.0'));
		expect(await screen.findByText(/Server 2\.0\.0/)).toBeInTheDocument();
		offen[0]?.(bericht('1.0.0'));
		await new Promise((r) => setTimeout(r, 0));
		expect(document.body.textContent).toContain('Server 2.0.0');
		expect(document.body.textContent).not.toContain('Server 1.0.0');
		expect(screen.getByText('Du bist an diesem PC angemeldet')).toBeInTheDocument();
	});

	it('ein später Fehler steht nicht unter einer neueren guten Antwort', async () => {
		const offen: { fertig: (b: Betrieb) => void; fehl: (e: Error) => void }[] = [];
		const laden = vi.fn(
			() =>
				new Promise<Betrieb>((fertig, fehl) => {
					offen.push({ fertig, fehl });
				})
		);
		vi.useFakeTimers({ toFake: ['setInterval'] });
		render(DashboardStand, { laden, zustand: null });
		vi.advanceTimersByTime(60_000);
		vi.useRealTimers();
		expect(laden).toHaveBeenCalledTimes(2);
		offen[1]?.fertig({
			version: '2.0.0',
			commit: null,
			gestartet: '2026-10-26T08:00:00+01:00',
			einstellungen: null,
			bote: 'nicht vorgesehen',
			bote_seit: null,
			sitzung: { geht: null, inhaber_angemeldet: true, quelle: null, fehler: null, geprueft: null },
			fassung: 'build',
			fassung_gebaut: null,
			ordner: []
		} as unknown as Betrieb);
		expect(await screen.findByText(/Server 2\.0\.0/)).toBeInTheDocument();
		offen[0]?.fehl(new Error('Der Server antwortet mit 500.'));
		await new Promise((r) => setTimeout(r, 0));
		expect(document.querySelector('.block-error')).toBeNull();
		expect(document.body.textContent).toContain('Server 2.0.0');
	});
});
