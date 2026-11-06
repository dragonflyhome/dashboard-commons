import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import Glocke from './Glocke.svelte';
import { Anstehend } from '#lib/anstehend.svelte.ts';
import { beispielListe, eintrag, json } from '#lib/anstehend.beispiel.ts';
import type { AnstehendEintrag, Meldung, MeldungsArt } from '$vertrag';

// Kopfleiste, Fassung 3: die Glocke ersetzt das Band.
function meldung(
	id: string,
	art: MeldungsArt,
	zuletzt: string,
	schluessel = `dashboard/${art}:${id}`,
	system = false
): AnstehendEintrag {
	const stufe = art === 'fertig' ? 'bald' : art === 'info' ? 'irgendwann' : 'jetzt';
	const e = eintrag({
		art: 'meldung',
		id,
		titel: `Meldung ${id}`,
		zustand: 'ungelesen',
		wartet_auf: null,
		wartet_auf_dich: false,
		ziel: system ? `/pc#meldung-${id}` : `/projects/dashboard#meldung-${id}`,
		meldung: { art, anzahl: 1, zuletzt, text: null, system, schluessel },
		erstellt: zuletzt
	});
	e.stufe = { vorschlag: stufe, inhaber: null, wirksam: stufe, grund: 'fest' };
	return e;
}

/** Eine Meldung, wie `/api/meldungen` sie liefert. */
function roh(id: string, behoben: string | null, gelesen: string | null = null): Meldung {
	return {
		id,
		projekt: 'dashboard',
		art: 'fehler',
		schluessel: `dashboard/bote-fehlt-${id}`,
		titel: `Roh ${id}`,
		text: null,
		verweis: null,
		system: true,
		geschuetzt: false,
		von: 'server',
		erstellt: '2026-10-30T08:00:00+01:00',
		zuletzt: '2026-10-30T08:00:00+01:00',
		anzahl: 1,
		gelesen,
		behoben,
		archiviert: null,
		zusammengefasst: false
	};
}

function speicherMit(
	eintraege: AnstehendEintrag[],
	alle: Meldung[] = [],
	status = 200
): { s: Anstehend; fetchFn: ReturnType<typeof vi.fn> } {
	const fetchFn = vi.fn((url: string) => {
		if (status !== 200)
			return Promise.resolve(json({ fehler: 'Bitte anmelden', art: 'anmeldung' }, status));
		if (url.endsWith('/api/meldungen')) return Promise.resolve(json({ meldungen: alle }));
		if (url.endsWith('/gelesen')) return Promise.resolve(json({ meldung: {}, geaendert: true }));
		return Promise.resolve(json({ ...beispielListe(), eintraege }));
	});
	return { s: new Anstehend(fetchFn as unknown as typeof fetch), fetchFn };
}

describe('Glocke (Kopfleiste, Fassung 3)', () => {
	it('zählt ungelesene Meldungen aus „Anstehend“, alle Arten, ohne Rechte-Hinweise', async () => {
		const { s } = speicherMit([
			meldung('m-1', 'fehler', '2026-10-30T08:00:00+01:00'),
			meldung('m-2', 'fertig', '2026-10-30T09:00:00+01:00'),
			meldung('m-3', 'info', '2026-10-30T10:00:00+01:00'),
			meldung('m-4', 'fehler', '2026-10-30T11:00:00+01:00', 'dashboard/lesen:docs', true),
			eintrag({ id: 'a-1', titel: 'Eine Aufgabe' })
		]);
		await s.laden();
		render(Glocke, { speicher: s });
		const knopf = screen.getByRole('button', { name: 'Meldungen: 3 ungelesen' });
		expect(knopf.querySelector('.zahl')).toHaveTextContent('3');
	});

	it('behobene zählen nicht; sie stehen nur in der Liste unter „behoben“', async () => {
		const { s, fetchFn } = speicherMit(
			[meldung('m-1', 'frist', '2026-10-30T08:00:00+01:00')],
			[
				roh('m-0003', '2026-10-30T09:00:00+01:00'),
				roh('m-0009', '2026-10-30T09:00:00+01:00'),
				roh('m-x', null)
			]
		);
		await s.laden();
		render(Glocke, { speicher: s });
		const knopf = screen.getByRole('button', { name: 'Meldungen: 1 ungelesen' });
		await fireEvent.click(knopf);
		await waitFor(() => {
			expect(fetchFn).toHaveBeenCalledWith(
				expect.stringMatching(/\/api\/meldungen$/),
				expect.anything()
			);
		});
		const feld = await screen.findByRole('dialog');
		await waitFor(() => {
			expect(within(feld).getByText('behoben')).toBeInTheDocument();
		});
		expect(within(feld).getByText('Roh m-0003')).toBeInTheDocument();
		expect(within(feld).getByText('Roh m-0009')).toBeInTheDocument();
		// Nicht behoben (aber nicht in Anstehend): nicht unter „behoben“.
		expect(within(feld).queryByText('Roh m-x')).toBeNull();
		// Die Zahl bleibt 1.
		expect(knopf).toHaveAccessibleName('Meldungen: 1 ungelesen');
	});

	it('Rechte-Meldungen stehen als Hinweis da, nicht als Fehler', async () => {
		const { s } = speicherMit([
			meldung('m-4', 'fehler', '2026-10-30T11:00:00+01:00', 'dashboard/lesen:docs', true)
		]);
		await s.laden();
		render(Glocke, { speicher: s, offen: true });
		const feld = await screen.findByRole('dialog');
		expect(within(feld).getByText('Hinweise')).toBeInTheDocument();
		expect(within(feld).getByText(/^Hinweis · PC/)).toBeInTheDocument();
		expect(within(feld).getByText('Keine ungelesenen Meldungen.')).toBeInTheDocument();
		expect(
			screen.getByRole('button', { name: 'Meldungen: keine ungelesenen' })
		).toBeInTheDocument();
	});

	it('ohne Anmeldung: keine Zahl, nur der Hinweis zur Anmeldung', async () => {
		const { s } = speicherMit([], [], 401);
		await s.laden();
		render(Glocke, { speicher: s, offen: true });
		const knopf = screen.getByRole('button', { name: 'Meldungen: erst nach der Anmeldung' });
		expect(knopf.querySelector('.zahl')).toBeNull();
		const feld = await screen.findByRole('dialog');
		expect(within(feld).getByText(/Anstehendes siehst du in Microsoft Edge/)).toBeInTheDocument();
		expect(within(feld).getByRole('link', { name: 'Mit Passkey anmelden' })).toBeInTheDocument();
	});

	it('offen sperrt die Seite nicht (kein pointer-events: none, kein overflow: hidden)', async () => {
		const { s } = speicherMit([meldung('m-1', 'fehler', '2026-10-30T08:00:00+01:00')]);
		await s.laden();
		render(Glocke, { speicher: s });
		await fireEvent.click(screen.getByRole('button', { name: /^Meldungen/ }));
		await screen.findByRole('dialog');
		for (let i = 0; i < 5; i++) {
			await tick();
			await new Promise((f) => setTimeout(f, 0));
		}
		expect(document.body.style.pointerEvents).not.toBe('none');
		expect(document.body.style.overflow).not.toBe('hidden');
	});

	it('„gelesen“ in der Liste: die Zahl folgt ohne Neuladen', async () => {
		let eintraege = [
			meldung('m-1', 'fehler', '2026-10-30T08:00:00+01:00'),
			meldung('m-2', 'frist', '2026-10-30T09:00:00+01:00')
		];
		const fetchFn = vi.fn((url: string) => {
			const m = /\/api\/meldungen\/([^/]+)\/gelesen$/.exec(url);
			if (m?.[1] !== undefined) {
				eintraege = eintraege.filter((e) => e.id !== m[1]);
				return Promise.resolve(json({ meldung: {}, geaendert: true }));
			}
			if (url.endsWith('/api/meldungen')) return Promise.resolve(json({ meldungen: [] }));
			return Promise.resolve(json({ ...beispielListe(), eintraege }));
		});
		const s = new Anstehend(fetchFn as unknown as typeof fetch);
		await s.laden();
		render(Glocke, { speicher: s, offen: true });
		const feld = await screen.findByRole('dialog');
		await fireEvent.click(within(feld).getByRole('button', { name: 'Gelesen: Meldung m-2' }));
		await waitFor(() => {
			expect(screen.getByRole('button', { name: 'Meldungen: 1 ungelesen' })).toBeInTheDocument();
		});
	});
});
