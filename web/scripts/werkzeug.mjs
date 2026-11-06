// Abkürzung auf die Befehle von dashboard.exe neben dem Server (Etappe 4a):
//
//   npm run pruefe -- <slug>                  → dashboard.exe pruefe <slug>
//
// Nimmt die neueste fertige dashboard.exe aus target/release,
// target/debug oder dem Bauordner des Dienstes
// (%LOCALAPPDATA%/Dashboard/cache/dienst/release, die eingespielte Fassung;
// ein Bau startet hier nicht) und führt sie im Ordner des
// Repos aus, damit `projects/` dasselbe ist wie für den Server. Ohne
// Fenster; Ausgabe und Rückgabewert kommen unverändert durch. Ohne
// Abhängigkeiten.
import { spawnSync } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const name = process.platform === 'win32' ? 'dashboard.exe' : 'dashboard';
const kandidaten = [
	resolve(repo, 'target', 'release', name),
	resolve(repo, 'target', 'debug', name),
	...(process.env.LOCALAPPDATA
		? [resolve(process.env.LOCALAPPDATA, 'Dashboard', 'cache', 'dienst', 'release', name)]
		: [])
]
	.filter((p) => existsSync(p))
	.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
const programm = kandidaten[0];

if (programm === undefined) {
	console.error(
		'dashboard.exe ist noch nicht gebaut. Bitte im Ordner des Dashboards einmal `cargo build --release` ausführen.'
	);
	process.exit(2);
}

const lauf = spawnSync(programm, process.argv.slice(2), {
	cwd: repo,
	stdio: 'inherit',
	windowsHide: true
});
if (lauf.error) {
	console.error(`dashboard.exe ließ sich nicht starten: ${lauf.error.message}`);
	process.exit(2);
}
process.exit(lauf.status ?? 2);
