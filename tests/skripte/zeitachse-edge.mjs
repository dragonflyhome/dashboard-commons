#!/usr/bin/env node
/**
 * Ende-zu-Ende-Probe für Etappe 3a-1 (Verlauf und gemeinsame Zeitachse) mit Edge.
 *
 * Was es tut (alles mit Wegwerf-Daten in einem Temp-Ordner, Port 8788, nie
 * 8787, nie C:\ProgramData\Dashboard):
 *
 * 1. Kopiert target\debug\dashboard.exe in den Temp-Ordner, dazu eine KOPIE von
 *    projects\ und data\ aus dem Repo, und startet den Server mit
 *    `--konsole --einstellungen` (Datenspeicher, Sicherung und Protokoll im
 *    Temp-Ordner, eine Pipe für einen Boten, den es nicht gibt). Die eigene
 *    SID steht als inhaber_sid darin: Edge meldet sich per Negotiate an, die
 *    Seiten zeigen also auch Starts, Meldungen und Vorgänge. Der Sammler liest
 *    die Windows-Ereignisse dieses PCs (nur lesend) in den Wegwerf-Speicher.
 * 2. Startet Edge headless mit Wegwerf-Profil (ohne Hintergrund-Netz und
 *    Komponenten-Updates; alle Kindprozesse ohne Fenster), sperrt über
 *    Fetch.enable jede Anfrage, die nicht an 127.0.0.1 oder localhost geht
 *    (dann besteht die Probe nicht), und spricht über das
 *    DevTools-Protokoll (WebSocket von Node 24) mit ihm. Kein Playwright,
 *    kein Browser-Download, keine Abhängigkeit.
 *
 * Geprüft wird:
 *   - alle Projektseiten mit Zeitstrahl oder Gantt (daten/*.toml), /pc und
 *     die Startseite: keine Fehler in der Konsole, der Baustein ist da
 *   - Dichtestreifen (/pc) und Gantt: Tastatur (+ und - zoomen, Pfeile
 *     verschieben, Pos1 springt auf heute), Verlauf: Pfeile von Eintrag zu
 *     Eintrag, Enter klappt auf
 *   - Touch-Ersatz (Emulation.setTouchEmulationEnabled, Input.dispatchTouchEvent):
 *     Antippen eines Tages im Dichtestreifen, Ziehen über mehrere Tage, Ziehen
 *     im Gantt
 *   - Hotfix 3a-1, Verlauf mit Anmeldung: /api/pc/verlauf/voll ohne Nachweis
 *     401 mit `WWW-Authenticate: Negotiate`; /pc fünfmal laden und einmal
 *     live neu laden (data/pc.toml geändert), jedes Mal mit dem Filter
 *     „Starts“ und ohne den Hinweis „nur mit Anmeldung“; die Seiten fragen
 *     /api/pc/verlauf/voll, nie den freien Weg; 8 gleichzeitige Abrufe von
 *     /api/pc/verlauf/voll aus der Seite haben alle Starts
 *   - 375 px Breite: keine Seite scrollt seitlich
 *   - keine überlappenden Beschriftungen auf den Achsen (Rechtecke der
 *     Beschriftungen, bei 1280 und 375 px)
 *   - Bildschirmfotos (ganze Seite, PNG) der Dashboard-Projektseite, von /pc
 *     und einer Projektseite mit Zeitstrahl bei 1280 und 375 px, wenn
 *     `--bilder <Ordner>` angegeben ist
 *
 * Aufruf aus dem Repo (vorher `cargo build` und ein Bau der Oberfläche):
 *
 *     node tests\skripte\zeitachse-edge.mjs --web <Ordner mit index.html> --bilder <Ordner>
 *
 * Ohne --web nimmt es web\build. Optional: --exe <dashboard.exe>,
 * --edge <msedge.exe>, --port 8788, --behalten (Temp-Ordner nicht löschen),
 * --sammler-sekunden 120 (so lange auf die Windows-Ereignisse warten),
 * --nur /pc,/ (nur diese Seiten besuchen; Bedienung und Bilder laufen trotzdem).
 * Exit 0: alles bestanden, 1: mindestens eine Prüfung nicht. Die Probe läuft
 * auch bei einem Fehlschlag weiter (alle Bilder entstehen) und zählt am Ende
 * auf, was nicht bestanden hat. Am Ende laufen weder Server noch Edge weiter.
 *
 * Netzsperre auf Browser-Ebene (Plan 3a+, edge-gemeinsam.mjs): Zusätzlich
 * zur Seite hängt sich eine zweite Verbindung mit Target.setAutoAttach
 * (flatten) an jedes andere Ziel, auch Service-Worker, Worker, weitere
 * Seiten und Frames, und sperrt dort ebenso mit Fetch.enable. Anfragen des
 * Browser-Prozesses selbst schalten die Startschalter ab. Strg+C räumt
 * Server, Edge und Temp-Ordner auf; ein belegter Port bricht vor dem Start ab.
 */
import { spawn, execFileSync } from 'node:child_process';
import {
	appendFileSync,
	cpSync,
	copyFileSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	rmSync,
	writeFileSync
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { antwortAbwarten } from './cdp-frist.mjs';
import { beiAbbruch, browserSperren, mitFrist, portePruefen, inhaltOrt } from './edge-gemeinsam.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
// Umzug des Inhalts: Kopien aus dashboard-inhalt, wenn es ihn gibt (edge-gemeinsam.mjs).
const INHALT = inhaltOrt(REPO);

function argument(name, standard) {
	const i = process.argv.indexOf(`--${name}`);
	return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : standard;
}

const PORT = Number(argument('port', '8788'));
if (PORT === 8787) {
	console.error('Port 8787 ist der echte Dienst. Bitte einen Testport nehmen.');
	process.exit(2);
}
const EXE = resolve(argument('exe', join(REPO, 'target', 'debug', 'dashboard.exe')));
const WEB = resolve(argument('web', join(REPO, 'web', 'build')));
const EDGE = argument('edge', 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe');
const BILDER = argument('bilder', null);
const BEHALTEN = process.argv.includes('--behalten');
const SAMMLER_SEKUNDEN = Number(argument('sammler-sekunden', '120'));
/** Nur diese Seiten besuchen (Komma-Liste), zum schnellen Nachprüfen; sonst alle. */
const NUR = argument('nur', null)?.split(',') ?? null;
const DEBUG_PORT = PORT + 1000;
const BASIS = `http://localhost:${PORT}`;
/** Programme aus System32 (nicht die gleichnamigen aus Git Bash). */
const SYSTEM = (name) => join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', name);

const ordner = mkdtempSync(join(tmpdir(), 'dashboard-zeitachse-e2e-'));
const prozesse = [];
/** Ob diese Probe ein Edge gestartet hat (nur das schließt das Aufräumen). */
let edgeGestartet = false;
// Ein Aufräumen für alle Wege (finally, Strg+C, unbehandelter Fehler).
const aufraeumen = beiAbbruch(aufraeumenJetzt);
let bestanden = 0;
const fehlgeschlagen = [];
const bilder = [];

/** Eine Prüfung: zählt und läuft weiter, auch wenn sie nicht besteht. */
function pruefe(bedingung, satz, einzelheiten = '') {
	if (bedingung) {
		bestanden++;
		console.log(`  ok    ${satz}`);
	} else {
		fehlgeschlagen.push(einzelheiten ? `${satz} (${einzelheiten})` : satz);
		console.log(`  NEIN  ${satz}${einzelheiten ? `\n        ${einzelheiten}` : ''}`);
	}
	return bedingung;
}

const pause = (ms) => new Promise((r) => setTimeout(r, ms));

async function aufraeumenJetzt() {
	try {
		// Nur ein Edge, das diese Probe gestartet hat; jeder Schritt mit Frist.
		if (!edgeGestartet) throw new Error('kein eigenes Edge');
		const v = await mitFrist(
			fetch(`http://127.0.0.1:${DEBUG_PORT}/json/version`).then((r) => r.json()),
			4000,
			'/json/version'
		);
		const b = await mitFrist(Cdp.verbinden(v.webSocketDebuggerUrl), 3000, 'Verbinden');
		await Promise.race([b.senden('Browser.close', {}, 3000).catch(() => {}), pause(3000)]);
		await pause(1000);
	} catch {
		// Edge läuft nicht (mehr)
	}
	for (const p of prozesse) {
		try {
			execFileSync(SYSTEM('taskkill.exe'), ['/PID', String(p.pid), '/T', '/F'], {
				stdio: 'ignore',
				windowsHide: true
			});
		} catch {
			// schon beendet
		}
	}
	prozesse.length = 0;
	// Was dann noch mit diesem Temp-Ordner läuft (nur Prozesse dieser Probe), beenden.
	try {
		const muster = ordner.replace(/'/g, "''");
		execFileSync(
			SYSTEM('WindowsPowerShell\\v1.0\\powershell.exe'),
			[
				'-NoProfile',
				'-Command',
				`Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains('${muster}') } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`
			],
			{ stdio: 'ignore', windowsHide: true }
		);
	} catch {
		// nichts mehr da
	}
	if (!BEHALTEN) {
		for (let i = 0; i < 10; i++) {
			try {
				rmSync(ordner, { recursive: true, force: true });
				break;
			} catch {
				Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1000);
			}
		}
	}
}

// ----- Server -----

function eigeneSid() {
	const zeile = execFileSync(SYSTEM('whoami.exe'), ['/user', '/fo', 'csv', '/nh'], {
		encoding: 'utf8',
		windowsHide: true
	});
	const sid = zeile.trim().split(',').pop().replace(/"/g, '');
	if (!/^S-1-\d+(-\d+)+$/.test(sid)) throw new Error(`SID nicht lesbar: ${zeile}`);
	return sid;
}

function einstellungen(inhaberSid) {
	const p = (rest) => join(ordner, rest).replace(/\\/g, '/');
	const text = [
		'schema = 1',
		`repo = "${p('repo')}"`,
		`port = ${PORT}`,
		'',
		'[pfade]',
		`daten = "${p('daten/dashboard.redb')}"`,
		`sicherung = "${p('sicherung')}"`,
		`protokoll = "${p('protokoll')}"`,
		`web = "${WEB.replace(/\\/g, '/')}"`,
		`projekte = "${p('repo/projects')}"`,
		'',
		'[konten]',
		`inhaber_sid = "${inhaberSid}"`,
		'',
		'[bote]',
		`pipe = "dashboard-bote-zeitachse-${process.pid}"`,
		`programm = "${p('bin/gibt-es-nicht.exe')}"`,
		''
	].join('\n');
	writeFileSync(join(ordner, 'dienst.toml'), text);
}

async function serverStarten(inhaberSid) {
	einstellungen(inhaberSid);
	const exe = join(ordner, 'bin', 'dashboard.exe');
	const kind = spawn(exe, ['--konsole', '--einstellungen', join(ordner, 'dienst.toml')], {
		env: {
			...process.env,
			USERPROFILE: join(ordner, 'profil'),
			RUST_LOG: 'warn'
		},
		stdio: ['ignore', 'ignore', 'pipe'],
		windowsHide: true
	});
	let fehler = '';
	kind.stderr.on('data', (d) => (fehler += d.toString()));
	prozesse.push(kind);
	for (let i = 0; i < 150; i++) {
		try {
			const r = await fetch(`http://127.0.0.1:${PORT}/healthz`);
			if (r.ok) return kind;
		} catch {
			// noch nicht da
		}
		if (kind.exitCode !== null) throw new Error(`Server beendet: ${fehler}`);
		await pause(200);
	}
	throw new Error(`Server antwortet nicht: ${fehler}`);
}

/** Wartet, bis der Sammler Windows-Ereignisse in den Speicher gelegt hat. */
async function aufSammlerWarten() {
	const ende = Date.now() + SAMMLER_SEKUNDEN * 1000;
	while (Date.now() < ende) {
		try {
			const v = await (await fetch(`http://127.0.0.1:${PORT}/api/pc/verlauf`)).json();
			const n = (v.ereignisse ?? []).filter((e) => e.quelle === 'update').length;
			if (n > 0) return n;
		} catch {
			// noch nicht da
		}
		await pause(2000);
	}
	return 0;
}

async function json(pfad) {
	const r = await fetch(`http://127.0.0.1:${PORT}${pfad}`, {
		headers: { accept: 'application/json' }
	});
	if (!r.ok) throw new Error(`${pfad}: ${r.status}`);
	return r.json();
}

// ----- Edge über das DevTools-Protokoll -----

class Cdp {
	constructor(ws) {
		this.ws = ws;
		this.nr = 0;
		this.offen = new Map();
		this.hoerer = [];
		ws.addEventListener('message', (e) => {
			const m = JSON.parse(String(e.data));
			if (m.id !== undefined && this.offen.has(m.id)) {
				const { ok, nein } = this.offen.get(m.id);
				this.offen.delete(m.id);
				if (m.error) nein(new Error(`${m.error.message} (${JSON.stringify(m.error)})`));
				else ok(m.result);
			} else if (m.method) {
				for (const h of [...this.hoerer]) h(m);
			}
		});
	}

	static async verbinden(url) {
		const ws = new WebSocket(url);
		await new Promise((ok, nein) => {
			ws.addEventListener('open', ok, { once: true });
			ws.addEventListener('error', nein, { once: true });
		});
		return new Cdp(ws);
	}

	/** Mit Zeitablauf (cdp-frist.mjs): eine hängende Antwort lässt die Probe nicht ewig warten. */
	senden(method, params = {}, ms) {
		const id = ++this.nr;
		this.ws.send(JSON.stringify({ id, method, params }));
		return antwortAbwarten(this.offen, id, method, ms);
	}

	warteAuf(method, ms = 20000) {
		return new Promise((ok, nein) => {
			const t = setTimeout(() => nein(new Error(`${method} kam nicht`)), ms);
			const h = (m) => {
				if (m.method === method) {
					clearTimeout(t);
					this.hoerer.splice(this.hoerer.indexOf(h), 1);
					ok(m.params);
				}
			};
			this.hoerer.push(h);
		});
	}

	/**
	 * Netzsperre: Die Seite darf nur mit diesem Server sprechen (127.0.0.1,
	 * localhost). Jede andere Anfrage bricht Fetch.failRequest ab, und sie
	 * steht in `gesperrt`; die Probe besteht dann nicht.
	 */
	async netzSperren() {
		this.gesperrt = [];
		this.hoerer.push((m) => {
			if (m.method !== 'Fetch.requestPaused') return;
			const { requestId, request } = m.params;
			let host = '';
			try {
				host = new URL(request.url).hostname;
			} catch {
				// keine Adresse, gilt als fremd
			}
			if (host === '127.0.0.1' || host === 'localhost' || host === '[::1]') {
				this.senden('Fetch.continueRequest', { requestId }).catch(() => {});
			} else {
				this.gesperrt.push(request.url);
				this.senden('Fetch.failRequest', { requestId, errorReason: 'BlockedByClient' }).catch(
					() => {}
				);
			}
		});
		await this.senden('Fetch.enable', { patterns: [{ urlPattern: '*' }] });
	}

	async auswerten(ausdruck) {
		const r = await this.senden('Runtime.evaluate', {
			expression: ausdruck,
			awaitPromise: true,
			returnByValue: true
		});
		if (r.exceptionDetails) throw new Error(`JS: ${JSON.stringify(r.exceptionDetails)}`);
		return r.result.value;
	}

	async oeffnen(pfad) {
		const geladen = this.warteAuf('Page.loadEventFired');
		// Scheitert das Navigieren, darf diese Frist nicht unbehandelt ablaufen.
		geladen.catch(() => {});
		await this.senden('Page.navigate', { url: `${BASIS}${pfad}` });
		await geladen;
		// Daten kommen nach dem Laden: warten, bis nichts mehr lädt.
		const ende = Date.now() + 8000;
		await pause(600);
		while (Date.now() < ende) {
			const laedt = await this.auswerten(
				`!document.querySelector('main') || document.querySelector('[aria-busy="true"]') !== null`
			).catch(() => true);
			if (!laedt) break;
			await pause(250);
		}
		await pause(500);
	}

	async taste(key, { code = key, vk = 0, text } = {}) {
		await this.senden('Input.dispatchKeyEvent', {
			type: 'keyDown',
			key,
			code,
			windowsVirtualKeyCode: vk,
			...(text === undefined ? {} : { text })
		});
		await this.senden('Input.dispatchKeyEvent', {
			type: 'keyUp',
			key,
			code,
			windowsVirtualKeyCode: vk
		});
		await pause(120);
	}

	async beruehren(typ, punkte) {
		await this.senden('Input.dispatchTouchEvent', {
			type: typ,
			touchPoints: punkte.map(([x, y]) => ({
				x,
				y,
				radiusX: 2,
				radiusY: 2,
				force: 1,
				id: 1
			}))
		});
	}

	async breite(px) {
		await this.senden('Emulation.setDeviceMetricsOverride', {
			width: px,
			height: px < 600 ? 812 : 900,
			deviceScaleFactor: 1,
			mobile: px < 600,
			screenWidth: px,
			screenHeight: px < 600 ? 812 : 900
		});
		await pause(300);
	}
}

const TASTEN = {
	plus: ['+', { code: 'Equal', vk: 187, text: '+' }],
	minus: ['-', { code: 'Minus', vk: 189, text: '-' }],
	links: ['ArrowLeft', { vk: 37 }],
	rechts: ['ArrowRight', { vk: 39 }],
	hoch: ['ArrowUp', { vk: 38 }],
	runter: ['ArrowDown', { vk: 40 }],
	pos1: ['Home', { vk: 36 }],
	enter: ['Enter', { vk: 13, text: '\r' }]
};
async function druecke(cdp, name, mal = 1) {
	const [key, opt] = TASTEN[name];
	for (let i = 0; i < mal; i++) await cdp.taste(key, opt);
}

async function edgeStarten() {
	const profil = join(ordner, 'edge-profil');
	mkdirSync(profil, { recursive: true });
	edgeGestartet = true;
	const edge = spawn(
		EDGE,
		[
			'--headless=new',
			`--remote-debugging-port=${DEBUG_PORT}`,
			`--user-data-dir=${profil}`,
			'--no-first-run',
			'--no-default-browser-check',
			'--disable-sync',
			'--disable-background-networking',
			'--disable-component-update',
			'--disable-extensions',
			'--disable-component-extensions-with-background-pages',
			'--hide-scrollbars',
			// Negotiate auch headless ohne Rückfrage (localhost gilt sonst schon als Intranet).
			'--auth-server-allowlist=localhost,127.0.0.1',
			'--window-size=1280,900',
			'about:blank'
		],
		{ stdio: 'ignore', windowsHide: true }
	);
	prozesse.push(edge);
	for (let i = 0; i < 100; i++) {
		try {
			const liste = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/list`)).json();
			const seite = liste.find((t) => t.type === 'page');
			if (seite) {
				const cdp = await Cdp.verbinden(seite.webSocketDebuggerUrl);
				await cdp.senden('Page.enable');
				await cdp.netzSperren();
				// Auch Service-Worker, Worker und weitere Ziele (edge-gemeinsam.mjs).
				cdp.browserSperre = await browserSperren(DEBUG_PORT, cdp.gesperrt, { ausser: seite.id });
				await cdp.senden('Runtime.enable');
				await cdp.senden('Log.enable');
				return cdp;
			}
		} catch {
			// Edge startet noch
		}
		await pause(200);
	}
	throw new Error('Edge antwortet nicht auf dem Debug-Port');
}

/** Sammelt Fehler aus der Konsole, bis `stopp()` gerufen wird. */
function konsoleMitschreiben(cdp) {
	const fehler = [];
	const h = (m) => {
		if (m.method === 'Runtime.exceptionThrown') {
			const d = m.params.exceptionDetails;
			fehler.push(`Ausnahme: ${d.exception?.description ?? d.text}`.slice(0, 400));
		} else if (m.method === 'Runtime.consoleAPICalled' && m.params.type === 'error') {
			fehler.push(
				`console.error: ${m.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`.slice(
					0,
					400
				)
			);
		} else if (m.method === 'Log.entryAdded' && m.params.entry.level === 'error') {
			fehler.push(
				`${m.params.entry.source}: ${m.params.entry.text} ${m.params.entry.url ?? ''}`.slice(0, 400)
			);
		}
	};
	cdp.hoerer.push(h);
	return {
		fehler,
		stopp: () => cdp.hoerer.splice(cdp.hoerer.indexOf(h), 1)
	};
}

// ----- Prüfungen im Browser -----

/** Bausteine auf der Seite: neu (data-zeit) und alt (Klassen bis 3a). */
const BAUSTEINE = `(() => ({
	verlauf: document.querySelectorAll('[data-zeit="verlauf"]').length,
	gantt: document.querySelectorAll('[data-zeit="gantt"]').length,
	dichte: document.querySelectorAll('[data-zeit="dichte"]').length,
	altTimeline: document.querySelectorAll('.timeline').length,
	altGantt: document.querySelectorAll('.gantt').length,
	fehlerBloecke: [...document.querySelectorAll('.block-error')].map((e) => e.textContent.trim().slice(0, 160))
}))()`;

/**
 * Überlappende Beschriftungen: je Achse (neu: [data-zeit="achse"] mit
 * [data-beschriftung]; alt: das SVG der TimeAxis mit seinen text-Elementen)
 * jedes sichtbare Paar, das sich um mehr als 1 px überdeckt.
 */
const UEBERLAPPUNG = `(() => {
	const gruppen = [];
	for (const a of document.querySelectorAll('[data-zeit="achse"]')) gruppen.push([...a.querySelectorAll('[data-beschriftung]')]);
	for (const s of document.querySelectorAll('.axis-wrap svg')) gruppen.push([...s.querySelectorAll('text')]);
	for (const g of document.querySelectorAll('[data-zeit="gantt"]')) gruppen.push([...g.querySelectorAll('[data-balken-name]')]);
	let paare = 0;
	const beispiele = [];
	let geprueft = 0;
	for (const elemente of gruppen) {
		const r = elemente
			.map((e) => ({ t: e.textContent.trim(), b: e.getBoundingClientRect() }))
			.filter((x) => x.b.width > 0 && x.b.height > 0 && x.t !== '');
		geprueft += r.length;
		for (let i = 0; i < r.length; i++)
			for (let j = i + 1; j < r.length; j++) {
				const a = r[i].b, b = r[j].b;
				const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
				const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
				if (w > 1 && h > 1) {
					paare++;
					if (beispiele.length < 4) beispiele.push(r[i].t + ' / ' + r[j].t);
				}
			}
	}
	return { achsen: gruppen.length, geprueft, paare, beispiele };
})()`;

/**
 * Seitliches Scrollen. Mit Handy-Emulation macht Edge das Layout-Fenster so
 * breit wie den Inhalt und verkleinert die Seite; darum zählt die Breite des
 * Geräts (`geraet`), nicht nur innerWidth. Dazu, was überläuft (für die
 * Einzelheiten): Elemente ohne eigenen Bildlauf, deren Inhalt breiter ist als
 * sie selbst (auch Text, der nicht umbricht), die innersten zuerst.
 */
const SEITLICH = `(() => {
	const geraet = screen.width;
	const zuBreit = [...document.querySelectorAll('main *')]
		.filter((e) => e.clientWidth > 0 && e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflowX === 'visible')
		.reverse()
		.slice(0, 4)
		.map((e) => e.tagName.toLowerCase() + (typeof e.className === 'string' && e.className ? '.' + e.className.trim().split(/\\s+/)[0] : '') + ' (' + e.scrollWidth + ' statt ' + e.clientWidth + ' px: ' + (e.textContent || '').trim().slice(0, 40) + ')');
	return { breit: document.documentElement.scrollWidth, fenster: window.innerWidth, geraet, zuBreit };
})()`;

async function ganzeSeiteFotografieren(cdp, datei) {
	if (BILDER === null) return;
	mkdirSync(BILDER, { recursive: true });
	const m = await cdp.senden('Page.getLayoutMetrics');
	const groesse = m.cssContentSize ?? m.contentSize;
	const breite = Math.ceil(m.cssLayoutViewport?.clientWidth ?? groesse.width);
	const hoehe = Math.min(Math.ceil(groesse.height), 14000);
	const { data } = await cdp.senden('Page.captureScreenshot', {
		format: 'png',
		captureBeyondViewport: true,
		clip: { x: 0, y: 0, width: breite, height: hoehe, scale: 1 }
	});
	const pfad = join(BILDER, datei);
	writeFileSync(pfad, Buffer.from(data, 'base64'));
	bilder.push(pfad);
	console.log(`  Bild  ${pfad}`);
}

/** Mitte eines Elements (Selektor, n-tes Vorkommen) im Fenster, nach dem Hineinrollen. */
async function mitte(cdp, selektor, n = 0) {
	return cdp.auswerten(`(() => {
		const e = document.querySelectorAll(${JSON.stringify(selektor)})[${n}];
		if (!e) return null;
		e.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'instant' });
		const b = e.getBoundingClientRect();
		return [b.left + b.width / 2, b.top + b.height / 2];
	})()`);
}

const attr = (cdp, selektor, name) =>
	cdp.auswerten(
		`document.querySelector(${JSON.stringify(selektor)})?.getAttribute(${JSON.stringify(name)}) ?? null`
	);

async function fokus(cdp, selektor) {
	return cdp.auswerten(`(() => {
		const e = document.querySelector(${JSON.stringify(selektor)});
		if (!e) return false;
		e.scrollIntoView({ block: 'center', behavior: 'instant' });
		e.focus();
		return document.activeElement === e;
	})()`);
}

/** Dichtestreifen und Verlauf auf /pc: Tastatur und Touch. */
async function pcBedienen(cdp) {
	const D = '[data-zeit="dichte"]';
	if (
		!pruefe(
			(await cdp.auswerten(`document.querySelectorAll('${D}').length`)) === 1,
			'/pc: Dichtestreifen da'
		)
	)
		return;
	pruefe(await fokus(cdp, D), 'Dichtestreifen: Fokus mit der Tastatur erreichbar');
	const von0 = await attr(cdp, D, 'data-von');
	const bis0 = await attr(cdp, D, 'data-bis');
	await druecke(cdp, 'plus', 2);
	const von1 = await attr(cdp, D, 'data-von');
	pruefe(
		von1 !== null && von1 > von0,
		'Dichtestreifen: + vergrößert (kürzerer Zeitraum)',
		`${von0}–${bis0} → ${von1}`
	);
	const cursor1 = await attr(cdp, D, 'data-cursor');
	await druecke(cdp, 'links', 20);
	const cursor2 = await attr(cdp, D, 'data-cursor');
	const von2 = await attr(cdp, D, 'data-von');
	pruefe(
		cursor2 !== null && cursor2 < cursor1,
		'Dichtestreifen: Pfeil links geht Tage zurück',
		`${cursor1} → ${cursor2}`
	);
	pruefe(
		von2 !== null && von2 < von1,
		'Dichtestreifen: der Ausschnitt verschiebt sich mit',
		`${von1} → ${von2}`
	);
	await druecke(cdp, 'pos1');
	const heute = await cdp.auswerten(
		`(() => { const d = new Date(); return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0'); })()`
	);
	pruefe((await attr(cdp, D, 'data-cursor')) === heute, 'Dichtestreifen: Pos1 springt auf heute');
	await druecke(cdp, 'minus', 2);
	pruefe(
		(await attr(cdp, D, 'data-von')) === von0,
		'Dichtestreifen: - verkleinert wieder auf alle 91 Tage'
	);
	// Alt+Pfeil bleibt dem Browser (zurück/vor): der Streifen reagiert nicht.
	const vorAlt = await attr(cdp, D, 'data-cursor');
	await cdp.senden('Input.dispatchKeyEvent', {
		type: 'keyDown',
		key: 'ArrowLeft',
		code: 'ArrowLeft',
		windowsVirtualKeyCode: 37,
		modifiers: 1
	});
	await cdp.senden('Input.dispatchKeyEvent', {
		type: 'keyUp',
		key: 'ArrowLeft',
		code: 'ArrowLeft',
		windowsVirtualKeyCode: 37,
		modifiers: 1
	});
	await pause(150);
	pruefe(
		(await attr(cdp, D, 'data-cursor')) === vorAlt,
		'Dichtestreifen: Alt+Pfeil bleibt beim Browser'
	);

	// Verlauf: Pfeile von Eintrag zu Eintrag, Enter klappt auf.
	const V = '[data-zeit="verlauf"]';
	if (
		pruefe(
			(await cdp.auswerten(`document.querySelectorAll('${V} [data-verlauf-ziel]').length`)) > 1,
			'Verlauf: Einträge da'
		)
	) {
		await fokus(cdp, `${V} [data-verlauf-ziel]`);
		const erstes = await cdp.auswerten(
			'document.activeElement?.textContent?.trim().slice(0, 60) ?? null'
		);
		await druecke(cdp, 'runter');
		const zweites = await cdp.auswerten(
			'document.activeElement?.textContent?.trim().slice(0, 60) ?? null'
		);
		pruefe(
			erstes !== zweites &&
				(await cdp.auswerten(`document.activeElement?.hasAttribute('data-verlauf-ziel') ?? false`)),
			'Verlauf: Pfeil runter geht zum nächsten Eintrag',
			`${erstes} → ${zweites}`
		);
		// Eine zugeklappte Gruppe suchen und mit Enter öffnen.
		const zu = await cdp.auswerten(`(() => {
			const z = [...document.querySelectorAll('${V} [data-verlauf-ziel][aria-expanded="false"]')];
			if (z.length === 0) return false;
			z[0].focus();
			return true;
		})()`);
		if (zu) {
			await druecke(cdp, 'enter');
			pruefe(
				(await cdp.auswerten(`document.activeElement?.getAttribute('aria-expanded')`)) === 'true',
				'Verlauf: Enter klappt eine Gruppe auf'
			);
		}
		await druecke(cdp, 'pos1');
		pruefe(
			await cdp.auswerten(`document.activeElement?.closest('[data-heute]') !== null`),
			'Verlauf: Pos1 springt auf heute'
		);
	}

	// Touch-Ersatz: antippen und ziehen.
	await cdp.senden('Emulation.setTouchEmulationEnabled', {
		enabled: true,
		maxTouchPoints: 5
	});
	await cdp.auswerten(
		`document.querySelector('${D}').scrollIntoView({ block: 'center', behavior: 'instant' })`
	);
	await pause(200);
	const balken = await cdp.auswerten(`(() => {
		const s = document.querySelector('${D} [data-streifen]');
		const b = s.getBoundingClientRect();
		return { links: b.left, rechts: b.right, mitte: b.top + b.height / 2, y: window.scrollY, h: window.innerHeight };
	})()`);
	// Was beim Finger ankommt, für die Einzelheiten, falls es nicht klappt.
	await cdp.auswerten(`(() => {
		window.__zeiger = [];
		for (const t of ['pointerdown', 'pointerup', 'pointercancel', 'touchstart', 'touchend']) {
			document.addEventListener(t, (e) => window.__zeiger.push(t + ':' + (e.target?.className ?? '')), true);
		}
	})()`);
	const x1 = balken.links + (balken.rechts - balken.links) * 0.5;
	await cdp.beruehren('touchStart', [[x1, balken.mitte]]);
	await cdp.beruehren('touchEnd', []);
	await pause(400);
	const getippt = await attr(cdp, D, 'data-cursor');
	pruefe(
		getippt !== null && getippt !== heute,
		'Touch: Antippen wählt einen Tag',
		`${String(getippt)}; bei ${Math.round(x1)},${Math.round(balken.mitte)}: ${await cdp.auswerten(`window.__zeiger.join(' | ') + ' scrollY vorher ${balken.y}, jetzt ' + window.scrollY + ', Höhe ${balken.h}, Streifen jetzt ' + Math.round(document.querySelector('[data-streifen]').getBoundingClientRect().top)`)}`
	);
	pruefe(
		await cdp.auswerten(`(() => {
			const z = document.querySelector('[data-zeit="verlauf"] [data-tag="${getippt}"]');
			return z !== null && z.getBoundingClientRect().top < window.innerHeight && z.getBoundingClientRect().bottom > 0;
		})()`),
		'Touch: Antippen springt im Verlauf zum Tag'
	);
	await cdp.auswerten(
		`document.querySelector('${D}').scrollIntoView({ block: 'center', behavior: 'instant' })`
	);
	await pause(200);
	const b2 = await cdp.auswerten(`(() => {
		const b = document.querySelector('${D} [data-streifen]').getBoundingClientRect();
		return { links: b.left, rechts: b.right, mitte: b.top + b.height / 2 };
	})()`);
	const xa = b2.links + (b2.rechts - b2.links) * 0.6;
	const xb = b2.links + (b2.rechts - b2.links) * 0.85;
	await cdp.beruehren('touchStart', [[xa, b2.mitte]]);
	for (let i = 1; i <= 6; i++) {
		await cdp.beruehren('touchMove', [[xa + ((xb - xa) * i) / 6, b2.mitte]]);
		await pause(30);
	}
	await cdp.beruehren('touchEnd', []);
	await pause(400);
	const av = await attr(cdp, D, 'data-auswahl-von');
	const ab = await attr(cdp, D, 'data-auswahl-bis');
	pruefe(
		av !== null && ab !== null && av < ab,
		'Touch: Ziehen wählt einen Zeitraum',
		`${av}–${ab}`
	);
	pruefe(
		await cdp.auswerten(`document.querySelector('[data-zeit="verlauf"] [data-zeitraum]') !== null`),
		'Touch: der Verlauf zeigt nur den gewählten Zeitraum (mit Knopf zum Aufheben)'
	);
	await cdp.senden('Emulation.setTouchEmulationEnabled', { enabled: false });
}

/** Gantt: Tastatur und Touch auf dem ersten Gantt der Seite. */
async function ganttBedienen(cdp, seite) {
	const G = '[data-zeit="gantt"]';
	const B = `${G} [data-zeit-buehne]`;
	if (
		!pruefe(
			(await cdp.auswerten(`document.querySelectorAll('${B}').length`)) > 0,
			`${seite}: Gantt mit Bühne da`
		)
	)
		return;
	pruefe(await fokus(cdp, B), 'Gantt: Fokus mit der Tastatur erreichbar');
	const s0 = await attr(cdp, G, 'data-stufe');
	await druecke(cdp, 'plus');
	const s1 = await attr(cdp, G, 'data-stufe');
	pruefe(s1 !== s0, 'Gantt: + zoomt (andere Stufe)', `${s0} → ${s1}`);
	// Seit 3b zoomt + um heute; liegt heute am Ende des Plans, steht der
	// Ausschnitt schon am rechten Rand. Dann zuerst nach links, dann zurück.
	const v1 = Number(await attr(cdp, G, 'data-von'));
	await druecke(cdp, 'rechts', 3);
	let v2 = Number(await attr(cdp, G, 'data-von'));
	let vor = 'rechts';
	if (v2 === v1) {
		vor = 'links';
		await druecke(cdp, 'links', 3);
		v2 = Number(await attr(cdp, G, 'data-von'));
	}
	await druecke(cdp, vor === 'rechts' ? 'links' : 'rechts', 1);
	const v3 = Number(await attr(cdp, G, 'data-von'));
	pruefe(
		vor === 'rechts' ? v2 > v1 && v3 < v2 : v2 < v1 && v3 > v2,
		'Gantt: Pfeile verschieben',
		`${v1} → ${v2} → ${v3} (zuerst ${vor})`
	);
	await druecke(cdp, 'pos1');
	const heuteDa = await cdp.auswerten(`document.querySelector('${G} [data-heute-linie]') !== null`);
	pruefe(
		heuteDa || (await attr(cdp, G, 'data-heute-ausserhalb')) === 'ja',
		'Gantt: Pos1 zeigt heute (oder sagt, dass heute außerhalb liegt)'
	);
	await druecke(cdp, 'minus');
	pruefe((await attr(cdp, G, 'data-stufe')) !== null, 'Gantt: - zoomt zurück');
	// Touch: mit dem Finger ziehen verschiebt.
	await druecke(cdp, 'plus', 2);
	await cdp.senden('Emulation.setTouchEmulationEnabled', {
		enabled: true,
		maxTouchPoints: 5
	});
	const m = await mitte(cdp, B);
	const vorher = Number(await attr(cdp, G, 'data-von'));
	// Erst nach links ziehen (spätere Zeit); steht der Ausschnitt schon am
	// Ende (Zoom um heute, 3b), dann nach rechts.
	const ziehen = async (richtung) => {
		await cdp.beruehren('touchStart', [[m[0], m[1]]]);
		for (let i = 1; i <= 6; i++) {
			await cdp.beruehren('touchMove', [[m[0] + richtung * i * 20, m[1]]]);
			await pause(30);
		}
		await cdp.beruehren('touchEnd', []);
		await pause(300);
		return Number(await attr(cdp, G, 'data-von'));
	};
	let nachher = await ziehen(-1);
	if (nachher === vorher) nachher = await ziehen(1);
	pruefe(nachher !== vorher, 'Gantt: Touch-Ziehen verschiebt', `${vorher} → ${nachher}`);
	await cdp.senden('Emulation.setTouchEmulationEnabled', { enabled: false });
}

/** Die Knöpfe des Filters nach Quelle im Verlauf (etwa „Starts 7“), und ob der Hinweis ohne Anmeldung steht. */
const QUELLEN = `(() => {
	const f = document.querySelector('[data-zeit="verlauf"] [aria-label="Nach Quelle filtern"]');
	return {
		knoepfe: f ? [...f.querySelectorAll('button')].map((b) => b.textContent.trim().replace(/ +/g, ' ')) : [],
		hinweis: document.body.textContent.includes('zeigt der Verlauf nur mit Anmeldung')
	};
})()`;

/** Wie viele Starts `n` gleichzeitige Abrufe von `pfad` aus der Seite je liefern (negativ: Status). */
const gleichzeitig = (pfad, n) => `Promise.all(Array.from({ length: ${n} }, () =>
	fetch('${pfad}', { headers: { accept: 'application/json' } }).then(async (r) =>
		r.ok ? (await r.json()).ereignisse.filter((e) => e.quelle === 'start').length : -r.status)))`;

/**
 * Hotfix 3a-1: Mit Anmeldung ist der Verlauf jedes Mal vollständig, nicht nur
 * zufällig. Edge schickt Negotiate nur auf einer Verbindung, die vorher 401
 * bekam; darum liest die Oberfläche den geschützten Weg /api/pc/verlauf/voll.
 */
async function verlaufMitAnmeldung(cdp) {
	const anfragen = [];
	const h = (m) => {
		if (m.method !== 'Fetch.requestPaused') return;
		try {
			const u = new URL(m.params.request.url);
			if (u.pathname.startsWith('/api/pc/verlauf')) anfragen.push(u.pathname);
		} catch {
			// keine Adresse
		}
	};
	cdp.hoerer.push(h);
	try {
		// Ohne Nachweis (Node schickt keinen): die Herausforderung, auf die Edge anmeldet.
		const r = await fetch(`http://127.0.0.1:${PORT}/api/pc/verlauf/voll`, {
			headers: { accept: 'application/json' }
		});
		pruefe(
			r.status === 401 && r.headers.get('www-authenticate') === 'Negotiate',
			'/api/pc/verlauf/voll ohne Anmeldung: 401 mit WWW-Authenticate: Negotiate',
			`${r.status} ${r.headers.get('www-authenticate')}`
		);
		const frei = await json('/api/pc/verlauf');
		pruefe(
			!frei.ereignisse.some((e) => e.quelle === 'start'),
			'/api/pc/verlauf ohne Anmeldung: frei, ohne Starts'
		);
		for (let i = 1; i <= 5; i++) {
			anfragen.length = 0;
			await cdp.oeffnen('/pc');
			const q = await cdp.auswerten(QUELLEN);
			pruefe(
				q.knoepfe.some((k) => k.startsWith('Starts ')) && !q.hinweis,
				`/pc geladen (${i}. Mal): Verlauf mit Starts, ohne Hinweis`,
				`Filter: ${q.knoepfe.join(', ') || 'keiner'}; Hinweis: ${q.hinweis}`
			);
			pruefe(
				anfragen.includes('/api/pc/verlauf/voll') && !anfragen.includes('/api/pc/verlauf'),
				`/pc geladen (${i}. Mal): fragt /api/pc/verlauf/voll`,
				anfragen.join(', ')
			);
		}
		// Live: data/pc.toml ändert sich, die Seite lädt api:pc neu (ohne Neuladen der Seite).
		const vorher = anfragen.length;
		appendFileSync(join(ordner, 'repo', 'data', 'pc.toml'), '\n# Probe: live neu laden\n');
		const ende = Date.now() + 15000;
		while (anfragen.length === vorher && Date.now() < ende) await pause(250);
		await pause(1500);
		const q = await cdp.auswerten(QUELLEN);
		pruefe(anfragen.length > vorher, '/pc live neu geladen (data/pc.toml geändert)', anfragen.join(', '));
		pruefe(
			q.knoepfe.some((k) => k.startsWith('Starts ')) && !q.hinweis,
			'/pc nach dem Neuladen live: Verlauf mit Starts, ohne Hinweis',
			`Filter: ${q.knoepfe.join(', ') || 'keiner'}; Hinweis: ${q.hinweis}`
		);
		pruefe(
			anfragen.slice(vorher).every((a) => a === '/api/pc/verlauf/voll'),
			'/pc live: fragt /api/pc/verlauf/voll',
			anfragen.slice(vorher).join(', ')
		);
		// Startseite: derselbe Weg (letzte 7 Tage; Starts darin sind nicht sicher).
		anfragen.length = 0;
		await cdp.oeffnen('/');
		pruefe(
			anfragen.includes('/api/pc/verlauf/voll') && !anfragen.includes('/api/pc/verlauf'),
			'Startseite: fragt /api/pc/verlauf/voll',
			anfragen.join(', ')
		);
		// Wie im Befund des Architekten: 8 gleichzeitige Abrufe aus der Seite.
		const voll = await cdp.auswerten(gleichzeitig('/api/pc/verlauf/voll', 8));
		pruefe(
			voll.every((n) => n > 0),
			'8 gleichzeitige Abrufe von /api/pc/verlauf/voll: alle mit Starts',
			voll.join(', ')
		);
		const freiGleichzeitig = await cdp.auswerten(gleichzeitig('/api/pc/verlauf', 8));
		console.log(
			`  info  8 gleichzeitige Abrufe des freien /api/pc/verlauf, Starts je Antwort: ${freiGleichzeitig.join(', ')}`
		);
	} finally {
		cdp.hoerer.splice(cdp.hoerer.indexOf(h), 1);
	}
}

/** Ein Prüfblock, der bei einem Fehler im Skript (etwa fehlendes Element) nur als nicht bestanden zählt. */
async function sicher(satz, f) {
	try {
		await f();
	} catch (e) {
		pruefe(false, satz, String(e?.message ?? e).slice(0, 300));
	}
}

// ----- Ablauf -----

async function seitenListe() {
	const liste = await json('/api/projects');
	const seiten = [];
	for (const p of liste.projects) {
		const k = await json(`/api/projekte/${encodeURIComponent(p.slug)}`).catch(() => null);
		const daten = k?.daten ?? [];
		if (daten.includes('zeitstrahl') || daten.includes('gantt')) {
			seiten.push({
				pfad: `/projects/${p.slug}`,
				timeline: daten.includes('zeitstrahl') ? 1 : 0,
				gantt: daten.includes('gantt') ? 1 : 0,
				slug: p.slug,
				neu: true
			});
		}
	}
	return seiten;
}

async function main() {
	// Etappe 4b: Testserver und beide Edge-Debug-Ports, wie alle Proben.
	await portePruefen(PORT);
	if (!existsSync(EXE)) throw new Error(`Programm fehlt: ${EXE}`);
	if (!existsSync(join(WEB, 'index.html'))) throw new Error(`Oberfläche fehlt: ${WEB}`);
	mkdirSync(join(ordner, 'bin'), { recursive: true });
	mkdirSync(join(ordner, 'profil'), { recursive: true });
	copyFileSync(EXE, join(ordner, 'bin', 'dashboard.exe'));
	// KOPIEN von projects\ und data\: der Server darf darin nichts am Repo ändern.
	cpSync(INHALT.projects, join(ordner, 'repo', 'projects'), {
		recursive: true
	});
	cpSync(INHALT.data, join(ordner, 'repo', 'data'), { recursive: true });
	console.log(`Temp-Ordner: ${ordner}`);

	const server = await serverStarten(eigeneSid());
	console.log('Server läuft; warte auf den Sammler (Windows-Ereignisse) …');
	const updates = await aufSammlerWarten();
	console.log(`  ${updates} Update-Ereignisse im Verlauf`);
	const cdp = await edgeStarten();
	await cdp.breite(1280);

	const seiten = await seitenListe();
	const mitTimeline = seiten.filter((s) => s.timeline > 0).length;
	const mitGantt = seiten.filter((s) => s.gantt > 0).length;
	console.log(
		`\n${seiten.length} Projektseiten mit timeline oder gantt (${mitTimeline} mit timeline, ${mitGantt} mit gantt), dazu / und /pc`
	);
	const alle = [
		{ pfad: '/', timeline: 0, gantt: 0 },
		{ pfad: '/pc', timeline: 0, gantt: 0 },
		...seiten
	].filter((s) => NUR === null || NUR.includes(s.pfad));

	const bericht = [];
	for (const breite of [1280, 375]) {
		console.log(`\n== ${breite} px ==`);
		await cdp.breite(breite);
		for (const s of alle) {
			const k = konsoleMitschreiben(cdp);
			await cdp.oeffnen(s.pfad);
			const b = await cdp.auswerten(BAUSTEINE);
			const u = await cdp.auswerten(UEBERLAPPUNG);
			const w = await cdp.auswerten(SEITLICH);
			k.stopp();
			bericht.push({
				breite,
				pfad: s.pfad,
				...b,
				ueberlappung: u,
				seitlich: w,
				konsole: k.fehler
			});
			console.log(`${s.pfad}`);
			pruefe(
				k.fehler.length === 0,
				`${s.pfad} (${breite} px): keine Fehler in der Konsole`,
				k.fehler.join(' | ')
			);
			if (s.timeline > 0) {
				pruefe(b.verlauf + b.altTimeline > 0, `${s.pfad}: Zeitstrahl gezeigt`, JSON.stringify(b));
				pruefe(b.verlauf > 0, `${s.pfad}: Zeitstrahl mit dem neuen Baustein Verlauf`);
			}
			if (s.gantt > 0) {
				pruefe(b.gantt + b.altGantt > 0, `${s.pfad}: Gantt gezeigt`, JSON.stringify(b));
				pruefe(b.gantt > 0, `${s.pfad}: Gantt auf der neuen Achse`);
			}
			if (s.pfad === '/pc') {
				pruefe(
					b.verlauf === 1 && b.dichte === 1,
					'/pc: Dichtestreifen und Verlauf da',
					JSON.stringify(b)
				);
			}
			if (s.pfad === '/')
				pruefe(b.verlauf === 1, 'Startseite: „Was mit dem PC passiert ist“ als Verlauf');
			pruefe(
				u.paare === 0,
				`${s.pfad} (${breite} px): keine überlappenden Beschriftungen (${u.geprueft} geprüft)`,
				u.beispiele.join(' | ')
			);
			if (breite === 375) {
				pruefe(
					w.breit <= w.geraet && w.fenster <= w.geraet,
					`${s.pfad}: bei 375 px kein seitliches Scrollen`,
					`Seite ${w.breit} px, Fenster ${w.fenster} px, Gerät ${w.geraet} px; zu breit: ${w.zuBreit.join(', ')}`
				);
			}
		}
	}

	console.log('\n== Verlauf mit Anmeldung (Hotfix 3a-1) ==');
	await cdp.breite(1280);
	await sicher('Verlauf mit Anmeldung', () => verlaufMitAnmeldung(cdp));

	// Bilder: Dashboard-Projektseite, /pc, eine Projektseite mit Zeitstrahl.
	const zeitstrahlSeite =
		seiten.find((s) => s.timeline > 0 && s.slug !== 'dashboard');
	const fotoSeiten = [
		['dashboard', '/projects/dashboard'],
		['pc', '/pc'],
		[
			`projekt-${zeitstrahlSeite?.slug ?? 'zeitstrahl'}`,
			zeitstrahlSeite?.pfad ?? '/projects/dashboard'
		]
	];
	if (BILDER !== null) {
		console.log('\n== Bilder ==');
		for (const breite of [1280, 375]) {
			await cdp.breite(breite);
			for (const [name, pfad] of fotoSeiten) {
				await cdp.oeffnen(pfad);
				await ganzeSeiteFotografieren(cdp, `${name}-${breite}.png`);
			}
		}
	}

	console.log('\n== Bedienung (1280 px) ==');
	await cdp.breite(1280);
	await cdp.oeffnen('/pc');
	await sicher('/pc bedienen (1280 px)', () => pcBedienen(cdp));
	const ganttSeite =
		seiten.find((s) => s.slug === 'dashboard' && s.gantt > 0) ?? seiten.find((s) => s.gantt > 0);
	if (ganttSeite) {
		await cdp.oeffnen(ganttSeite.pfad);
		await sicher(`Gantt bedienen (${ganttSeite.pfad})`, () => ganttBedienen(cdp, ganttSeite.pfad));
	}
	console.log('\n== Bedienung (375 px) ==');
	await cdp.breite(375);
	await cdp.oeffnen('/pc');
	await sicher('/pc bedienen (375 px)', () => pcBedienen(cdp));

	writeFileSync(
		join(BILDER ?? ordner, 'bericht.json'),
		JSON.stringify({ seiten: alle, bericht }, null, 2)
	);
	void server;
	pruefe(
		cdp.gesperrt.length === 0,
		'Netzsperre: die Seiten fragen nur beim eigenen Server an',
		cdp.gesperrt.slice(0, 5).join(' | ')
	);
	console.log(
		`\nSeiten geprüft: ${alle.length} je Breite (${seiten.length} Projektseiten, / und /pc)`
	);
	console.log(`Bestanden: ${bestanden} Prüfungen. Nicht bestanden: ${fehlgeschlagen.length}.`);
	for (const f of fehlgeschlagen) console.log(`  - ${f}`);
	if (bilder.length > 0) console.log(`Bilder:\n${bilder.map((b) => `  ${b}`).join('\n')}`);
}

let code = 0;
try {
	await main();
	code = fehlgeschlagen.length === 0 ? 0 : 1;
} catch (e) {
	console.error(String(e?.stack ?? e));
	if (BEHALTEN) console.error(`Temp-Ordner bleibt: ${ordner}`);
	code = 1;
} finally {
	await aufraeumen();
}
process.exit(code);
