/**
 * Die Bausteine von /heute (Kalender, Stufe 7b): Uhr mit eigenem Minutentakt in der
 * Anzeigezone, Sonnenbogen (Bild mit Namen und Textfassung, Nacht und Polartag) und
 * Mond (Schmuck ohne Namen, Bild mit Namen). Alle Werte erfunden.
 */
import { render, screen } from '@testing-library/svelte';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { KalenderKopf } from '$vertrag';
import Uhr from './Uhr.svelte';
import Sonnenbogen from './Sonnenbogen.svelte';
import Mond from './Mond.svelte';
import { zoneSetzen } from '#lib/zeit/zone.svelte.ts';

const vorherTz = process.env.TZ;
beforeAll(() => {
	process.env.TZ = 'Europe/Berlin';
});
afterAll(() => {
	if (vorherTz === undefined) delete process.env.TZ;
	else process.env.TZ = vorherTz;
});
afterEach(() => {
	vi.useRealTimers();
	zoneSetzen(null);
});

const KOPF: KalenderKopf = {
	ort: 'Zuhause',
	kennung: 'zuhause',
	folgt_heimat: true,
	datum: '2026-11-27',
	zone: null,
	aufgang: '07:53',
	untergang: '16:18',
	mittag: '12:05',
	aufgang_morgen: '07:54',
	hoehe: 12,
	hoehe_mittag: 18,
	naechster_tag: '2026-11-27T23:00:00Z',
	text: null,
	phase_grad: 250,
	phase: 'abnehmend',
	sued: false,
	naechster_vollmond: '2026-12-24'
};

/** Zeilen der Textfassung als Text je Zelle. */
function zeilen(wurzel: HTMLElement): string[][] {
	return [...wurzel.querySelectorAll('[data-textfassung] tbody tr')].map((tr) =>
		[...tr.querySelectorAll('th, td')].map((z) => z.textContent.trim())
	);
}

describe('Uhr', () => {
	it('eigener Takt: 14:05:59 wird zur vollen Minute 14:06, danach jede Minute', async () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date(2026, 10, 27, 14, 5, 59));
		const { container } = render(Uhr, { datum: '2026-11-27', ort: 'Zuhause' });
		const zeit = (): string => container.querySelector('time')?.textContent ?? '';
		expect(zeit()).toBe('14:05');
		await vi.advanceTimersByTimeAsync(999);
		expect(zeit()).toBe('14:05');
		await vi.advanceTimersByTimeAsync(1);
		expect(zeit()).toBe('14:06');
		await vi.advanceTimersByTimeAsync(60_000);
		expect(zeit()).toBe('14:07');
		expect(container.querySelector('time')).toHaveAttribute('datetime', '2026-11-27T13:07Z');
		expect(container.querySelector('time')).not.toHaveAttribute('role');
		expect(screen.getByText('Freitag, 27. November 2026 · Zuhause')).toBeInTheDocument();
	});

	it('folgt der Anzeigezone, auch wenn sie nach dem Zeichnen kommt', async () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-11-27T13:06:10Z'));
		const { container } = render(Uhr, { datum: '2026-11-28', ort: 'Hütte' });
		expect(container.querySelector('time')?.textContent).toBe('14:06');
		zoneSetzen('Pacific/Auckland');
		await vi.advanceTimersByTimeAsync(0);
		expect(container.querySelector('time')?.textContent).toBe('02:06');
	});
});

describe('Sonnenbogen', () => {
	it('Bild mit Namen; Bogen, Mittagsmarke und Sonne; Textfassung mit denselben Zahlen', () => {
		const { container } = render(Sonnenbogen, { kopf: KOPF, uhrzeit: '10:30' });
		const bild = screen.getByRole('img', {
			name: 'Sonnenbogen: Aufgang 07:53, Mittag 12:05 mit 18°, Untergang 16:18. Um 10:30 steht die Sonne 12° hoch.'
		});
		expect(bild.querySelector('polyline.bogen')).not.toBeNull();
		expect(bild.querySelector('[data-mittag]')).not.toBeNull();
		const sonne = bild.querySelector('[data-sonne]');
		const horizont = Number(bild.querySelector('.horizont')?.getAttribute('y1'));
		expect(Number(sonne?.getAttribute('cy'))).toBeLessThan(horizont);
		expect(zeilen(container)).toEqual([
			['Aufgang', '07:53', ''],
			['Mittag', '12:05', '18°'],
			['Untergang', '16:18', ''],
			['Höhe um', '10:30', '12°']
		]);
		expect(container.querySelector('[data-nacht]')).toBeNull();
	});

	it('nachts unter dem Horizont: vor Mitternacht der Aufgang von morgen, danach der von heute', () => {
		const nacht = { ...KOPF, hoehe: -30 };
		const abend = render(Sonnenbogen, { kopf: nacht, uhrzeit: '22:15' });
		const sonne = abend.container.querySelector('[data-sonne]');
		const horizont = Number(abend.container.querySelector('.horizont')?.getAttribute('y1'));
		expect(Number(sonne?.getAttribute('cy'))).toBeGreaterThan(horizont);
		expect(abend.container.querySelector('[data-nacht]')?.textContent).toBe(
			'Die Sonne steht unter dem Horizont. Sie geht morgen um 07:54 auf.'
		);
		abend.unmount();
		const frueh = render(Sonnenbogen, { kopf: nacht, uhrzeit: '04:10' });
		expect(frueh.container.querySelector('[data-nacht]')?.textContent).toBe(
			'Die Sonne steht unter dem Horizont. Sie geht heute um 07:53 auf.'
		);
	});

	it('Umstellungstag: Zeiten mit Kürzel oder Versatz zeichnen Bogen und Mittagsmarke', () => {
		for (const [auf, mit, unter] of [
			['07:01 CEST', '13:30 CEST', '20:01 CEST'],
			['07:01 +03:00', '13:30 +03:00', '20:01 +03:00']
		] as const) {
			const { container, unmount } = render(Sonnenbogen, {
				kopf: { ...KOPF, aufgang: auf, mittag: mit, untergang: unter },
				uhrzeit: '10:30'
			});
			expect(container.querySelector('polyline.bogen'), auf).not.toBeNull();
			expect(container.querySelector('[data-mittag]'), auf).not.toBeNull();
			unmount();
		}
		// Eine Zeit an einem anderen Tag bleibt draußen.
		const { container } = render(Sonnenbogen, {
			kopf: { ...KOPF, untergang: '00:17 (am 28.11.)' },
			uhrzeit: '10:30'
		});
		expect(container.querySelector('polyline.bogen')).toBeNull();
	});

	it('Polarnacht: kein Bogen, der Satz des Servers steht da', () => {
		const polar = {
			...KOPF,
			aufgang: null,
			untergang: null,
			aufgang_morgen: null,
			hoehe: -3,
			hoehe_mittag: -2,
			text: 'Die Sonne geht heute nicht auf.'
		};
		const { container } = render(Sonnenbogen, { kopf: polar, uhrzeit: '12:00' });
		expect(container.querySelector('polyline.bogen')).toBeNull();
		expect(container.querySelector('[data-polar]')?.textContent).toBe(
			'Die Sonne geht heute nicht auf.'
		);
		expect(container.querySelector('[data-nacht]')).toBeNull();
		expect(zeilen(container)[0]).toEqual(['Aufgang', 'Die Sonne geht heute nicht auf.', '']);
	});
});

describe('Mond', () => {
	it('ohne Namen nur Schmuck (aria-hidden, keine Textfassung)', () => {
		const { container } = render(Mond, { kopf: KOPF });
		expect(container.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
		expect(container.querySelector('[data-textfassung]')).toBeNull();
	});

	it('mit Namen ein Bild in der gewünschten Größe mit Textfassung: Phase und nächster Vollmond', () => {
		const { container } = render(Mond, { kopf: KOPF, groesse: 96, name: 'Mond: abnehmend' });
		const bild = screen.getByRole('img', { name: 'Mond: abnehmend' });
		expect(bild).toHaveAttribute('width', '96');
		expect(bild).not.toHaveAttribute('aria-hidden');
		expect(zeilen(container)).toEqual([
			['Phase', 'abnehmend'],
			['Nächster Vollmond', '24.12.2026']
		]);
	});
});
