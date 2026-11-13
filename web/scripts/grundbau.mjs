// Fassung des Baus für SvelteKit (`kit.version.name`, landet in
// _app/version.json): `<Abdruck von web/src>-<Bauzeit>`. Der Dienst nimmt
// einen Bau des Boten nur an, wenn der Abdruck zur eingespielten Oberfläche
// passt (src/app.rs, `grundbau`). Was der Bau aus dem Inhalt schreibt (die
// Bauliste und die Typen der Projekte), zählt nicht mit; eine geänderte
// Projektseite ändert den Abdruck also nicht. Die Bauzeit hält die Fassung je
// Bau verschieden (Neuladen bei neuer Fassung, src/lib/fassung.ts).
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

/** @param {string} pfad relativ zu src, mit `/` */
const ausInhalt = (pfad) =>
	// Segmentweise statt als ein Pfad: tests/inhalt_getrennt.rs sucht diesen Pfad als Zeichenkette.
	pfad === 'lib/projekte.generated.ts' ||
	pfad.split('/').slice(0, 3).join('|') === 'lib|vertrag|projekte';

/**
 * Abdruck aller Dateien unter `src` (Pfad und Inhalt), 16 Hex-Zeichen.
 * @param {string} src
 * @returns {string}
 */
export function grundbauAbdruck(src) {
	const h = createHash('sha256');
	const dateien = readdirSync(src, { recursive: true, encoding: 'utf8' })
		.map((p) => p.replaceAll('\\', '/'))
		.filter((p) => !ausInhalt(p) && statSync(join(src, p)).isFile())
		.sort();
	for (const p of dateien) {
		h.update(p)
			.update('\0')
			.update(readFileSync(join(src, p)))
			.update('\0');
	}
	return h.digest('hex').slice(0, 16);
}

/**
 * @param {string} src
 * @param {number} [jetzt]
 * @returns {string}
 */
export function grundbauFassung(src, jetzt = Date.now()) {
	return `${grundbauAbdruck(src)}-${jetzt}`;
}
