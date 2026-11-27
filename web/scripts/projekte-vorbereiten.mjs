// Legt src/lib/projekte.generated.ts in der leeren Fassung an, wenn die Datei
// fehlt. Die echte Liste schreibt Rust (`dashboard pruefe --bauliste` und der
// Bote vor jedem Bau); eingecheckt ist die Datei nicht. So gehen build, check
// und test auch in einem frischen Stand. Ist die Datei die leere Fassung einer
// älteren Form (nur leere Tabellen, aber nicht genau die heutige), wird sie neu
// geschrieben; eine Liste mit Einträgen bleibt, wie sie ist.
//
// Aufruf (über package.json vor build, check und test):
//   node scripts/projekte-vorbereiten.mjs [--eigene-module] [ziel]
// Ohne Abhängigkeiten.
//
// --eigene-module (vor lint, check und test, auch lint:schnell, check:watch, test:unit): Abbruch, wenn web/node_modules eine
// Verknüpfung (Junction, Symlink) ist. TypeScript löst $app/tsconfig dann hinter
// dem Link auf; dessen rootDirs gehören zum anderen Checkout, ./$types bleibt
// ungelöst (falsche eslint-Meldungen), und svelte-kit sync schreibt dorthin.
import { existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const web = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const argumente = process.argv.slice(2);
const zielArg = argumente.find((a) => !a.startsWith('--'));
const ziel = zielArg ? resolve(zielArg) : resolve(web, 'src', 'lib', 'projekte.generated.ts');

const modulOrdner = join(web, 'node_modules');
if (
	argumente.includes('--eigene-module') &&
	existsSync(modulOrdner) &&
	realpathSync(modulOrdner) !== join(realpathSync(web), 'node_modules')
) {
	console.error(
		`projekte-vorbereiten: ${modulOrdner} ist eine Verknüpfung (Ziel ${realpathSync(modulOrdner)}). Prüfungen brauchen eigene Module: Link entfernen, dann in web/ npm ci --offline (ohne npm-Cache: npm ci).`
	);
	process.exit(1);
}

// Genau die Form aus src/werkzeug/scanner.rs (bauliste_text), mit leeren Listen.
const leer = [
	'// Erzeugt von dashboard pruefe --bauliste. Nicht von Hand ändern.',
	'',
	"type Laden = () => Promise<{ default: import('svelte').Component }>;",
	'',
	'export const projektSeiten: Record<string, Laden> = {};',
	'',
	'export const projektUnterseiten: Record<string, Laden> = {};',
	'',
	'export const projektKopfEnde: Record<string, Laden> = {};',
	''
].join('\n');

/**
 * Mindestens eine leere Tabelle und sonst nur Kommentare und die Typzeile: eine
 * Bauliste ohne Einträge, in welcher Form auch immer. Andere Dateien bleiben.
 */
const nurLeer = (text) => {
	const zeilen = text.split(/\r?\n/).map((z) => z.trim());
	const tabelle = (z) => /^export const \w+: Record<string, Laden> = \{\};$/.test(z);
	return (
		zeilen.some(tabelle) &&
		zeilen.every(
			(z) => z === '' || z.startsWith('//') || /^type Laden = .*;$/.test(z) || tabelle(z)
		)
	);
};

if (!existsSync(ziel)) {
	mkdirSync(dirname(ziel), { recursive: true });
	writeFileSync(ziel, leer);
	console.log(`projekte-vorbereiten: leere Fassung angelegt (${ziel})`);
} else {
	const alt = readFileSync(ziel, 'utf8');
	if (alt !== leer && nurLeer(alt)) {
		writeFileSync(ziel, leer);
		console.log(`projekte-vorbereiten: alte leere Fassung ersetzt (${ziel})`);
	}
}
