import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { REIHEN, reihe } from './reihen.ts';

describe('Reihen', () => {
	it('sind zehn Token, die es in app.css gibt', () => {
		const css = readFileSync(join(import.meta.dirname, '..', 'app.css'), 'utf8');
		expect(REIHEN).toHaveLength(10);
		for (const r of REIHEN) {
			const name = /^var\((--reihe-\d+)\)$/.exec(r)?.[1];
			expect(name).toBeDefined();
			expect(css).toContain(`${String(name)}: #`);
		}
	});
	it('beginnen nach der zehnten von vorn', () => {
		expect(reihe(0)).toBe('var(--reihe-1)');
		expect(reihe(10)).toBe('var(--reihe-1)');
		expect(reihe(13)).toBe('var(--reihe-4)');
	});
});
