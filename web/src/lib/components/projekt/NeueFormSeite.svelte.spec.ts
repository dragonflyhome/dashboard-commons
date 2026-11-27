import { render, screen } from '@testing-library/svelte';
import type { ComponentProps } from 'svelte';
import { describe, expect, it, vi } from 'vitest';

// Die Bauliste als einfaches Objekt (wie die erzeugte Datei): „constructor“
// erbt es von Object.prototype. Grenze und Standardaufbau als Platzhalter.
// Schritt E2b: Die Seite lädt die Liste dynamisch; der Mock gilt auch dafür.
const { grenze, standard } = vi.hoisted(() => ({ grenze: vi.fn(), standard: vi.fn() }));
vi.mock('#lib/projekte.generated.ts', () => ({
	projektSeiten: { gut: () => Promise.resolve({ default: vi.fn() }) },
	projektUnterseiten: {},
	projektKopfEnde: {}
}));
vi.mock('#lib/components/projekt/Grenze.svelte', () => ({ default: grenze }));
vi.mock('#lib/components/projekt/Standardaufbau.svelte', () => ({ default: standard }));

const { default: Page } = await import('./NeueFormSeite.svelte');

function zeige(slug: string): void {
	const props = { kopf: { slug } };
	render(Page, props as unknown as ComponentProps<typeof Page>);
}

describe('Hauptseite der neuen Form', () => {
	it('nimmt für „constructor“ nichts aus Object.prototype, sondern den Standardaufbau', async () => {
		grenze.mockClear();
		standard.mockClear();
		zeige('constructor');
		await vi.waitFor(() => {
			expect(standard).toHaveBeenCalledTimes(1);
		});
		expect(grenze).not.toHaveBeenCalled();
	});
	it('lädt ein Projekt aus der Bauliste in der Fehlergrenze', async () => {
		grenze.mockClear();
		standard.mockClear();
		zeige('gut');
		await vi.waitFor(() => {
			expect(grenze).toHaveBeenCalledTimes(1);
		});
		expect(standard).not.toHaveBeenCalled();
	});
	it('Schritt E2b: bis die Bauliste da ist, steht „Seite wird geladen …“', () => {
		grenze.mockClear();
		standard.mockClear();
		zeige('gut');
		expect(screen.getByText('Seite wird geladen …')).toBeInTheDocument();
		expect(grenze).not.toHaveBeenCalled();
	});
});
