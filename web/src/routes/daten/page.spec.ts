import { isHttpError } from '@sveltejs/kit';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '#lib/api/client.ts';
import type * as Client from '#lib/api/client.ts';
import type { DatenBericht, PcVerlaufAntwort } from '$vertrag';

const daten = vi.fn<() => Promise<DatenBericht>>();
const pcVerlauf = vi.fn<() => Promise<PcVerlaufAntwort>>();
vi.mock('#lib/api/client.ts', async (original) => ({
	...(await original<typeof Client>()),
	api: { daten: () => daten(), pcVerlauf: () => pcVerlauf() }
}));

const { load } = await import('./+page.ts');

const bericht: DatenBericht = {
	pfad: 'C:\\daten\\dashboard.redb',
	groesse_bytes: 4096,
	schema: 1,
	letzte_sicherung: null,
	sicherung_klasse: 'ok',
	sicherungs_ordner: 'C:\\daten\\sicherung',
	tabellen: [],
	pc_verlauf: { erster: null, letzter: null, eintraege: 0 },
	fehler: null
};

async function lade(): Promise<unknown> {
	const event = { fetch, depends: () => undefined } as unknown as Parameters<typeof load>[0];
	return load(event);
}

describe('Laden der Seite Daten', () => {
	beforeEach(() => {
		daten.mockReset();
		pcVerlauf.mockReset();
	});
	it('fragt den Verlauf ohne Datenspeicher gar nicht erst', async () => {
		daten.mockResolvedValue({ ...bericht, fehler: 'Speicher zu.' });
		const ergebnis = await lade();
		expect(pcVerlauf).not.toHaveBeenCalled();
		expect(ergebnis).toMatchObject({ verlauf: { werte: [] } });
	});
	it('nimmt bei 503 des Verlaufs leere Werte', async () => {
		daten.mockResolvedValue(bericht);
		pcVerlauf.mockRejectedValue(new ApiError(503, 'Speicher zu.'));
		expect(await lade()).toMatchObject({ verlauf: { werte: [] } });
	});
	it('reicht andere Fehler als HttpError weiter', async () => {
		daten.mockResolvedValue(bericht);
		pcVerlauf.mockRejectedValue(new ApiError(500, 'Kaputt.'));
		const e = await lade().catch((x: unknown) => x);
		expect(isHttpError(e, 500)).toBe(true);
		expect(e).toMatchObject({ body: { message: 'Kaputt.' } });
		expect(e).not.toHaveProperty('body.art');
		daten.mockRejectedValue(new ApiError(503, 'Zu.'));
		expect(isHttpError(await lade().catch((x: unknown) => x), 503)).toBe(true);
	});
});
