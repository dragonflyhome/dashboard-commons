// Stylelint für die Projektseiten (Etappe 3b, Teil C). Rust ruft das Skript
// in web/ auf (src/werkzeug/stil.rs, für `dashboard pruefe` und die
// Bauliste des Boten); von Hand geht es genauso:
//
//   node scripts/stil-projekte.mjs < liste.txt
//
// Eingabe auf stdin: die Svelte-Dateien der Projekte, ein absoluter Pfad je
// Zeile. Geprüft werden die Stile darin (<style> und style="…") mit
// denselben Regeln wie web/src (stylelint.config.js), nur mit zwei
// Unterschieden: Kommentare wie `stylelint-disable` wirken in Projekten nicht
// (Auflage aus 3a-2: Abschalten in Projekten verboten), und Meldungen über
// solche Kommentare gibt es darum auch nicht.
//
// Ausgabe auf stdout: je Fund eine Zeile JSON
//   {"nr":0,"zeile":12,"regel":"color-no-hex","text":"…"}
// (`nr` ist die Zeile der Eingabe ab 0), am Ende `FERTIG <dateien> <funde>`.
// Eine Datei, die sich nicht lesen lässt, ergibt einen Fund mit der Regel
// `lesen`; stürzt Stylelint bei einer Datei ab, gibt es für sie einen Fund mit
// der Regel `absturz`, und die anderen Dateien werden weiter geprüft (Etappe
// 3b-2: ein Absturz nimmt nicht alle Projekte aus der Prüfung). Ohne weitere Abhängigkeiten als stylelint und postcss-html, die
// ohnehin in web/node_modules liegen.
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import stylelint from 'stylelint';
import basis from '../stylelint.config.js';

const web = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Die Regeln von web/src, für Projekte ohne Abschalten per Kommentar. */
const config = {
	plugins: basis.plugins,
	rules: basis.rules,
	customSyntax: 'postcss-html',
	reportDescriptionlessDisables: false,
	reportNeedlessDisables: false,
	reportInvalidScopeDisables: false
};

const eingabe = readFileSync(0, 'utf8');
const dateien = eingabe
	.split(/\r?\n/)
	.map((z) => z.trim())
	.filter((z) => z !== '');

let funde = 0;
const aus = [];
for (const [nr, datei] of dateien.entries()) {
	let code;
	try {
		code = readFileSync(datei, 'utf8');
	} catch (e) {
		aus.push(JSON.stringify({ nr, zeile: null, regel: 'lesen', text: String(e?.message ?? e) }));
		funde += 1;
		continue;
	}
	let ergebnis;
	try {
		ergebnis = await stylelint.lint({
			code,
			codeFilename: datei,
			config,
			configBasedir: web,
			ignoreDisables: true
		});
	} catch (e) {
		aus.push(JSON.stringify({ nr, zeile: null, regel: 'absturz', text: String(e?.message ?? e) }));
		funde += 1;
		continue;
	}
	for (const r of ergebnis.results) {
		for (const w of r.warnings) {
			aus.push(JSON.stringify({ nr, zeile: w.line ?? null, regel: w.rule, text: w.text }));
			funde += 1;
		}
	}
}
aus.push(`FERTIG ${dateien.length} ${funde}`);
process.stdout.write(aus.join('\n') + '\n');
