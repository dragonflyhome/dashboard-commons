import { describe, expect, it } from 'vitest';
import { scrollShadow } from './scrollschatten.ts';

function box(scrollWidth: number, clientWidth: number): HTMLDivElement {
	const el = document.createElement('div');
	Object.defineProperty(el, 'scrollWidth', { value: scrollWidth });
	Object.defineProperty(el, 'clientWidth', { value: clientWidth });
	document.body.append(el);
	return el;
}

describe('scrollShadow', () => {
	it('ohne Überlauf keine Schattenklasse', () => {
		const el = box(300, 300);
		const a = scrollShadow(el);
		expect(el.classList.contains('schatten-links')).toBe(false);
		expect(el.classList.contains('schatten-rechts')).toBe(false);
		a.destroy();
		el.remove();
	});

	it('mit Überlauf Schatten am Rand, in dessen Richtung noch etwas kommt', () => {
		const el = box(800, 300);
		const a = scrollShadow(el);
		expect(el.className).toBe('schatten-rechts');
		el.scrollLeft = 200;
		el.dispatchEvent(new Event('scroll'));
		expect(el.classList.contains('schatten-links')).toBe(true);
		expect(el.classList.contains('schatten-rechts')).toBe(true);
		el.scrollLeft = 500;
		el.dispatchEvent(new Event('scroll'));
		expect(el.className).toBe('schatten-links');
		a.destroy();
		expect(el.className).toBe('');
		el.remove();
	});
});
