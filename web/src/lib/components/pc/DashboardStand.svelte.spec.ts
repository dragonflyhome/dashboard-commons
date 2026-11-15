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
			sitzung: null,
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
