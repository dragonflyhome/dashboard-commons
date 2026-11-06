import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import Sicherung from './Sicherung.svelte';
import { ApiError } from '#lib/api/client.ts';
import type { DatenBericht, SicherungAntwort } from '$vertrag';

const bericht: DatenBericht = {
	pfad: 'C:\\daten\\dashboard.db',
	groesse_bytes: 4096,
	schema: 1,
	letzte_sicherung: null,
	sicherung_klasse: 'bad',
	sicherungs_ordner: 'C:\\daten\\sicherung',
	tabellen: [],
	pc_verlauf: { erster: null, letzter: null, eintraege: 0 },
	fehler: null
};

describe('Sicherung', () => {
	it('zeigt „noch keine“ und nach Erfolg den grünen Satz', async () => {
		const sichern = vi.fn((): Promise<SicherungAntwort> =>
			Promise.resolve({ datei: 'a.db', export: 'a.toml', datum: '2026-10-25' })
		);
		const nachher = vi.fn();
		render(Sicherung, { bericht, sichern, nachher });
		expect(screen.getByText(/noch keine/)).toBeInTheDocument();
		await fireEvent.click(screen.getByRole('button', { name: 'Jetzt sichern' }));
		expect(await screen.findByText('Sicherung vom 25.10.2026 angelegt.')).toBeInTheDocument();
		expect(sichern).toHaveBeenCalledTimes(1);
		expect(nachher).toHaveBeenCalledTimes(1);
		expect(screen.getByRole('button', { name: 'Jetzt sichern' })).toBeEnabled();
	});
	it('zeigt den Fehler des Servers als roten Satz', async () => {
		const sichern = vi.fn((): Promise<SicherungAntwort> =>
			Promise.reject(new ApiError(500, 'Die Platte ist voll.'))
		);
		const nachher = vi.fn();
		render(Sicherung, { bericht, sichern, nachher });
		await fireEvent.click(screen.getByRole('button', { name: 'Jetzt sichern' }));
		const satz = await screen.findByText('Die Platte ist voll.');
		expect(satz).toHaveClass('block-error');
		expect(nachher).not.toHaveBeenCalled();
		expect(screen.queryByText(/angelegt/)).not.toBeInTheDocument();
	});
	it('zeigt „Sichere …“, solange es läuft', async () => {
		let fertig: (a: SicherungAntwort) => void = () => undefined;
		const sichern = vi.fn(
			() =>
				new Promise<SicherungAntwort>((r) => {
					fertig = r;
				})
		);
		render(Sicherung, { bericht, sichern });
		await fireEvent.click(screen.getByRole('button', { name: 'Jetzt sichern' }));
		expect(screen.getByRole('button', { name: 'Sichere …' })).toBeDisabled();
		fertig({ datei: 'a.db', export: 'a.toml', datum: '2026-10-25' });
		expect(await screen.findByText(/angelegt/)).toBeInTheDocument();
	});
	it('sperrt den Knopf ohne Datenspeicher und sagt warum', async () => {
		const sichern = vi.fn((): Promise<SicherungAntwort> => Promise.reject(new Error('nie')));
		render(Sicherung, {
			bericht: { ...bericht, fehler: 'Der Datenspeicher ließ sich nicht öffnen.' },
			sichern
		});
		const knopf = screen.getByRole('button', { name: 'Jetzt sichern' });
		expect(knopf).toBeDisabled();
		expect(knopf).toHaveAccessibleDescription(/erst wieder, wenn der Datenspeicher offen ist/);
		await fireEvent.click(knopf);
		expect(sichern).not.toHaveBeenCalled();
		expect(screen.getByText('C:\\daten\\sicherung')).toBeInTheDocument();
	});
});
