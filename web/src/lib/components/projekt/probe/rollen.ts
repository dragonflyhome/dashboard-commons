import { vi } from 'vitest';

/**
 * jsdom kennt scrollIntoView nicht: ein Spy an seiner Stelle, der je Aufruf
 * festhält, ob ein Hinweis (`role="alert"`) dann schon steht. `ende` räumt ihn weg.
 */
export function rollenBeobachten(): {
	rollen: ReturnType<typeof vi.fn<(this: Element, arg?: ScrollIntoViewOptions) => void>>;
	hinweisDa: boolean[];
	ende: () => void;
} {
	const hinweisDa: boolean[] = [];
	const rollen = vi.fn<(this: Element, arg?: ScrollIntoViewOptions) => void>(() => {
		hinweisDa.push(document.querySelector('[role="alert"]') !== null);
	});
	Element.prototype.scrollIntoView = rollen;
	return {
		rollen,
		hinweisDa,
		ende: () => {
			Reflect.deleteProperty(Element.prototype, 'scrollIntoView');
		}
	};
}
