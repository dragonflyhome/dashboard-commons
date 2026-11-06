import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import Confetti from './Confetti.svelte';

/** matchMedia gibt es in jsdom nicht; hier ohne reduced-motion. */
function ohneReduce(reduce = false): void {
	vi.stubGlobal(
		'matchMedia',
		vi.fn(() => ({ matches: reduce }))
	);
}

describe('Confetti', () => {
	beforeEach(() => {
		vi.unstubAllGlobals();
	});

	it('rieselt nicht schon beim ersten Zeichnen', () => {
		ohneReduce();
		const { container } = render(Confetti, { trigger: 0 });
		expect(container.querySelector('.confetti')).toBeNull();
	});

	it('rieselt, wenn trigger wechselt', async () => {
		ohneReduce();
		const { container, rerender } = render(Confetti, { trigger: 0 });
		await rerender({ trigger: 1 });
		expect(container.querySelector('.confetti')).not.toBeNull();
	});

	it('rieselt mit sofort schon beim ersten Zeichnen, wenn trigger > 0', () => {
		ohneReduce();
		const { container } = render(Confetti, { trigger: 1, sofort: true });
		expect(container.querySelector('.confetti')).not.toBeNull();
	});

	it('rieselt mit sofort nicht beim ersten Zeichnen, wenn trigger 0', () => {
		ohneReduce();
		const { container } = render(Confetti, { trigger: 0, sofort: true });
		expect(container.querySelector('.confetti')).toBeNull();
	});

	it('rieselt ohne sofort nicht beim Start mit trigger 1', () => {
		ohneReduce();
		const { container } = render(Confetti, { trigger: 1 });
		expect(container.querySelector('.confetti')).toBeNull();
	});

	it('sofort mit 0, dann 1: rieselt', async () => {
		ohneReduce();
		const { container, rerender } = render(Confetti, { trigger: 0, sofort: true });
		expect(container.querySelector('.confetti')).toBeNull();
		await rerender({ trigger: 1 });
		expect(container.querySelector('.confetti')).not.toBeNull();
	});

	it('gleicher Wert noch einmal: rieselt nicht', async () => {
		ohneReduce();
		const { container, rerender } = render(Confetti, { trigger: 1 });
		await rerender({ trigger: 1 });
		expect(container.querySelector('.confetti')).toBeNull();
	});

	it('kleinerer Wert: rieselt nicht', async () => {
		ohneReduce();
		const { container, rerender } = render(Confetti, { trigger: 2, sofort: false });
		await rerender({ trigger: 1 });
		expect(container.querySelector('.confetti')).toBeNull();
		await rerender({ trigger: 0 });
		expect(container.querySelector('.confetti')).toBeNull();
	});

	it('sofort mit 0, dann 1, aber reduced-motion: rieselt nicht', async () => {
		ohneReduce(true);
		const { container, rerender } = render(Confetti, { trigger: 0, sofort: true });
		await rerender({ trigger: 1 });
		expect(container.querySelector('.confetti')).toBeNull();
	});

	it('rieselt nicht bei reduced-motion', async () => {
		ohneReduce(true);
		const { container, rerender } = render(Confetti, { trigger: 0 });
		await rerender({ trigger: 1 });
		expect(container.querySelector('.confetti')).toBeNull();
	});

	it('räumt nach der Animation auf, auch wenn trigger danach noch einmal wechselt (3b)', async () => {
		ohneReduce();
		vi.useFakeTimers();
		try {
			const { container, rerender } = render(Confetti, { trigger: 1 });
			await rerender({ trigger: 2 });
			expect(container.querySelectorAll('.confetti span')).toHaveLength(48);
			// Ein kleinerer Wert läuft den Effekt noch einmal, ohne zu rieseln.
			await rerender({ trigger: 0 });
			await vi.advanceTimersByTimeAsync(3300);
			expect(container.querySelector('.confetti')).toBeNull();
		} finally {
			vi.useRealTimers();
		}
	});
});
