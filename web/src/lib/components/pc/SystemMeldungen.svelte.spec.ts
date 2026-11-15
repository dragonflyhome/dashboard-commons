import { render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import { SATZ_NICHT_MEHR_DA } from '#lib/anstehend.ts';
import { json } from '#lib/anstehend.beispiel.ts';
import type { Meldung } from '$vertrag';

const { ort } = vi.hoisted(() => ({ ort: { url: new URL('http://localhost:8787/pc') } }));
vi.mock('$app/state', () => ({ page: ort }));
// Der freie Kanal als Platzhalter; die Hörer von `onAbgemeldet` stehen in `abgemeldet`, damit
// ein Test das Abmelden auslösen kann.
const { abgemeldet } = vi.hoisted(() => ({ abgemeldet: new Set<() => void>() }));
vi.mock('#lib/freikanal.svelte.ts', () => ({
	freierKanal: {
		onAbgemeldet: (l: () => void) => {
			abgemeldet.add(l);
			return () => abgemeldet.delete(l);
		}
	}
}));
const { default: SystemMeldungen } = await import('./SystemMeldungen.svelte');

function meldung(teil: Partial<Meldung>): Meldung {
	return {
		id: 'm-20261030-0001',
		projekt: 'dashboard',
		art: 'fehler',
		schluessel: 'dashboard/bote-fehlt',
		titel: 'Der Bote fehlt seit mehr als 10 Minuten, obwohl du angemeldet bist.',
		text: 'Ohne Boten fehlen die Benutzer-Daten der PC-Seite.',
		verweis: null,
		system: true,
		geschuetzt: false,
		von: 'server',
		erstellt: '2026-10-30T08:11:00+01:00',
		zuletzt: '2026-10-30T08:11:00+01:00',
		anzahl: 1,
		gelesen: null,
		behoben: null,
		archiviert: null,
		zusammengefasst: false,
		...teil
	};
}

function netz(meldungen: Meldung[]): typeof fetch {
	return () => Promise.resolve(json({ meldungen }));
}

describe('Systemmeldungen auf der PC-Seite (Etappe A3)', () => {
	it('„Bote fehlt“: Ziel /pc#meldung-<id> klappt auf und bekommt den Fokus', async () => {
		ort.url = new URL('http://localhost:8787/pc#meldung-m-20261030-0001');
		const andere = meldung({ id: 'm-2', system: false, titel: 'Projektmeldung' });
		render(SystemMeldungen, { fetchFn: netz([meldung({}), andere]) });
		await waitFor(() => {
			expect(document.getElementById('meldung-m-20261030-0001')).toHaveFocus();
		});
		expect(document.getElementById('meldung-m-20261030-0001')).toHaveProperty('open', true);
		expect(screen.getByRole('heading', { name: 'Meldungen zum PC' })).toBeInTheDocument();
		// Nur Systemmeldungen.
		expect(screen.queryByText('Projektmeldung')).toBeNull();
		expect(screen.getByRole('button', { name: /^Gelesen:/ })).toBeInTheDocument();
	});

	it('zeigt Titel und Text aus /api/meldungen (geschützt) im Klartext (Schritt E1)', async () => {
		ort.url = new URL('http://localhost:8787/pc');
		const m = meldung({
			titel: 'C:\\Users\\beispiel\\dev ist nicht lesbar',
			text: 'Zugriff verweigert: C:\\Users\\beispiel\\dev'
		});
		const { container } = render(SystemMeldungen, { fetchFn: netz([m]) });
		expect(
			await screen.findByText('C:\\Users\\beispiel\\dev ist nicht lesbar')
		).toBeInTheDocument();
		expect(container.textContent).toContain('Zugriff verweigert: C:\\Users\\beispiel\\dev');
		expect(container.textContent).not.toContain('%USERPROFILE%');
	});

	it('fehlt die Meldung (im Archiv): „nicht mehr da“', async () => {
		ort.url = new URL('http://localhost:8787/pc#meldung-m-alt');
		render(SystemMeldungen, { fetchFn: netz([meldung({ behoben: '2026-10-30T09:00:00+01:00' })]) });
		expect(await screen.findByRole('status')).toHaveTextContent(SATZ_NICHT_MEHR_DA);
		expect(screen.getByText(/behoben/)).toBeInTheDocument();
	});
	it('lädt nach dem Abmelden neu; ohne Anmeldung verschwindet der Kasten (Schritt E1)', async () => {
		ort.url = new URL('http://localhost:8787/pc');
		let angemeldet = true;
		const fetchFn = vi.fn(() =>
			Promise.resolve(angemeldet ? json({ meldungen: [meldung({})] }) : json({}, 401))
		);
		const { unmount } = render(SystemMeldungen, { fetchFn });
		expect(await screen.findByRole('heading', { name: 'Meldungen zum PC' })).toBeInTheDocument();
		expect(fetchFn).toHaveBeenCalledTimes(1);
		angemeldet = false;
		for (const l of abgemeldet) l();
		await waitFor(() => {
			expect(screen.queryByRole('heading', { name: 'Meldungen zum PC' })).toBeNull();
		});
		expect(fetchFn).toHaveBeenCalledTimes(2);
		unmount();
		expect(abgemeldet.size).toBe(0);
	});
});
