import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NACHLADEN_FENSTER_MS, Nachlader } from './nachlader.ts';

describe('Nachlader', () => {
	beforeEach(() => {
		vi.useFakeTimers();
	});
	afterEach(() => {
		vi.useRealTimers();
	});

	it('vorgang und speicher innerhalb von 250 ms: eine Ladung', () => {
		const laden = vi.fn();
		const n = new Nachlader(laden);
		n.spaeter(); // vorgang
		vi.advanceTimersByTime(250);
		n.jetzt(); // speicher mit Bereich passkeys
		vi.advanceTimersByTime(NACHLADEN_FENSTER_MS * 2);
		expect(laden).toHaveBeenCalledTimes(1);
	});

	it('vorgang allein lädt nach dem Fenster, mehrere zusammen einmal', () => {
		const laden = vi.fn();
		const n = new Nachlader(laden);
		n.spaeter();
		n.spaeter();
		vi.advanceTimersByTime(NACHLADEN_FENSTER_MS - 1);
		expect(laden).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(laden).toHaveBeenCalledTimes(1);
		n.spaeter();
		vi.advanceTimersByTime(NACHLADEN_FENSTER_MS);
		expect(laden).toHaveBeenCalledTimes(2);
	});

	it('jetzt lädt sofort, stop streicht das Geplante', () => {
		const laden = vi.fn();
		const n = new Nachlader(laden);
		n.jetzt();
		expect(laden).toHaveBeenCalledTimes(1);
		n.spaeter();
		n.stop();
		vi.advanceTimersByTime(NACHLADEN_FENSTER_MS * 2);
		expect(laden).toHaveBeenCalledTimes(1);
	});

	it('das Fenster ist länger als die Bündelzeit des Servers', () => {
		expect(NACHLADEN_FENSTER_MS).toBeGreaterThan(250);
	});
});
