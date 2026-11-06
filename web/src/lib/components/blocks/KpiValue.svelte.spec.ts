import { render } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import KpiValue from './KpiValue.svelte';

/** Stellt `document.hidden` nach (jsdom sagt sonst immer „sichtbar“). */
function verborgen(wert: boolean): void {
	Object.defineProperty(document, 'hidden', { configurable: true, get: () => wert });
}

function wechsel(wert: boolean): void {
	verborgen(wert);
	document.dispatchEvent(new Event('visibilitychange'));
	flushSync();
}

/** Stellt prefers-reduced-motion: reduce nach. */
function reduziert(): void {
	vi.spyOn(window, 'matchMedia').mockImplementation(
		(query: string) =>
			({
				matches: query.includes('reduce'),
				media: query,
				onchange: null,
				addEventListener: () => undefined,
				removeEventListener: () => undefined,
				dispatchEvent: () => false
			}) as unknown as MediaQueryList
	);
}

/**
 * Frames sammeln statt laufen lassen: requestAnimationFrame steht still, bis
 * ein Test `frames()` ruft. Svelte fordert für alle Tweens nur einen Frame an;
 * darum leert jeder Test am Ende die wartenden Frames, sonst hinge der nächste.
 */
let wartend: FrameRequestCallback[] = [];

/**
 * Ruft die wartenden Frames auf, mit einer Zeit weit nach dem Ende (Svelte
 * liest die Zeit aus `performance.now()`, nicht aus dem Frame).
 */
function frames(): void {
	const jetzt = wartend;
	wartend = [];
	const spaeter = performance.now() + 10_000;
	const uhr = vi.spyOn(performance, 'now').mockReturnValue(spaeter);
	for (const f of jetzt) f(spaeter);
	uhr.mockRestore();
	flushSync();
}

beforeEach(() => {
	wartend = [];
	vi.stubGlobal('requestAnimationFrame', (f: FrameRequestCallback) => {
		wartend.push(f);
		return wartend.length;
	});
});

afterEach(() => {
	while (wartend.length > 0) frames();
	Reflect.deleteProperty(document, 'hidden');
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
	vi.useRealTimers();
});

describe('KpiValue', () => {
	it('zählt im sichtbaren Tab von 0 hoch', () => {
		verborgen(false);
		const { container } = render(KpiValue, { value: '8 offen' });
		expect(container).toHaveTextContent('0 offen');
	});

	it('Tab beim Start verborgen: sofort der Endwert', () => {
		verborgen(true);
		const { container } = render(KpiValue, { value: '8 offen' });
		expect(container).toHaveTextContent('8 offen');
	});

	it('Tab wird während des Zählens verborgen: sofort der Endwert', () => {
		verborgen(false);
		const { container } = render(KpiValue, { value: '1.200 €' });
		expect(container).toHaveTextContent('0 €');
		wechsel(true);
		expect(container).toHaveTextContent('1.200 €');
	});

	it('Tab wird wieder sichtbar: sofort der Endwert', async () => {
		verborgen(true);
		const r = render(KpiValue, { value: '3,5 %' });
		verborgen(false);
		await r.rerender({ value: '7,5 %' });
		expect(r.container).not.toHaveTextContent('7,5 %');
		wechsel(false);
		expect(r.container).toHaveTextContent('7,5 %');
	});

	it('neuer Wert im verborgenen Tab springt ohne Zählen', async () => {
		verborgen(true);
		const r = render(KpiValue, { value: '3' });
		await r.rerender({ value: '8' });
		expect(r.container).toHaveTextContent('8');
	});

	it('prefers-reduced-motion: Endwert ohne Animation', () => {
		verborgen(false);
		reduziert();
		const { container } = render(KpiValue, { value: '42' });
		expect(container).toHaveTextContent('42');
	});

	it('prefers-reduced-motion: auch ein neuer Wert zählt nicht (kein Frame, keine Frist)', async () => {
		verborgen(false);
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		reduziert();
		const r = render(KpiValue, { value: '3' });
		await r.rerender({ value: '8' });
		expect(r.container).toHaveTextContent('8');
		expect(wartend).toHaveLength(0);
		expect(vi.getTimerCount()).toBe(0);
	});

	describe('requestAnimationFrame steht still, die Seite gilt als sichtbar', () => {
		beforeEach(() => {
			verborgen(false);
			vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		});

		it('nach Dauer + 200 ms steht der Endwert da', () => {
			const { container } = render(KpiValue, { value: '8 offen' });
			expect(container).toHaveTextContent('0 offen');
			vi.advanceTimersByTime(899);
			flushSync();
			expect(container).toHaveTextContent('0 offen');
			vi.advanceTimersByTime(1);
			flushSync();
			expect(container).toHaveTextContent('8 offen');
			expect(vi.getTimerCount()).toBe(0);
		});

		it('ein später Frame nach der Frist setzt nichts mehr', () => {
			const { container } = render(KpiValue, { value: '1.200 €' });
			vi.advanceTimersByTime(900);
			flushSync();
			expect(container).toHaveTextContent('1.200 €');
			frames();
			expect(container).toHaveTextContent('1.200 €');
		});

		it('kommen die Frames doch, wird die Frist gelöscht', async () => {
			const { container } = render(KpiValue, { value: '42' });
			frames();
			// Das Ende des Zählens meldet ein Versprechen; danach aufräumen.
			await Promise.resolve();
			expect(container).toHaveTextContent('42');
			expect(vi.getTimerCount()).toBe(0);
		});

		it('beim Abbau wird die Frist gelöscht', () => {
			const r = render(KpiValue, { value: '8' });
			expect(vi.getTimerCount()).toBe(1);
			r.unmount();
			expect(vi.getTimerCount()).toBe(0);
		});

		it('ein neuer Wert startet eine neue Frist statt einer zweiten', async () => {
			const r = render(KpiValue, { value: '3' });
			vi.advanceTimersByTime(500);
			await r.rerender({ value: '9' });
			expect(vi.getTimerCount()).toBe(1);
			vi.advanceTimersByTime(899);
			flushSync();
			expect(r.container).not.toHaveTextContent('9');
			vi.advanceTimersByTime(1);
			flushSync();
			expect(r.container).toHaveTextContent('9');
		});
	});

	it.each(['01/2025', '02–03/2026', '06/2025 bis 06/2026', '2–3', 'Baumeister (4b)'])(
		'zeigt „%s“ unverändert',
		(value) => {
			verborgen(false);
			const { container } = render(KpiValue, { value });
			expect(container.textContent).toBe(value);
		}
	);
});
