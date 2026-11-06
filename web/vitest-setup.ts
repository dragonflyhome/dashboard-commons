import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

// jsdom kennt keinen ResizeObserver (bind:clientWidth braucht ihn).
if (typeof globalThis.ResizeObserver === 'undefined') {
	class ResizeObserverStub {
		observe(): void {
			// nichts zu beobachten im Test
		}
		unobserve(): void {
			// nichts
		}
		disconnect(): void {
			// nichts
		}
	}
	globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;
}
// Auch matchMedia fehlt in jsdom (prefers-reduced-motion).
if (typeof window !== 'undefined' && typeof window.matchMedia === 'undefined') {
	window.matchMedia = (query: string): MediaQueryList =>
		({
			matches: false,
			media: query,
			onchange: null,
			addEventListener: () => undefined,
			removeEventListener: () => undefined,
			addListener: () => undefined,
			removeListener: () => undefined,
			dispatchEvent: () => false
		}) as MediaQueryList;
}

// Vitest 5 ruft das Aufräumen der Testing Library nicht mehr von selbst; sonst
// bleiben gerenderte Komponenten über Tests hinweg stehen.
afterEach(() => {
	cleanup();
});
