/**
 * Worktree gleich Haupt-Checkout: lint, check und test brechen ab, wenn
 * web/node_modules eine Verknüpfung ist (scripts/projekte-vorbereiten.mjs
 * --eigene-module). Über die Junction lösten TypeScript und eslint ./$types
 * gegen den anderen Checkout auf und meldeten falsche no-unsafe-* Fehler.
 */
import { spawnSync } from 'node:child_process';
import {
	copyFileSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	statSync,
	symlinkSync,
	writeFileSync
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, describe, expect, it } from 'vitest';

const basis = mkdtempSync(join(tmpdir(), 'eigene-module-'));
// Das Ziel der Junction liegt selbst in basis; das Aufräumen greift nie hinaus.
afterAll(() => {
	rmSync(basis, { recursive: true, force: true });
});

function lauf(): { code: number | null; fehler: string } {
	const r = spawnSync(
		process.execPath,
		[
			join(basis, 'web', 'scripts', 'projekte-vorbereiten.mjs'),
			'--eigene-module',
			join(basis, 'liste.ts')
		],
		{ encoding: 'utf8' }
	);
	return { code: r.status, fehler: r.stderr };
}

describe('projekte-vorbereiten --eigene-module', () => {
	mkdirSync(join(basis, 'web', 'scripts'), { recursive: true });
	mkdirSync(join(basis, 'fremd'));
	copyFileSync(
		join(import.meta.dirname, '..', 'scripts', 'projekte-vorbereiten.mjs'),
		join(basis, 'web', 'scripts', 'projekte-vorbereiten.mjs')
	);

	it('bricht bei node_modules als Junction ab', () => {
		symlinkSync(join(basis, 'fremd'), join(basis, 'web', 'node_modules'), 'junction');
		const r = lauf();
		expect(r.code).toBe(1);
		expect(r.fehler).toContain('ist eine Verknüpfung');
		rmSync(join(basis, 'web', 'node_modules'), { recursive: false, force: true });
	});

	it('läuft mit eigenem node_modules', () => {
		mkdirSync(join(basis, 'web', 'node_modules'));
		expect(lauf()).toEqual({ code: 0, fehler: '' });
	});
});

describe('projekte-vorbereiten: leere Bauliste', () => {
	const skript = join(import.meta.dirname, '..', 'scripts', 'projekte-vorbereiten.mjs');
	const liste = join(basis, 'bauliste.generated.ts');
	const vorbereiten = (): string =>
		spawnSync(process.execPath, [skript, liste], { encoding: 'utf8' }).stdout;
	// Die heutige leere Fassung, wie das Skript sie anlegt.
	rmSync(liste, { force: true });
	vorbereiten();
	const heute = readFileSync(liste, 'utf8');

	it('ersetzt die leere Fassung einer älteren Form (vor 0.29.0: zwei Tabellen)', () => {
		const alt = heute
			.split('\n')
			.filter((z) => !z.includes('projektKopfEnde'))
			.join('\n');
		writeFileSync(liste, alt);
		expect(vorbereiten()).toContain('alte leere Fassung ersetzt');
		expect(readFileSync(liste, 'utf8')).toBe(heute);
	});

	it('lässt eine Liste mit Einträgen unangetastet (die schreibt Rust)', () => {
		const mitEintrag = heute.replace(
			'export const projektSeiten: Record<string, Laden> = {};',
			"export const projektSeiten: Record<string, Laden> = {\n\tbeispiel: () => import('./beispiel.svelte')\n};"
		);
		writeFileSync(liste, mitEintrag);
		expect(vorbereiten()).toBe('');
		expect(readFileSync(liste, 'utf8')).toBe(mitEintrag);
	});

	it('lässt eine Datei ohne Tabelle (etwa nur ein Kommentar) in Ruhe', () => {
		writeFileSync(liste, '// schon da\n');
		expect(vorbereiten()).toBe('');
		expect(readFileSync(liste, 'utf8')).toBe('// schon da\n');
	});

	it('lässt die heutige leere Fassung Byte für Byte, wie sie ist', () => {
		writeFileSync(liste, heute);
		const vorher = statSync(liste).mtimeMs;
		expect(vorbereiten()).toBe('');
		expect(readFileSync(liste, 'utf8')).toBe(heute);
		expect(statSync(liste).mtimeMs).toBe(vorher);
	});
});

describe('Verdrahtung in package.json', () => {
	it('lint, check und test beginnen mit der Sperre gegen fremde Module', () => {
		const paket = JSON.parse(
			readFileSync(fileURLToPath(new URL('../package.json', import.meta.url)), 'utf8')
		) as { scripts: Record<string, string> };
		for (const name of ['lint', 'check', 'test']) {
			expect(paket.scripts[name], name).toMatch(
				/^node scripts\/projekte-vorbereiten\.mjs --eigene-module &&/
			);
		}
	});
});
