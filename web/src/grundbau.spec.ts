/**
 * Der Abdruck des Grundbaus (scripts/grundbau.mjs): Er ändert sich mit jeder
 * Datei unter src, nicht aber mit dem, was der Bau aus dem Inhalt schreibt.
 * Die Fassung hat die Form, die der Dienst liest (src/app.rs, `grundbau`):
 * Abdruck ohne `-`, dann `-`, dann die Bauzeit.
 */
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { grundbauAbdruck, grundbauFassung } from '../scripts/grundbau.mjs';
import { kennungZeit, neuere } from '#lib/fassung.ts';

let wurzel = '';

function src(): string {
	wurzel = mkdtempSync(join(tmpdir(), 'grundbau-'));
	const s = join(wurzel, 'src');
	mkdirSync(join(s, 'lib', 'vertrag', 'projekte'), { recursive: true });
	writeFileSync(join(s, 'lib', 'a.ts'), 'export const a = 1;\n');
	writeFileSync(join(s, 'lib', 'vertrag', 'Plan.ts'), 'export type Plan = {};\n');
	return s;
}

afterEach(() => {
	if (wurzel) rmSync(wurzel, { recursive: true, force: true });
	wurzel = '';
});

describe('grundbauAbdruck', () => {
	it('bleibt bei Bauliste und Projekttypen, ändert sich mit jeder anderen Datei', () => {
		const s = src();
		const vorher = grundbauAbdruck(s);
		expect(vorher).toMatch(/^[0-9a-f]{16}$/);
		writeFileSync(join(s, 'lib', 'projekte.generated.ts'), 'export const x = 1;\n');
		writeFileSync(join(s, 'lib', 'vertrag', 'projekte', 'P.ts'), 'export type P = {};\n');
		expect(grundbauAbdruck(s)).toBe(vorher);
		// Gegenprobe: ein neuer Vertragstyp und eine geänderte Datei ändern ihn.
		writeFileSync(join(s, 'lib', 'vertrag', 'Verlauf.ts'), 'export type V = {};\n');
		const mitVertrag = grundbauAbdruck(s);
		expect(mitVertrag).not.toBe(vorher);
		writeFileSync(join(s, 'lib', 'a.ts'), 'export const a = 2;\n');
		expect(grundbauAbdruck(s)).not.toBe(mitVertrag);
	});

	it('Fassung: Abdruck, Bindestrich, Bauzeit', () => {
		const s = src();
		expect(grundbauFassung(s, 1794553248560)).toBe(`${grundbauAbdruck(s)}-1794553248560`);
	});

	it('der Tab liest aus der Fassung die Bauzeit (src/lib/fassung.ts)', () => {
		const s = src();
		expect(kennungZeit(grundbauFassung(s, 1794553248560))).toBe(1794553248560);
		expect(neuere('20271113-120000', grundbauFassung(s, 1794553248560))).toBe(true);
	});
});
