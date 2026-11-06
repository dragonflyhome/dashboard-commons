// Legt src/lib/projekte.generated.ts in der leeren Fassung an, wenn die Datei
// fehlt. Die echte Liste schreibt Rust (`dashboard pruefe --bauliste` und der
// Bote vor jedem Bau); eingecheckt ist die Datei nicht. So gehen build, check
// und test auch in einem frischen Stand. Gibt es die Datei, bleibt sie, wie sie ist.
//
// Aufruf (über package.json vor build, check und test):
//   node scripts/projekte-vorbereiten.mjs [ziel]
// Ohne Abhängigkeiten.
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const web = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const ziel = process.argv[2]
	? resolve(process.argv[2])
	: resolve(web, 'src', 'lib', 'projekte.generated.ts');

// Genau die Form aus src/werkzeug/scanner.rs (bauliste_text), mit leeren Listen.
const leer = [
	'// Erzeugt von dashboard pruefe --bauliste. Nicht von Hand ändern.',
	'',
	"type Laden = () => Promise<{ default: import('svelte').Component }>;",
	'',
	'export const projektSeiten: Record<string, Laden> = {};',
	'',
	'export const projektUnterseiten: Record<string, Laden> = {};',
	''
].join('\n');

if (!existsSync(ziel)) {
	mkdirSync(dirname(ziel), { recursive: true });
	writeFileSync(ziel, leer);
	console.log(`projekte-vorbereiten: leere Fassung angelegt (${ziel})`);
}
