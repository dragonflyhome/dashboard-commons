// Symbole aus dem Logo erzeugen: rendert web/static/logo.svg (helles Thema)
// mit Edge ohne Fenster (headless, über das DevTools-Protokoll) und schreibt
//
//   web/static/favicon.ico            PNG-Einträge 16, 24, 32, 48, 64, 128, 256
//   web/static/icons/symbol-192.png   App-Symbol (Manifest-Rückfall)
//   web/static/icons/symbol-256.png   dasselbe Bild wie der 256er im ICO
//
// Aufruf in web/, nach jeder Änderung an logo.svg (Anleitung, „das Logo
// tauschen“):
//
//   node scripts/symbole-erzeugen.mjs
//   node scripts/symbole-erzeugen.mjs --ziel <ordner> --vorschau <ordner>
//
// --ziel schreibt statt nach web/static in einen anderen Ordner (zum
// Ausprobieren), --vorschau legt dazu hell-192.png, dunkel-192.png und
// hell-32.png zum Ansehen ab. Die PNGs tragen nur IHDR, IDAT und IEND, keine
// Metadaten (logo.spec.ts). Edge läuft mit eigenem, frischen Profil im
// Temp-Ordner auf einem freien Port, den Edge selbst wählt; danach wird Edge
// samt Kindprozessen beendet und das Profil gelöscht. Ohne Abhängigkeiten:
// nur Node und das installierte Edge. Die reinen Teile (Blockfilter, ICO)
// prüft symbole.spec.ts ohne Edge.
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

/** Größen im favicon.ico, in dieser Reihenfolge (logo.spec.ts). */
export const ICO_GROESSEN = [16, 24, 32, 48, 64, 128, 256];

/** Die Blöcke, die ein PNG zum Anzeigen braucht; alles andere sind Metadaten. */
const BILD_BLOECKE = new Set(['IHDR', 'IDAT', 'IEND']);

/**
 * Ein PNG ohne Metadaten: nur Signatur, IHDR, IDAT und IEND, in der
 * ursprünglichen Reihenfolge und unverändert (die Prüfsummen gelten je
 * Block weiter). Bilder mit Palette (Farbtyp 3) brauchen PLTE und werden
 * abgelehnt; Edge liefert RGBA.
 * @param {Buffer} png
 * @returns {Buffer}
 */
export function nurBildBloecke(png) {
	if (png.length < 8 || png.toString('latin1', 1, 4) !== 'PNG') {
		throw new Error('kein PNG');
	}
	/** @type {Buffer[]} */
	const teile = [png.subarray(0, 8)];
	let i = 8;
	while (i + 8 <= png.length) {
		const laenge = png.readUInt32BE(i);
		const typ = png.toString('latin1', i + 4, i + 8);
		const ende = i + 12 + laenge;
		if (ende > png.length) throw new Error(`Block ${typ} ist abgeschnitten`);
		if (typ === 'IHDR' && png[i + 8 + 9] === 3) {
			throw new Error('PNG mit Palette: PLTE wäre nötig');
		}
		if (BILD_BLOECKE.has(typ)) teile.push(png.subarray(i, ende));
		i = ende;
	}
	return Buffer.concat(teile);
}

/**
 * Packt PNGs in eine ICO-Datei (Kopf, je Bild ein Eintrag zu 16 Byte, dann
 * die PNGs). 256 Pixel stehen im Eintrag als 0.
 * @param {{ groesse: number, png: Buffer }[]} bilder
 * @returns {Buffer}
 */
export function icoPacken(bilder) {
	const kopf = Buffer.alloc(6 + 16 * bilder.length);
	kopf.writeUInt16LE(0, 0);
	kopf.writeUInt16LE(1, 2);
	kopf.writeUInt16LE(bilder.length, 4);
	let versatz = kopf.length;
	bilder.forEach(({ groesse, png }, n) => {
		if (groesse < 1 || groesse > 256)
			throw new Error(`Größe ${String(groesse)} passt nicht ins ICO`);
		const o = 6 + 16 * n;
		kopf[o] = groesse === 256 ? 0 : groesse;
		kopf[o + 1] = groesse === 256 ? 0 : groesse;
		kopf.writeUInt16LE(1, o + 4);
		kopf.writeUInt16LE(32, o + 6);
		kopf.writeUInt32LE(png.length, o + 8);
		kopf.writeUInt32LE(versatz, o + 12);
		versatz += png.length;
	});
	return Buffer.concat([kopf, ...bilder.map((b) => b.png)]);
}

/** Edge an den üblichen Orten unter Windows. */
function edgeFinden() {
	const orte = [
		process.env['ProgramFiles(x86)'],
		process.env.ProgramFiles,
		process.env.LOCALAPPDATA
	]
		.filter((o) => o !== undefined && o !== '')
		.map((o) => join(String(o), 'Microsoft', 'Edge', 'Application', 'msedge.exe'));
	return orte.find((p) => existsSync(p));
}

/** @param {number} ms */
const warten = (ms) => new Promise((ok) => setTimeout(ok, ms));

/**
 * Wartet, bis Edge den gewählten Port in DevToolsActivePort im Profil
 * schreibt (Start mit --remote-debugging-port=0), und gibt die Seite zurück.
 * @param {string} profil
 * @returns {Promise<string>} webSocketDebuggerUrl der ersten Seite
 */
async function seiteFinden(profil) {
	const datei = join(profil, 'DevToolsActivePort');
	for (let n = 0; n < 150; n++) {
		try {
			const port = Number(readFileSync(datei, 'utf8').split('\n')[0]);
			if (port > 0) {
				const liste = /** @type {{ type: string, webSocketDebuggerUrl: string }[]} */ (
					await (await fetch(`http://127.0.0.1:${String(port)}/json/list`)).json()
				);
				const seite = liste.find((t) => t.type === 'page');
				if (seite) return seite.webSocketDebuggerUrl;
			}
		} catch {
			// Edge startet noch.
		}
		await warten(100);
	}
	throw new Error('Edge antwortet nicht (DevToolsActivePort fehlt)');
}

/**
 * Verbindung zum DevTools-Protokoll: senden(methode, parameter) → Ergebnis.
 * @param {string} adresse
 */
async function verbinden(adresse) {
	const ws = new WebSocket(adresse);
	await new Promise((ok, nein) => {
		ws.addEventListener('open', ok, { once: true });
		ws.addEventListener('error', nein, { once: true });
	});
	let nr = 0;
	/** @type {Map<number, { ok: (r: any) => void, nein: (e: Error) => void }>} */
	const offen = new Map();
	ws.addEventListener('message', (e) => {
		const m = JSON.parse(String(e.data));
		const o = offen.get(m.id);
		if (!o) return;
		offen.delete(m.id);
		if (m.error) o.nein(new Error(JSON.stringify(m.error)));
		else o.ok(m.result);
	});
	/**
	 * @param {string} method
	 * @param {object} [params]
	 * @returns {Promise<any>}
	 */
	const senden = (method, params = {}) =>
		new Promise((ok, nein) => {
			const id = ++nr;
			offen.set(id, { ok, nein });
			ws.send(JSON.stringify({ id, method, params }));
		});
	ws.addEventListener('close', () => {
		for (const o of offen.values()) o.nein(new Error('Verbindung zu Edge geschlossen'));
		offen.clear();
	});
	return { senden, schliessen: () => ws.close() };
}

/**
 * Rendert das SVG in Edge zu PNGs.
 * @param {string} adresse
 * @param {Buffer} svg
 */
async function zeichner(adresse, svg) {
	const { senden, schliessen } = await verbinden(adresse);
	await senden('Page.enable');
	await senden('Emulation.setDefaultBackgroundColorOverride', {
		color: { r: 0, g: 0, b: 0, a: 0 }
	});
	const daten = `data:image/svg+xml;base64,${svg.toString('base64')}`;
	/**
	 * @param {number} groesse
	 * @param {'light' | 'dark'} schema
	 */
	const rendern = async (groesse, schema) => {
		await senden('Emulation.setDeviceMetricsOverride', {
			width: groesse,
			height: groesse,
			deviceScaleFactor: 1,
			mobile: false
		});
		await senden('Emulation.setEmulatedMedia', {
			features: [{ name: 'prefers-color-scheme', value: schema }]
		});
		const html = `<!doctype html><style>html,body{margin:0;background:transparent}img{display:block}</style><img src="${daten}" width="${String(groesse)}" height="${String(groesse)}">`;
		await senden('Page.navigate', {
			url: `data:text/html;base64,${Buffer.from(html).toString('base64')}`
		});
		// Warten, bis das Bild dekodiert ist.
		await senden('Runtime.evaluate', {
			expression: 'document.images[0].decode()',
			awaitPromise: true
		});
		await warten(100);
		const { data } = await senden('Page.captureScreenshot', {
			format: 'png',
			clip: { x: 0, y: 0, width: groesse, height: groesse, scale: 1 }
		});
		return nurBildBloecke(Buffer.from(data, 'base64'));
	};
	return { rendern, schliessen };
}

/**
 * Löscht das Profil; gelingt das, läuft kein Edge mehr darauf.
 * @param {string} profil
 */
function profilLoeschen(profil) {
	try {
		rmSync(profil, { recursive: true, force: true });
		return true;
	} catch {
		return false;
	}
}

/**
 * Beendet Edge samt Kindprozessen und löscht das Profil. Edge gibt sich beim
 * Start an einen neuen Hauptprozess weiter, darum hilft kill() am gestarteten
 * Prozess nicht: zuerst Browser.close über das DevTools-Protokoll (schließt
 * alle Kinder), sonst unter Windows jeder msedge.exe mit genau diesem Profil
 * in der Befehlszeile.
 * @param {import('node:child_process').ChildProcess} edge
 * @param {string} profil
 */
async function edgeBeenden(edge, profil) {
	try {
		const port = Number(readFileSync(join(profil, 'DevToolsActivePort'), 'utf8').split('\n')[0]);
		const version = /** @type {{ webSocketDebuggerUrl: string }} */ (
			await (await fetch(`http://127.0.0.1:${String(port)}/json/version`)).json()
		);
		const { senden } = await verbinden(version.webSocketDebuggerUrl);
		await Promise.race([senden('Browser.close').catch(() => undefined), warten(2000)]);
	} catch {
		// Edge ist schon weg oder antwortet nicht mehr.
	}
	for (let n = 0; n < 50; n++) {
		if (profilLoeschen(profil)) return;
		await warten(100);
	}
	if (process.platform === 'win32') {
		const pfad = profil.replace(/'/g, "''");
		spawnSync(
			'powershell.exe',
			[
				'-NoProfile',
				'-NonInteractive',
				'-Command',
				`Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" | Where-Object { $_.CommandLine -like '*${pfad}*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`
			],
			{ stdio: 'ignore', windowsHide: true }
		);
	} else {
		edge.kill('SIGKILL');
	}
	for (let n = 0; n < 50; n++) {
		if (profilLoeschen(profil)) return;
		await warten(100);
	}
	throw new Error(`Edge-Profil ließ sich nicht löschen: ${profil}`);
}

/** @param {string[]} argv */
function argumente(argv) {
	/** @type {{ ziel: string | null, vorschau: string | null }} */
	const a = { ziel: null, vorschau: null };
	for (let i = 0; i < argv.length; i++) {
		const name = argv[i];
		const wert = argv[i + 1];
		if ((name === '--ziel' || name === '--vorschau') && wert !== undefined) {
			a[name === '--ziel' ? 'ziel' : 'vorschau'] = resolve(wert);
			i++;
		} else {
			throw new Error(`Unbekanntes Argument: ${String(name)}`);
		}
	}
	return a;
}

async function main() {
	const web = resolve(dirname(fileURLToPath(import.meta.url)), '..');
	const { ziel, vorschau } = argumente(process.argv.slice(2));
	const statisch = ziel ?? join(web, 'static');
	const svg = readFileSync(join(web, 'static', 'logo.svg'));
	const programm = edgeFinden();
	if (programm === undefined) throw new Error('Microsoft Edge ist nicht installiert');

	const profil = mkdtempSync(join(tmpdir(), 'symbole-edge-'));
	const edge = spawn(
		programm,
		[
			'--headless=new',
			'--remote-debugging-port=0',
			`--user-data-dir=${profil}`,
			'--no-first-run',
			'--no-default-browser-check',
			'--disable-sync',
			'--disable-background-networking',
			'--disable-component-update',
			'--disable-extensions',
			'--disable-component-extensions-with-background-pages',
			'--hide-scrollbars',
			'--force-device-scale-factor=1',
			'about:blank'
		],
		{ stdio: 'ignore', windowsHide: true }
	);
	// Abbruch mit Strg+C: Edge und Profil trotzdem aufräumen.
	const abbruch = () => {
		void edgeBeenden(edge, profil).finally(() => process.exit(130));
	};
	process.once('SIGINT', abbruch);
	try {
		const { rendern, schliessen } = await zeichner(await seiteFinden(profil), svg);
		/** @type {{ groesse: number, png: Buffer }[]} */
		const bilder = [];
		for (const groesse of ICO_GROESSEN)
			bilder.push({ groesse, png: await rendern(groesse, 'light') });
		const s256 = bilder.find((b) => b.groesse === 256)?.png;
		if (s256 === undefined) throw new Error('256er fehlt');
		mkdirSync(join(statisch, 'icons'), { recursive: true });
		writeFileSync(join(statisch, 'favicon.ico'), icoPacken(bilder));
		writeFileSync(join(statisch, 'icons', 'symbol-256.png'), s256);
		writeFileSync(join(statisch, 'icons', 'symbol-192.png'), await rendern(192, 'light'));
		if (vorschau !== null) {
			mkdirSync(vorschau, { recursive: true });
			writeFileSync(join(vorschau, 'hell-192.png'), await rendern(192, 'light'));
			writeFileSync(join(vorschau, 'dunkel-192.png'), await rendern(192, 'dark'));
			writeFileSync(join(vorschau, 'hell-32.png'), bilder[2]?.png ?? Buffer.alloc(0));
		}
		schliessen();
		console.log(`Symbole geschrieben nach ${statisch}`);
	} finally {
		process.off('SIGINT', abbruch);
		await edgeBeenden(edge, profil);
	}
}

// Nur als Programm ausführen, nicht beim Import im Test.
if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
	main().catch((/** @type {unknown} */ fehler) => {
		console.error(fehler instanceof Error ? fehler.message : fehler);
		process.exitCode = 1;
	});
}
