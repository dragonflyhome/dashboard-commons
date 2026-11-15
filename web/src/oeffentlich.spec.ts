/**
 * Schritt E2b: die öffentliche Menge des Baus (scripts/oeffentlich.mjs) an
 * einem echten Testbau mit Vite und Svelte. Ein erfundenes Projekt liegt in
 * einem Ordner außerhalb des Baus und außerhalb jeder Ordnerliste (ohne
 * DASHBOARD_PROJECTS_DIR): Sein Slug und sein Satz stehen in keiner Datei der
 * öffentlichen Menge, wohl aber in einer geschützten. Virtuelle Module führen
 * nicht zum Abbruch; eine statisch geladene Bauliste, eine Projektseite, die die
 * Hülle an der Bauliste vorbei einbindet, und ein öffentliches Asset aus dem
 * Projekt brechen den Bau.
 */
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { build, type Plugin, type Rolldown } from 'vite';
import { afterEach, describe, expect, it } from 'vitest';
import { LISTE, modulPfad, oeffentlicheListe } from '../scripts/oeffentlich.mjs';

const SLUG = 'zqxerfundenslug';
const SATZ = 'Der erfundene Satz steht nur auf der Projektseite';
/** Gleiche Bytes für das Logo der Hülle und das Bild des Projekts. */
const BILD = Buffer.from(
	'89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489',
	'hex'
);

const WEB = fileURLToPath(new URL('..', import.meta.url));
let ordner: string[] = [];
afterEach(() => {
	for (const o of ordner) rmSync(o, { recursive: true, force: true });
	ordner = [];
});

function neuerOrdner(name: string): string {
	// Unter web/, damit svelte aus node_modules auffindbar ist; außerhalb jeder Ordnerliste des Baus.
	const o = mkdtempSync(join(WEB, `.probe-oeffentlich-${name}-`));
	ordner.push(o);
	return o;
}

interface Fall {
	/** Die Hülle lädt die Bauliste statisch statt dynamisch. */
	statisch?: boolean;
	/** Das Projekt bringt ein Bild mit denselben Bytes wie das Logo der Hülle. */
	bildImProjekt?: boolean;
	/** Ein Modul der Hülle bindet die Projektseite selbst ein, an der Bauliste vorbei. */
	huelleBindetSeite?: boolean;
}

/** Ein virtuelles Modul, nur vom Projekt geladen, in denselben Teil wie die Hülle gezwungen. */
const virtuell: Plugin = {
	name: 'probe-virtuell',
	resolveId: (id) => (id === 'virtual:probe' ? '\0virtual:probe' : null),
	load: (id) =>
		id === '\0virtual:probe' ? 'export function probeVirtuell(x) { return [x, x]; }' : null
};

async function testbau(fall: Fall = {}): Promise<Map<string, string>> {
	const wurzel = neuerOrdner('bau');
	const projekt = neuerOrdner('projekt');
	const vorwaerts = (p: string): string => p.replaceAll('\\', '/');
	writeFileSync(join(wurzel, 'logo.png'), BILD);
	writeFileSync(join(projekt, 'bild.png'), BILD);
	writeFileSync(join(wurzel, 'geteilt.js'), 'export const geteilt = (x) => `[${x}]`;\n');
	writeFileSync(join(wurzel, 'start.js'), 'export const start = () => import("./knoten.js");\n');
	writeFileSync(
		join(wurzel, 'knoten.js'),
		[
			'import { geteilt } from "./geteilt.js";',
			'import logo from "./logo.png";',
			fall.statisch
				? 'import * as tabelle from "./lib/projekte.generated.ts"; export const lade = () => Promise.resolve(tabelle);'
				: 'export const lade = () => import("./lib/projekte.generated.ts");',
			'export const huelle = geteilt(logo);',
			fall.huelleBindetSeite
				? `import Seite from "${vorwaerts(join(projekt, 'Seite.svelte'))}"; export const s = Seite;`
				: '',
			''
		].join('\n')
	);
	mkdirSync(join(wurzel, 'lib'));
	const tabelle = join(wurzel, 'lib', 'projekte.generated.ts');
	writeFileSync(
		tabelle,
		`export const projektSeiten = { ${SLUG}: () => import('${vorwaerts(join(projekt, 'Seite.svelte'))}') };\n`
	);
	writeFileSync(
		join(projekt, 'Seite.svelte'),
		[
			'<script>',
			`\timport { geteilt } from '${vorwaerts(join(wurzel, 'geteilt.js'))}';`,
			"\timport { probeVirtuell } from 'virtual:probe';",
			fall.bildImProjekt ? "\timport bild from './bild.png';" : "\tconst bild = '';",
			'</script>',
			'',
			`<p>${SATZ}</p>`,
			'<p>{geteilt(probeVirtuell(1))}{bild}</p>',
			'',
			'<style>',
			'\tp {',
			'\t\tletter-spacing: 3px;',
			'\t}',
			'</style>',
			''
		].join('\n')
	);
	const ausgabe = await build({
		root: wurzel,
		configFile: false,
		logLevel: 'silent',
		plugins: [svelte({ configFile: false }), virtuell, oeffentlicheListe({ tabelle })],
		build: {
			write: false,
			assetsInlineLimit: 0,
			modulePreload: false,
			minify: false,
			rolldownOptions: {
				input: { 'entry/start': join(wurzel, 'start.js'), 'nodes/0': join(wurzel, 'knoten.js') },
				output: {
					entryFileNames: '_app/immutable/[name].[hash].js',
					chunkFileNames: '_app/immutable/chunks/[hash].js',
					assetFileNames: '_app/immutable/assets/[name].[hash][extname]',
					codeSplitting: { groups: [{ name: 'geteilt', test: /geteilt|virtual:probe/ }] }
				}
			}
		}
	});
	const ergebnis = (Array.isArray(ausgabe) ? ausgabe[0] : ausgabe) as Rolldown.RolldownOutput;
	return new Map(
		ergebnis.output.map((o) => [
			o.fileName,
			o.type === 'chunk'
				? o.code
				: typeof o.source === 'string'
					? o.source
					: Buffer.from(o.source).toString('utf8')
		])
	);
}

describe('modulPfad', () => {
	it('lässt virtuelle Module aus und schneidet Abfragen ab', () => {
		expect(modulPfad('\0virtual:probe')).toBeNull();
		expect(modulPfad('virtual:probe')).toBeNull();
		expect(modulPfad('<sveltekit:generated>/app-manifest.js')).toBeNull();
		const p = modulPfad('/x/Seite.svelte?svelte&type=style&lang.css');
		expect(p?.toLowerCase().endsWith('/x/seite.svelte')).toBe(true);
	});
});

describe('Öffentliche Menge des Baus (Vite-Plugin)', () => {
	it('Slug und Satz stehen nur in geschützten Dateien; virtuelle Module brechen nicht ab', async () => {
		const dateien = await testbau();
		const liste = JSON.parse(dateien.get(`_app/${LISTE}`) ?? 'null') as string[];
		expect(Array.isArray(liste)).toBe(true);
		expect(liste).toContain('_app/version.json');
		expect(liste).toContain(`_app/${LISTE}`);
		expect(liste.some((d) => d.startsWith('_app/immutable/entry/start.'))).toBe(true);
		expect(liste.some((d) => d.startsWith('_app/immutable/nodes/0.'))).toBe(true);
		expect(liste.some((d) => d.startsWith('_app/immutable/assets/logo.'))).toBe(true);
		const oeffentlich = [...dateien].filter(([name]) => liste.includes(name));
		const geschuetzt = [...dateien].filter(([name]) => !liste.includes(name));
		for (const [name, inhalt] of oeffentlich) {
			expect(inhalt.includes(SLUG), `Slug in ${name}`).toBe(false);
			expect(inhalt.includes(SATZ), `Satz in ${name}`).toBe(false);
		}
		expect(geschuetzt.some(([, i]) => i.includes(SLUG))).toBe(true);
		expect(geschuetzt.some(([, i]) => i.includes(SATZ))).toBe(true);
		// Das CSS des Projekts ist geschützt, und das virtuelle Modul liegt im öffentlichen Teil der Hülle.
		expect(geschuetzt.some(([n, i]) => n.endsWith('.css') && i.includes('letter-spacing'))).toBe(
			true
		);
		expect(oeffentlich.some(([, i]) => i.includes('probeVirtuell'))).toBe(true);
	});

	it('bricht ab, wenn die Hülle die Bauliste statisch lädt', async () => {
		await expect(testbau({ statisch: true })).rejects.toThrow(
			/Bauliste stünde in der öffentlichen Menge/
		);
	});

	it('bricht ab, wenn die Hülle eine Projektseite an der Bauliste vorbei einbindet', async () => {
		await expect(testbau({ huelleBindetSeite: true })).rejects.toThrow(
			/Hülle erreicht 1 eigene Projektseiten ohne die Bauliste/
		);
	});

	it('bricht ab, wenn ein öffentliches Asset aus dem Projekt stammt', async () => {
		await expect(testbau({ bildImProjekt: true })).rejects.toThrow(
			/Asset stammt aus einer eigenen Projektseite/
		);
	});
});
