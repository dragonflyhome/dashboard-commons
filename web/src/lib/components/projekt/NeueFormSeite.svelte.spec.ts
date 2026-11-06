import { render } from '@testing-library/svelte';
import type { ComponentProps } from 'svelte';
import { describe, expect, it, vi } from 'vitest';

// Die Bauliste als einfaches Objekt (wie die erzeugte Datei): „constructor“
// erbt es von Object.prototype. Grenze und Standardaufbau als Platzhalter.
const { grenze, standard } = vi.hoisted(() => ({ grenze: vi.fn(), standard: vi.fn() }));
vi.mock('#lib/projekte.generated.ts', () => ({
	projektSeiten: { gut: () => Promise.resolve({ default: vi.fn() }) },
	projektUnterseiten: {}
}));
vi.mock('#lib/components/projekt/Grenze.svelte', () => ({ default: grenze }));
vi.mock('#lib/components/projekt/Standardaufbau.svelte', () => ({ default: standard }));

const { default: Page } = await import('./NeueFormSeite.svelte');

function zeige(slug: string): void {
	const props = { kopf: { slug } };
	render(Page, props as unknown as ComponentProps<typeof Page>);
}

describe('Hauptseite der neuen Form', () => {
	it('nimmt für „constructor“ nichts aus Object.prototype, sondern den Standardaufbau', () => {
		grenze.mockClear();
		standard.mockClear();
		zeige('constructor');
		expect(grenze).not.toHaveBeenCalled();
		expect(standard).toHaveBeenCalledTimes(1);
	});
	it('lädt ein Projekt aus der Bauliste in der Fehlergrenze', () => {
		grenze.mockClear();
		standard.mockClear();
		zeige('gut');
		expect(grenze).toHaveBeenCalledTimes(1);
		expect(standard).not.toHaveBeenCalled();
	});
});
