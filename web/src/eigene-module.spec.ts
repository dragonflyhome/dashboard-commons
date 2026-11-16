/**
 * Worktree gleich Haupt-Checkout: lint, check und test brechen ab, wenn
 * web/node_modules eine Verknüpfung ist (scripts/projekte-vorbereiten.mjs
 * --eigene-module). Über die Junction lösten TypeScript und eslint ./$types
 * gegen den anderen Checkout auf und meldeten falsche no-unsafe-* Fehler.
 */
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
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
