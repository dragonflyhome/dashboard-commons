import { isHttpError } from '@sveltejs/kit';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AnmeldungNoetig, ApiError } from '#lib/api/client.ts';
import type * as Client from '#lib/api/client.ts';
import { rueckfall } from '#lib/rueckfall.ts';
import type { PcBenutzer, PcReport, Verlauf } from '$vertrag';

const pc = vi.fn<() => Promise<PcReport>>();
const pcBenutzer = vi.fn<() => Promise<PcBenutzer>>();
const verlauf = vi.fn<() => Promise<Verlauf>>();
const verlaufVoll = vi.fn<() => Promise<Verlauf>>();
vi.mock('#lib/api/client.ts', async (original) => ({
	...(await original<typeof Client>()),
	api: {
		pc: () => pc(),
		pcBenutzer: () => pcBenutzer(),
		verlauf: () => verlauf(),
		verlaufVoll: () => verlaufVoll()
	}
}));

const { load } = await import('./+page.ts');

const bericht = { snapshot: { host: 'PC' } } as unknown as PcReport;
const benutzer = {
	schluessel: { vollstaendig: 'ja', gespeichert: 1, fehlen: 0, grund: null },
	bote: 'da'
} as unknown as PcBenutzer;
const pcVerlauf: Verlauf = {
	ereignisse: [],
	seit: '',
	bis: '',
	hinweise: [
		{ art: 'ohne-anmeldung', text: 'Ohne Anmeldung fehlen die Starts, Meldungen und Vorgänge.' }
	]
};
const vollerVerlauf: Verlauf = { ereignisse: [], seit: '', bis: '', hinweise: [] };
const v = { verlauf: vollerVerlauf, fehler: null, ohneAnmeldung: false };
const ohne = { verlauf: pcVerlauf, fehler: null, ohneAnmeldung: true };

async function lade(): Promise<unknown> {
	const event = { fetch, depends: () => undefined } as unknown as Parameters<typeof load>[0];
	return load(event);
}

describe('Laden der Seite PC', () => {
	beforeEach(() => {
		rueckfall.vergessen();
		pc.mockReset();
		pcBenutzer.mockReset();
		pc.mockResolvedValue(bericht);
		verlauf.mockReset().mockResolvedValue(pcVerlauf);
		verlaufVoll.mockReset().mockResolvedValue(vollerVerlauf);
	});
	it('lädt Rechner-Werte, Benutzer-Daten und Verlauf', async () => {
		pcBenutzer.mockResolvedValue(benutzer);
		expect(await lade()).toEqual({ pc: bericht, benutzer, grund: null, verlauf: v });
	});
	it('ohne Windows-Anmeldung (401): Benutzer-Daten null mit Grund, Verlauf vom freien Weg', async () => {
		pcBenutzer.mockRejectedValue(new AnmeldungNoetig());
		verlaufVoll.mockRejectedValue(new AnmeldungNoetig());
		expect(await lade()).toEqual({
			pc: bericht,
			benutzer: null,
			grund: 'anmeldung',
			verlauf: ohne
		});
	});
	it('Hotfix 3a-1: der Verlauf kommt vom geschützten Weg, der freie nur ohne Anmeldung', async () => {
		pcBenutzer.mockResolvedValue(benutzer);
		await lade();
		expect(verlaufVoll).toHaveBeenCalledTimes(1);
		expect(verlauf).not.toHaveBeenCalled();
	});
	it('fremdes Konto (403): Benutzer-Daten null mit Grund', async () => {
		const fremd = new ApiError(403, 'Dieses Windows-Konto darf hier nicht.', 'fremdes-konto');
		pcBenutzer.mockRejectedValue(fremd);
		verlaufVoll.mockRejectedValue(fremd);
		expect(await lade()).toEqual({
			pc: bericht,
			benutzer: null,
			grund: 'fremdes-konto',
			verlauf: ohne
		});
	});
	it('Verlauf mit Fehler: die Seite lädt trotzdem, mit Satz', async () => {
		pcBenutzer.mockResolvedValue(benutzer);
		verlaufVoll.mockRejectedValue(new ApiError(500, 'Speicher zu.'));
		expect(await lade()).toMatchObject({ verlauf: { verlauf: null, fehler: 'Speicher zu.' } });
	});
	it('reicht andere Fehler als HttpError weiter', async () => {
		pcBenutzer.mockRejectedValue(new ApiError(500, 'Kaputt.'));
		const e = await lade().catch((x: unknown) => x);
		expect(isHttpError(e, 500)).toBe(true);
		expect(e).toMatchObject({ body: { message: 'Kaputt.' } });
		pcBenutzer.mockResolvedValue(benutzer);
		pc.mockRejectedValue(new ApiError(503, 'Zu.'));
		expect(await lade().catch((x: unknown) => x)).toMatchObject({
			status: 503,
			body: { message: 'Zu.' }
		});
	});
});
