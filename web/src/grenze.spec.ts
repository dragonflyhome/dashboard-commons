/**
 * Die Schichten-Grenze aus eslint.config.js (Etappe 3a-3, ergänzt 3a-4): Das
 * Toolkit in web/vendor binden nur Dateien unter src/lib/katalog ein, statisch
 * wie dynamisch. Geprüft mit dem echten Block `grenze` aus der Konfiguration.
 * Dazu (Auflage aus 3a-4): Die Liste TOOLKIT ist gleich den Paket-Ordnern in
 * web/vendor; ein neues Vendor-Paket ohne Eintrag fällt hier auf.
 */
import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { Linter } from 'eslint';
import { describe, expect, it } from 'vitest';
import { TOOLKIT, TOOLKIT_SCOPES, grenze } from '../eslint.config.js';

const linter = new Linter({ configType: 'flat' });

/** Meldungen der Grenze für `code` in der Datei `datei` (relativ zu web/). */
function pruefe(code: string, datei: string): string[] {
	return linter
		.verify(
			code,
			[
				{ files: ['**/*.ts'], languageOptions: { ecmaVersion: 'latest', sourceType: 'module' } },
				grenze
			],
			{
				filename: datei
			}
		)
		.map((m) => `${m.ruleId ?? 'parser'}: ${m.message}`);
}

const PAKETE = [
	'bits-ui',
	'runed',
	'svelte-toolbelt',
	'tabbable',
	'@internationalized/date',
	'@swc/helpers',
	'@floating-ui/dom',
	'clsx',
	'dequal',
	'esm-env',
	'lz-string',
	'style-to-object',
	'inline-style-parser',
	'tslib'
];

/** Paket-Ordner in web/vendor: <name> und @scope/<name> (wie vendor.spec.ts). */
function vendorPakete(): string[] {
	const vendor = join(import.meta.dirname, '..', 'vendor');
	const namen: string[] = [];
	for (const d of readdirSync(vendor, { withFileTypes: true })) {
		if (!d.isDirectory()) continue;
		if (d.name.startsWith('@')) {
			for (const u of readdirSync(join(vendor, d.name), { withFileTypes: true })) {
				if (u.isDirectory()) namen.push(`${d.name}/${u.name}`);
			}
		} else {
			namen.push(d.name);
		}
	}
	return namen.sort();
}

describe('TOOLKIT in eslint.config.js gleich web/vendor', () => {
	it('jeder Ordner steht in der Liste (oder in einem gesperrten Scope) und umgekehrt', () => {
		const ordner = vendorPakete();
		expect(ordner.length).toBeGreaterThan(0);
		const imScope = (p: string): boolean => TOOLKIT_SCOPES.some((s) => p.startsWith(`${s}/`));
		expect([...TOOLKIT].sort()).toEqual(ordner.filter((p) => !imScope(p)));
		// Jeder gesperrte Scope hat auch Ordner (sonst ist er überflüssig).
		for (const s of TOOLKIT_SCOPES)
			expect(
				ordner.some((p) => p.startsWith(`${s}/`)),
				s
			).toBe(true);
		// bits-ui ist kein Sonderfall: Es steht in der Liste wie jedes Paket.
		expect(TOOLKIT).toContain('bits-ui');
	});

	it.each(vendorPakete())('%s ist außerhalb des Katalogs gesperrt', (p) => {
		expect(
			pruefe(`import x from '${p}';\nexport { x };\n`, 'src/routes/probe/seite.ts')
		).toHaveLength(1);
	});
});

describe('ESLint-Grenze zum Toolkit', () => {
	it.each(PAKETE)(
		'%s: statisch, mit Unterpfad und dynamisch außerhalb des Katalogs gesperrt',
		(p) => {
			const seite = 'src/routes/probe/seite.ts';
			expect(pruefe(`import x from '${p}';\nexport { x };\n`, seite)).toEqual([
				expect.stringMatching(/^no-restricted-imports: .*nur im Katalog/)
			]);
			expect(pruefe(`import x from '${p}/lite';\nexport { x };\n`, seite)).toHaveLength(1);
			expect(pruefe(`export const m = import('${p}');\n`, seite)).toEqual([
				expect.stringMatching(/^no-restricted-syntax: .*nur im Katalog/)
			]);
			expect(pruefe(`export const m = import(\`${p}\`);\n`, seite)).toEqual([
				expect.stringMatching(/^no-restricted-syntax: /)
			]);
		}
	);

	it('im Katalog erlaubt, statisch und dynamisch', () => {
		const katalog = 'src/lib/katalog/grund/Probe.ts';
		expect(pruefe(`import { Tabs } from 'bits-ui';\nexport { Tabs };\n`, katalog)).toEqual([]);
		expect(pruefe(`export const m = import('clsx');\n`, katalog)).toEqual([]);
	});

	it('ähnliche Namen und eigene Dateien bleiben frei', () => {
		const seite = 'src/routes/probe/seite.ts';
		for (const frei of ['clsx-extra', 'tslib2', 'svelte', './lokal.ts', '#lib/format.ts']) {
			expect(pruefe(`export const m = import('${frei}');\n`, seite), frei).toEqual([]);
			expect(pruefe(`import x from '${frei}';\nexport { x };\n`, seite), frei).toEqual([]);
		}
	});
});
