import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: FassungFuss } = await import('./FassungFuss.svelte');

describe('Fußzeile: laufende Fassung', () => {
	it.each([
		[false, 'Fassung 0.2.0'],
		[true, 'Fassung 0.2.0 + Unveröffentlicht']
	])('unveröffentlicht %s: „%s“ als Link auf /fassungen', async (unveroeffentlicht, text) => {
		render(FassungFuss, {
			laden: () => Promise.resolve({ version: '0.2.0', unveroeffentlicht })
		});
		const link = await screen.findByRole('link', { name: text });
		expect(link).toHaveAttribute('href', '/fassungen');
	});

	it('ohne Antwort nur der Link', async () => {
		render(FassungFuss, { laden: () => Promise.reject(new Error('weg')) });
		await new Promise((r) => setTimeout(r, 0));
		expect(screen.getByRole('link', { name: 'Fassungen' })).toHaveAttribute('href', '/fassungen');
	});
});
