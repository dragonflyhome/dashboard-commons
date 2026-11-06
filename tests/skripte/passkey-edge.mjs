#!/usr/bin/env node
/**
 * Ende-zu-Ende-Probe für Etappe 7c (Passkey für andere Konten) mit Edge.
 *
 * Was es tut (alles mit Wegwerf-Daten in einem Temp-Ordner, Port 8788, nie
 * 8787, nie C:\ProgramData\Dashboard, keine echten Passkeys, kein Windows Hello):
 *
 * 1. Kopiert target\debug\dashboard.exe in den Temp-Ordner und startet sie mit
 *    `--konsole --einstellungen` (eigene dienst.toml: Datenspeicher, Sicherung,
 *    Protokoll und der JWT-Schlüssel im Temp-Ordner, eine Pipe für den Boten,
 *    die es nicht gibt, die Projekte aus tests\beispiel).
 * 2. Startet Edge headless mit Wegwerf-Profil und `--remote-debugging-port`
 *    und spricht über das DevTools-Protokoll (WebSocket von Node 24) mit ihm.
 *    Ohne Hintergrund-Netz und Komponenten-Updates, alle Kindprozesse ohne
 *    Fenster; Fetch.enable sperrt jede Anfrage, die nicht an 127.0.0.1 oder
 *    localhost geht (dann besteht die Probe nicht).
 *    Passkeys kommen von virtuellen Authenticatoren (`WebAuthn.addVirtualAuthenticator`,
 *    ctap2, intern, mit Nutzerprüfung, UV immer ja), nie von einem echten Gerät.
 *
 * Teil A, Hauptkonto (inhaber_sid = die SID des eigenen Kontos, Edge meldet per
 * Negotiate von selbst an):
 *   - /vorgaenge öffnet sich ohne Passkey (wie 7b)
 *   - auf /anmelden einen Passkey mit RS256 anlegen (die Seite bekommt
 *     dafür nur RS256 angeboten) und einen mit ES256: beide sofort registriert
 *   - mit Passkey anmelden: Cookie __Host-dashboard mit HttpOnly, Secure,
 *     SameSite=Strict, nicht in document.cookie
 * Teil B, „anderes Konto“ (derselbe Server mit erfundener inhaber_sid, also
 * gilt das eigene Konto jetzt als fremd; dieselben Daten und derselbe Schlüssel):
 *   - /vorgaenge leitet auf /anmelden?ziel=/vorgaenge um, die API sagt 401
 *   - Antrag mit einem neuen Authenticator: ausstehend, Vergleichszahl, keine
 *     Rechte, Anmeldung damit geht nicht
 *   - mit dem ES256-Passkey aus Teil A anmelden (frisch) und auf /schluessel
 *     den Antrag freigeben, nachdem dort dieselbe Zahl steht
 *   - mit dem neuen Passkey anmelden, /vorgaenge öffnen (geschützte Seite)
 *   - Widerruf (mit dem ES256-Passkey) beendet die laufende Sitzung des
 *     neuen Passkeys sofort: nächste Anfrage 401
 *   - Abmelden sperrt den Ausweis
 *   - Kopfleiste, Fassung 3 (Auflage aus der Abnahme): der Stand für die
 *     Aktualitätsprüfung /api/stand/voll mit der laufenden Sitzung 200, nach
 *     Widerruf und nach Abmelden 401; /api/stand bleibt frei (200)
 *
 * Kopfleiste, Fassung 2 (`[fassung_2_passkey_wege]` in
 * kopfleiste), Kürzel „schloss“:
 *   (a) nicht angemeldet (Teil B vor der Anmeldung): Schloss zu, Klick führt
 *       auf /anmelden#mit-passkey-anmelden, der Antrag steht dort
 *   (b) angemeldet (Teil A, Hauptkonto): Schloss offen, Klick führt auf
 *       /anmelden#passkey-anlegen, Antrag da, die Liste der Passkeys darunter
 *   (c) auf /anmelden (hinter dem Schloss) einen fremden ausstehenden Passkey
 *       mit der Vergleichszahl bestätigen und einen registrierten widerrufen;
 *       der Widerruf gilt sofort (nächste Anfrage mit ihm 401)
 *   (d) der Passkey der eigenen Sitzung: „Mit diesem Passkey bist du gerade
 *       angemeldet“, ohne Knöpfe
 *   (e) ohne Verbindung (der freie Kanal /api/events/frei wird über
 *       Fetch.failRequest abgewiesen): nur der Hinweis, keine Passkey-Knöpfe
 *   Nachbesserung aus der Abnahme: je Seite genau eine offene
 *   EventSource (gezählt über eine Hülle um window.EventSource); angemeldet
 *   der geschützte Kanal, sonst der freie. Wird die Sitzung dieses Tabs in
 *   einem zweiten Tab (eigener Browser-Kontext, eigene Cookies) widerrufen
 *   oder hier abgemeldet, geht das Schloss zu, der Punkt zeigt „live, nicht
 *   angemeldet“, und offen ist nur noch der freie Kanal.
 *
 * Was es NICHT zeigen kann: ein echtes zweites Windows-Konto (es gibt keins,
 * und keins wird angelegt). Negotiate meldet immer das eigene Konto an; Teil B
 * stellt nur die erwartete SID um, damit das eigene Konto als fremd gilt. Das
 * Hauptkonto ist auch in Teil B nicht erreichbar; Freigabe geht dort mit dem
 * Passkey aus Teil A. Windows Hello selbst (Fingerabdruck, PIN) und ein Handy
 * über Bluetooth sind nicht dabei; das prüft nur ein echter Versuch.
 *
 * Aufruf aus dem Repo (vorher `cargo build` und ein Bau der Oberfläche):
 *
 *     node tests\skripte\passkey-edge.mjs --web <Ordner mit index.html>
 *
 * Ohne --web nimmt es web\build. Optional: --exe <dashboard.exe>,
 * --edge <msedge.exe>, --port 8788, --behalten (Temp-Ordner nicht löschen).
 * Exit 0: alles bestanden. Am Ende laufen weder Server noch Edge weiter.
 *
 * Netzsperre auf Browser-Ebene (Plan 3a+, edge-gemeinsam.mjs): Zusätzlich
 * zur Seite hängt sich eine zweite Verbindung mit Target.setAutoAttach
 * (flatten) an jedes andere Ziel, auch Service-Worker, Worker, weitere
 * Seiten und Frames, und sperrt dort ebenso mit Fetch.enable. Anfragen des
 * Browser-Prozesses selbst schalten die Startschalter ab. Strg+C räumt
 * Server, Edge und Temp-Ordner auf; ein belegter Port bricht vor dem Start ab.
 */
import { spawn, execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { antwortAbwarten } from './cdp-frist.mjs';
import { beiAbbruch, browserSperren, mitFrist, portePruefen } from './edge-gemeinsam.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

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
const BEHALTEN = process.argv.includes('--behalten');
const DEBUG_PORT = PORT + 1000;
const BASIS = `http://localhost:${PORT}`;
const FREMDE_SID = 'S-1-5-21-1111111111-2222222222-3333333333-1001';
/** Programme aus System32 (nicht die gleichnamigen aus Git Bash). */
const SYSTEM = (name) => join(process.env.SystemRoot ?? 'C:\Windows', 'System32', name);

const ordner = mkdtempSync(join(tmpdir(), 'dashboard-passkey-e2e-'));
const prozesse = [];
/** Ob diese Probe ein Edge gestartet hat (nur das schließt das Aufräumen). */
let edgeGestartet = false;
// Ein Aufräumen für alle Wege (finally, Strg+C, unbehandelter Fehler).
const aufraeumen = beiAbbruch(aufraeumenJetzt);
let bestanden = 0;

function pruefe(bedingung, satz) {
	if (!bedingung) throw new Error(`NICHT BESTANDEN: ${satz}`);
	bestanden++;
	console.log(`  ok  ${satz}`);
}

const pause = (ms) => new Promise((r) => setTimeout(r, ms));

async function aufraeumenJetzt() {
	// Edge erst höflich schließen (der Starter beendet sich früh, seine Kinder nicht).
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
			execFileSync(SYSTEM('taskkill.exe'), ['/PID', String(p.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
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
	const zeile = execFileSync(SYSTEM('whoami.exe'), ['/user', '/fo', 'csv', '/nh'], { encoding: 'utf8', windowsHide: true });
	const sid = zeile.trim().split(',').pop().replace(/"/g, '');
	if (!/^S-1-\d+(-\d+)+$/.test(sid)) throw new Error(`SID nicht lesbar: ${zeile}`);
	return sid;
}

function einstellungen(inhaberSid) {
	const p = (rest) => join(ordner, rest).replace(/\\/g, '/');
	const text = [
		'schema = 1',
		`repo = "${join(REPO, 'tests', 'beispiel').replace(/\\/g, '/')}"`,
		`port = ${PORT}`,
		'',
		'[pfade]',
		`daten = "${p('daten/dashboard.redb')}"`,
		`sicherung = "${p('sicherung')}"`,
		`protokoll = "${p('protokoll')}"`,
		`web = "${WEB.replace(/\\/g, '/')}"`,
		'',
		'[konten]',
		`inhaber_sid = "${inhaberSid}"`,
		'',
		'[bote]',
		`pipe = "dashboard-bote-e2e-${process.pid}"`,
		`programm = "${p('bin/gibt-es-nicht.exe')}"`,
		''
	].join('\n');
	writeFileSync(join(ordner, 'dienst.toml'), text);
}

async function serverStarten(inhaberSid) {
	einstellungen(inhaberSid);
	const exe = join(ordner, 'bin', 'dashboard.exe');
	const kind = spawn(exe, ['--konsole', '--einstellungen', join(ordner, 'dienst.toml')], {
		env: { ...process.env, USERPROFILE: join(ordner, 'profil'), RUST_LOG: 'warn' },
		stdio: ['ignore', 'ignore', 'pipe'],
		windowsHide: true
	});
	let fehler = '';
	kind.stderr.on('data', (d) => (fehler += d.toString()));
	prozesse.push(kind);
	for (let i = 0; i < 100; i++) {
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

async function serverStoppen(kind) {
	try {
		execFileSync(SYSTEM('taskkill.exe'), ['/PID', String(kind.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
	} catch {
		// schon beendet
	}
	prozesse.splice(prozesse.indexOf(kind), 1);
	for (let i = 0; i < 50 && kind.exitCode === null; i++) await pause(100);
	await pause(300);
}

/** GET mit dem Ausweis als Cookie, ohne Windows-Anmeldung (Node schickt kein Negotiate). */
async function mitAusweis(pfad, ausweis) {
	const r = await fetch(`${BASIS}${pfad}`, {
		headers: { cookie: `__Host-dashboard=${ausweis}`, accept: 'application/json' }
	});
	return r.status;
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
				for (const h of this.hoerer) h(m);
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

	warteAuf(method, ms = 15000) {
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
			const eigen = host === '127.0.0.1' || host === 'localhost' || host === '[::1]';
			if (eigen && this.freiAbweisen && new URL(request.url).pathname === '/api/events/frei') {
				// Kopfleiste, Fassung 2 (e): keine Verbindung zum freien Kanal; gewollt, kein Fund.
				this.senden('Fetch.failRequest', { requestId, errorReason: 'ConnectionRefused' }).catch(() => {});
			} else if (eigen) {
				this.senden('Fetch.continueRequest', { requestId }).catch(() => {});
			} else {
				this.gesperrt.push(request.url);
				this.senden('Fetch.failRequest', { requestId, errorReason: 'BlockedByClient' }).catch(() => {});
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
		await pause(500);
	}

	/** Wartet, bis `ausdruck` wahr ist (höchstens `ms`). */
	async bis(ausdruck, ms = 15000, satz = ausdruck) {
		const ende = Date.now() + ms;
		while (Date.now() < ende) {
			if (await this.auswerten(`Boolean(${ausdruck})`).catch(() => false)) return;
			await pause(200);
		}
		throw new Error(`Zeit abgelaufen: ${satz}\nSeite: ${await this.text()}`);
	}

	text() {
		return this.auswerten('document.body ? document.body.innerText.slice(0, 1500) : ""');
	}

	/** Klickt den ersten Knopf, dessen Text `teil` enthält. */
	async klick(teil) {
		const ok = await this.auswerten(`(() => {
			const b = [...document.querySelectorAll('button')].find((b) => b.textContent.includes(${JSON.stringify(teil)}) && !b.disabled);
			if (!b) return false;
			b.click();
			return true;
		})()`);
		if (!ok) throw new Error(`Knopf „${teil}“ fehlt oder ist aus.\nSeite: ${await this.text()}`);
	}

	async eingeben(selektor, wert) {
		await this.auswerten(`(() => {
			const e = document.querySelector(${JSON.stringify(selektor)});
			e.focus();
			e.value = ${JSON.stringify(wert)};
			e.dispatchEvent(new Event('input', { bubbles: true }));
		})()`);
	}

	async ausweis() {
		const { cookies } = await this.senden('Network.getCookies', { urls: [BASIS] });
		return cookies.find((c) => c.name === '__Host-dashboard') ?? null;
	}
}

async function authenticator(cdp) {
	const { authenticatorId } = await cdp.senden('WebAuthn.addVirtualAuthenticator', {
		options: {
			protocol: 'ctap2',
			transport: 'internal',
			hasResidentKey: true,
			hasUserVerification: true,
			isUserVerified: true,
			automaticPresenceSimulation: true
		}
	});
	return authenticatorId;
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
				await quellenZaehlen(cdp);
				await cdp.netzSperren();
				// Auch Service-Worker, Worker und weitere Ziele (edge-gemeinsam.mjs).
				cdp.browserSperre = await browserSperren(DEBUG_PORT, cdp.gesperrt, { ausser: seite.id });
				await cdp.senden('Network.enable');
				await cdp.senden('WebAuthn.enable', { enableUI: false });
				return cdp;
			}
		} catch {
			// Edge startet noch
		}
		await pause(200);
	}
	throw new Error('Edge antwortet nicht auf dem Debug-Port');
}

// ----- Ablauf -----

async function passkeyAnlegen(cdp, name) {
	await cdp.oeffnen('/anmelden');
	await cdp.bis(`document.querySelector('#geraet')`, 15000, 'Feld „Name des Geräts“');
	await cdp.eingeben('#geraet', name);
	await cdp.klick('Passkey anlegen');
}

async function mitPasskeyAnmelden(cdp, ziel = '/') {
	await cdp.oeffnen(`/anmelden?ziel=${encodeURIComponent(ziel)}`);
	await cdp.bis(
		`[...document.querySelectorAll('button')].some((b) => b.textContent.includes('Mit Passkey anmelden'))`,
		15000,
		'Knopf „Mit Passkey anmelden“'
	);
	await cdp.klick('Mit Passkey anmelden');
}

// ----- Kopfleiste, Fassung 2: Schloss und Passkey-Seite -----

/** Hülle um window.EventSource: merkt sich jede Quelle (offen = nicht CLOSED). */
async function quellenZaehlen(cdp) {
	await cdp.senden('Page.addScriptToEvaluateOnNewDocument', {
		source: `(() => {
			const O = window.EventSource;
			if (!O) return;
			const alle = new Set();
			window.__eventQuellen = () => [...alle].filter((q) => q.readyState !== 2).map((q) => new URL(q.url, location.href).pathname);
			window.EventSource = class extends O {
				constructor(u, o) { super(u, o); alle.add(this); }
				close() { alle.delete(this); super.close(); }
			};
		})()`
	});
}

/** Die offenen EventSource der Seite (Pfade). */
function quellen(cdp) {
	return cdp.auswerten(`window.__eventQuellen ? window.__eventQuellen() : null`);
}

/** Wartet auf Schloss, Live-Punkt und genau eine offene Quelle. */
async function kopfUndKanal(cdp, stand, zustand, kanal, satz) {
	await cdp.bis(
		`document.querySelector('header a[data-schloss="${stand}"]') && document.querySelector('header a[data-zustand="${zustand}"]') && JSON.stringify(window.__eventQuellen?.()) === '${JSON.stringify([kanal])}'`,
		20000,
		satz
	);
	pruefe(true, `${satz}: Schloss ${stand}, „${zustand}“, genau eine EventSource (${kanal})`);
}

/** Ein zweiter Tab in einem eigenen Browser-Kontext (eigene Cookies). */
async function zweiterTab() {
	const version = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/version`)).json();
	const browser = await Cdp.verbinden(version.webSocketDebuggerUrl);
	const { browserContextId } = await browser.senden('Target.createBrowserContext');
	const { targetId } = await browser.senden('Target.createTarget', { url: 'about:blank', browserContextId });
	let tab = null;
	for (let i = 0; i < 50 && tab === null; i++) {
		const liste = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/list`)).json();
		const z = liste.find((t) => t.id === targetId);
		if (z) tab = await Cdp.verbinden(z.webSocketDebuggerUrl);
		else await pause(100);
	}
	if (tab === null) throw new Error('zweiter Tab antwortet nicht');
	// Die Netzsperre hängt sich über browserSperren von selbst an (neues Ziel).
	await tab.senden('Page.enable');
	await tab.senden('Network.enable');
	return {
		tab,
		schliessen: async () => {
			await browser.senden('Target.disposeBrowserContext', { browserContextId }).catch(() => {});
			browser.ws.close();
		}
	};
}

/** Öffnet /anstehend, wartet auf das Schloss im Zustand `stand` und klickt es. */
async function schlossKlicken(cdp, stand, anker, zustand) {
	await cdp.oeffnen('/anstehend');
	await cdp.bis(
		`document.querySelector('header a[data-schloss="${stand}"]') && document.querySelector('header a[data-zustand="${zustand}"]')`,
		20000,
		`Schloss ${stand}, Live-Punkt ${zustand}`
	);
	const s = await cdp.auswerten(`(() => {
		const a = document.querySelector('header a[data-schloss]');
		const r = a.getBoundingClientRect();
		return { href: a.getAttribute('href'), name: a.getAttribute('aria-label'), breite: r.width, hoehe: r.height };
	})()`);
	pruefe(s.href.startsWith('/anmelden') && s.href.endsWith(anker) && s.breite >= 44 - 0.5 && s.hoehe >= 44 - 0.5, `schloss: ${stand} („${s.name}“), Ziel ${s.href}, ${Math.round(s.breite)}×${Math.round(s.hoehe)} px`);
	await cdp.auswerten(`document.querySelector('header a[data-schloss]').click()`);
	await cdp.bis(`location.pathname === '/anmelden' && location.hash === '${anker}'`, 15000, `/anmelden${anker}`);
	await cdp.bis(`document.querySelector('#geraet') || document.querySelector('.vergleich')`, 15000, 'Antrag auf /anmelden');
	pruefe(true, `schloss: Klick führt auf /anmelden${anker}, der Antrag steht dort`);
}

/** Die Zeile eines Passkeys in der Liste auf /anmelden: Text und Knöpfe. */
function zeile(cdp, name) {
	return cdp.auswerten(`(() => {
		const li = [...document.querySelectorAll('.liste li')].find((l) => l.querySelector('b')?.textContent.trim() === ${JSON.stringify(name)});
		if (!li) return null;
		return { text: li.innerText, knoepfe: [...li.querySelectorAll('button')].map((b) => b.textContent.trim()) };
	})()`);
}

/** Klickt in der Zeile des Passkeys `name` den Knopf mit `teil`. */
async function zeileKlick(cdp, name, teil) {
	const ok = await cdp.auswerten(`(() => {
		const li = [...document.querySelectorAll('.liste li')].find((l) => l.querySelector('b')?.textContent.trim() === ${JSON.stringify(name)});
		const b = li && [...li.querySelectorAll('button')].find((b) => b.textContent.includes(${JSON.stringify(teil)}) && !b.disabled);
		if (!b) return false;
		b.click();
		return true;
	})()`);
	if (!ok) throw new Error(`Knopf „${teil}“ bei „${name}“ fehlt.\nSeite: ${await cdp.text()}`);
}

async function main() {
	// Etappe 4b: Testserver und beide Edge-Debug-Ports, wie alle Proben.
	await portePruefen(PORT);
	pruefe(existsSync(EXE), `Programm da: ${EXE}`);
	pruefe(existsSync(join(WEB, 'index.html')), `Oberfläche da: ${WEB}`);
	mkdirSync(join(ordner, 'bin'), { recursive: true });
	mkdirSync(join(ordner, 'profil'), { recursive: true });
	copyFileSync(EXE, join(ordner, 'bin', 'dashboard.exe'));
	const sid = eigeneSid();

	console.log('Teil A: Hauptkonto');
	let server = await serverStarten(sid);
	pruefe(existsSync(join(ordner, 'geheim', 'jwt-schluessel')), 'JWT-Schlüssel beim ersten Start angelegt');
	const cdp = await edgeStarten();

	await cdp.oeffnen('/vorgaenge');
	await cdp.bis(`document.body.innerText.includes('Was zwischen den Chats läuft')`, 20000, '/vorgaenge');
	pruefe(true, 'Hauptkonto: /vorgaenge ohne Passkey (Negotiate)');

	// (b) Angemeldet: Schloss offen, Antrag und darunter die Liste.
	await schlossKlicken(cdp, 'offen', '#passkey-anlegen', 'angemeldet');
	await cdp.bis(`document.querySelector('#h-passkeys')`, 15000, 'Liste der Passkeys');
	const unten = await cdp.auswerten(
		`Boolean(document.getElementById('passkey-anlegen').compareDocumentPosition(document.getElementById('h-passkeys')) & Node.DOCUMENT_POSITION_FOLLOWING)`
	);
	pruefe(unten, 'schloss: angemeldet steht die Liste der Passkeys unter dem Antrag');
	pruefe(JSON.stringify(await quellen(cdp)) === '["/api/events"]', `ein Kanal: angemeldet nur der geschützte (${JSON.stringify(await quellen(cdp))})`);

	// Ohne Namen
	// ist „Passkey anlegen“ klickbar und sagt, was fehlt; kein Antrag geht raus.
	const leer = await cdp.auswerten(`(() => {
		const f = document.getElementById('geraet');
		f.value = ''; f.dispatchEvent(new Event('input', { bubbles: true }));
		const b = document.querySelector('#passkey-anlegen button[type=submit]');
		const vorher = b.disabled;
		b.click();
		return { vorher };
	})()`);
	await cdp.bis(`document.getElementById('geraet-fehlt') !== null`, 5000, 'Satz zum fehlenden Namen');
	const nachLeer = await cdp.auswerten(
		`({ fokus: document.activeElement?.id, satz: document.getElementById('geraet-fehlt')?.textContent.trim(), invalid: document.getElementById('geraet').getAttribute('aria-invalid') })`
	);
	pruefe(
		leer.vorher === false && nachLeer.fokus === 'geraet' && nachLeer.invalid === 'true' && /Namen für das Gerät/.test(nachLeer.satz ?? ''),
		`anlegen: ohne Namen ist der Knopf klickbar und sagt, was fehlt (${JSON.stringify({ ...leer, ...nachLeer })})`
	);

	// RS256: die Seite bekommt nur RS256 angeboten (wie Windows Hello oft).
	const rsAuth = await authenticator(cdp);
	await cdp.senden('Page.addScriptToEvaluateOnNewDocument', {
		source: `(() => {
			const orig = navigator.credentials.create.bind(navigator.credentials);
			navigator.credentials.create = (o) => {
				if (window.__nurRs256 && o && o.publicKey) o.publicKey.pubKeyCredParams = [{ type: 'public-key', alg: -257 }];
				return orig(o);
			};
		})()`
	});
	await cdp.oeffnen('/anmelden');
	await cdp.auswerten('window.__nurRs256 = true');
	await cdp.eingeben('#geraet', 'Edge RS256');
	await cdp.klick('Passkey anlegen');
	await cdp.bis(`document.body.innerText.includes('angelegt und registriert')`, 20000, 'RS256 registriert');
	pruefe(true, 'Hauptkonto: Passkey mit RS256 angelegt, sofort registriert');
	await cdp.senden('WebAuthn.removeVirtualAuthenticator', { authenticatorId: rsAuth });

	const esAuth = await authenticator(cdp);
	await passkeyAnlegen(cdp, 'Edge ES256');
	await cdp.bis(`document.body.innerText.includes('angelegt und registriert')`, 20000, 'ES256 registriert');
	const liste = await cdp.auswerten(
		`fetch('/api/schluessel/passkeys', { headers: { accept: 'application/json' } }).then((r) => r.json())`
	);
	pruefe(
		liste.passkeys.length === 2 &&
			liste.passkeys.every((p) => p.status === 'registriert' && p.hauptkonto) &&
			liste.passkeys.some((p) => p.verfahren === 'RS256') &&
			liste.passkeys.some((p) => p.verfahren === 'ES256'),
		'Liste auf /schluessel: zwei registrierte Passkeys, RS256 und ES256'
	);
	await mitPasskeyAnmelden(cdp, '/schluessel');
	await cdp.bis(`location.pathname === '/schluessel'`, 20000, 'nach Anmeldung auf /schluessel');
	const c = await cdp.ausweis();
	pruefe(c !== null, 'Cookie __Host-dashboard gesetzt');
	pruefe(c.httpOnly && c.secure && c.sameSite === 'Strict' && c.path === '/', 'Cookie: HttpOnly, Secure, SameSite=Strict, Path=/');
	pruefe(!(await cdp.auswerten('document.cookie')).includes('__Host-dashboard'), 'Ausweis nicht in document.cookie');
	pruefe((await mitAusweis('/api/vorgaenge/uebersicht', c.value)) === 200, 'Ausweis allein (ohne Negotiate) öffnet die API');
	// Erst nach der Anmeldung sichern: Der Zähler im Authenticator ist dann aktuell
	// (ein älterer Stand wäre ein kopierter Passkey, und der Server weist ihn ab).
	const { credentials: esCreds } = await cdp.senden('WebAuthn.getCredentials', { authenticatorId: esAuth });
	await cdp.senden('WebAuthn.removeVirtualAuthenticator', { authenticatorId: esAuth });
	await cdp.senden('Network.clearBrowserCookies');
	await serverStoppen(server);

	console.log('Teil B: dasselbe Konto gilt als fremd (inhaber_sid umgestellt)');
	server = await serverStarten(FREMDE_SID);
	await cdp.oeffnen('/vorgaenge');
	await cdp.bis(`location.pathname === '/anmelden'`, 20000, 'Umleitung auf /anmelden');
	pruefe(
		(await cdp.auswerten('new URLSearchParams(location.search).get("ziel")')) === '/vorgaenge',
		'fremdes Konto: /vorgaenge leitet auf /anmelden?ziel=/vorgaenge'
	);
	const api = await cdp.auswerten(
		`fetch('/api/vorgaenge/uebersicht', { headers: { accept: 'application/json' } }).then(async (r) => [r.status, (await r.json()).art])`
	);
	pruefe(api[0] === 401 && api[1] === 'passkey', 'fremdes Konto: API 401, Art passkey');

	// (a) Nicht angemeldet (das Konto hat registrierte Passkeys aus Teil A): Schloss zu.
	await schlossKlicken(cdp, 'zu', '#mit-passkey-anmelden', 'nicht_angemeldet');
	pruefe(
		!(await cdp.auswerten(`Boolean(document.querySelector('#h-passkeys'))`)),
		'schloss: nicht angemeldet keine Liste der Passkeys'
	);
	pruefe(JSON.stringify(await quellen(cdp)) === '["/api/events/frei"]', `ein Kanal: nicht angemeldet nur der freie (${JSON.stringify(await quellen(cdp))})`);

	// Antrag mit einem neuen Authenticator.
	const neuAuth = await authenticator(cdp);
	await passkeyAnlegen(cdp, 'Antrag Edge');
	await cdp.bis(`document.querySelector('.vergleich .zahl')`, 20000, 'Vergleichszahl');
	const zahl = await cdp.auswerten(`document.querySelector('.vergleich .zahl').textContent.trim()`);
	pruefe(/^\d{4}$/.test(zahl), `Antrag ausstehend, Vergleichszahl ${zahl}`);
	const eigene = await cdp.auswerten(`fetch('/api/passkey/antrag').then((r) => r.json())`);
	const antragId = eigene.passkeys.find((p) => p.status === 'ausstehend')?.id;
	pruefe(antragId !== undefined && !eigene.hauptkonto, `Antrag mit Windows-Kontoname ${eigene.konto}`);
	const nochNicht = await cdp.auswerten(
		`fetch('/api/vorgaenge/uebersicht').then((r) => r.status)`
	);
	pruefe(nochNicht === 401, 'Antrag berechtigt zu nichts (401)');
	await mitPasskeyAnmelden(cdp);
	await cdp.bis(`document.querySelector('[role=alert]')`, 20000, 'Fehlermeldung');
	pruefe((await cdp.ausweis()) === null, 'ausstehender Passkey: keine Anmeldung, kein Cookie');
	const { credentials: neuCreds } = await cdp.senden('WebAuthn.getCredentials', { authenticatorId: neuAuth });
	await cdp.senden('WebAuthn.removeVirtualAuthenticator', { authenticatorId: neuAuth });

	// Freigabe mit dem ES256-Passkey aus Teil A (frische Anmeldung).
	const esWieder = await authenticator(cdp);
	for (const cred of esCreds) await cdp.senden('WebAuthn.addCredential', { authenticatorId: esWieder, credential: cred });
	await mitPasskeyAnmelden(cdp, '/anstehend');
	await cdp.bis(`location.pathname === '/anstehend'`, 20000, 'mit ES256 angemeldet');
	// (c) Hinter dem Schloss (offen, mit Passkey angemeldet): die Liste auf /anmelden.
	await schlossKlicken(cdp, 'offen', '#passkey-anlegen', 'angemeldet');
	await cdp.bis(`document.querySelector('.liste') && document.body.innerText.includes(${JSON.stringify(zahl)})`, 20000, 'Zahl auf /anmelden');
	pruefe(true, `/anmelden zeigt den fremden Antrag mit derselben Vergleichszahl ${zahl}`);
	// (d) Der Passkey dieser Sitzung: Satz, keine Knöpfe.
	const eigen = await zeile(cdp, 'Edge ES256');
	pruefe(
		eigen !== null && eigen.text.includes('Mit diesem Passkey bist du gerade angemeldet') && eigen.knoepfe.length === 0,
		`eigener Passkey ohne Knöpfe (${JSON.stringify(eigen?.knoepfe)})`
	);
	await zeileKlick(cdp, 'Antrag Edge', 'Freigeben');
	await cdp.klick('Ja, gleiche Zahl: freigeben');
	await cdp.bis(`document.body.innerText.includes('ist freigegeben')`, 20000, 'freigegeben');
	pruefe(true, 'Bestätigen auf /anmelden mit der Vergleichszahl, frischer Passkey aus dem anderen Konto');
	const esAusweis = (await cdp.ausweis()).value;
	await cdp.senden('Network.clearBrowserCookies');
	await cdp.senden('WebAuthn.removeVirtualAuthenticator', { authenticatorId: esWieder });

	// Anmelden mit dem freigegebenen Passkey, geschützte Seite öffnen.
	const neuWieder = await authenticator(cdp);
	for (const cred of neuCreds) await cdp.senden('WebAuthn.addCredential', { authenticatorId: neuWieder, credential: cred });
	await mitPasskeyAnmelden(cdp, '/vorgaenge');
	await cdp.bis(`document.body.innerText.includes('Was zwischen den Chats läuft')`, 20000, '/vorgaenge mit Passkey');
	pruefe(true, 'freigegebener Passkey: Anmeldung und geschützte Seite /vorgaenge');
	const neuAusweis = (await cdp.ausweis()).value;
	pruefe((await mitAusweis('/api/vorgaenge/uebersicht', neuAusweis)) === 200, 'laufende Sitzung des neuen Passkeys: 200');
	pruefe((await mitAusweis('/api/stand/voll', neuAusweis)) === 200, 'laufende Sitzung des neuen Passkeys: /api/stand/voll 200');

	// Dieser Tab hält die Sitzung des neuen Passkeys: geschützter Kanal, Schloss offen.
	await cdp.oeffnen('/anstehend');
	await kopfUndKanal(cdp, 'offen', 'angemeldet', '/api/events', 'Sitzung des neuen Passkeys');

	// (c) Widerruf mit dem ES256-Passkey auf /anmelden, in einem zweiten Tab mit eigenen Cookies.
	const zweiter = await zweiterTab();
	const tab2 = zweiter.tab;
	await tab2.senden('Network.setCookie', { name: '__Host-dashboard', value: esAusweis, url: BASIS, path: '/', secure: true, httpOnly: true, sameSite: 'Strict' });
	await schlossKlicken(tab2, 'offen', '#passkey-anlegen', 'angemeldet');
	await tab2.bis(`document.querySelector('.liste')`, 20000, 'Liste auf /anmelden');
	await zeileKlick(tab2, 'Antrag Edge', 'Widerrufen');
	await tab2.klick('Ja, widerrufen');
	await tab2.bis(`document.body.innerText.includes('ist widerrufen')`, 20000, 'widerrufen');
	pruefe(true, `Widerruf auf /anmelden mit dem ES256-Passkey (${antragId.slice(0, 8)}…)`);
	pruefe((await mitAusweis('/api/vorgaenge/uebersicht', neuAusweis)) === 401, 'Widerruf beendet die laufende Sitzung sofort (401)');
	pruefe((await mitAusweis('/api/stand/voll', neuAusweis)) === 401, 'widerrufene Sitzung: /api/stand/voll 401');
	pruefe((await mitAusweis('/api/stand', neuAusweis)) === 200, 'widerrufene Sitzung: /api/stand bleibt frei (200)');
	await zweiter.schliessen();
	// Der erste Tab merkt es von selbst: Schloss zu, „nicht angemeldet“, nur der freie Kanal.
	await kopfUndKanal(cdp, 'zu', 'nicht_angemeldet', '/api/events/frei', 'nach dem Widerruf');

	// (e) Ohne Verbindung: der freie Kanal wird abgewiesen; nur der Hinweis, keine Handlung.
	cdp.freiAbweisen = true;
	await cdp.oeffnen('/anmelden');
	await cdp.bis(`document.querySelector('header a[data-zustand="nicht_gefunden"]')`, 15000, 'Verbindung nicht gefunden');
	const ohne = await cdp.auswerten(`(() => {
		const main = document.querySelector('main');
		return {
			hinweis: main.innerText.includes('Ohne Verbindung zum Dashboard geht hier gerade nichts'),
			knoepfe: [...main.querySelectorAll('button')].map((b) => b.textContent.trim()).filter((t) => /Passkey|Freigeben|Widerrufen|Ablehnen|anmelden|Abmelden/i.test(t)),
			formular: document.querySelector('#geraet') !== null,
			liste: document.querySelector('.liste') !== null
		};
	})()`);
	pruefe(ohne.hinweis && ohne.knoepfe.length === 0 && !ohne.formular && !ohne.liste, `ohne Verbindung: nur der Hinweis, keine Passkey-Knöpfe (${JSON.stringify(ohne)})`);
	cdp.freiAbweisen = false;

	// Abmelden über den Knopf: Schloss zu, „nicht angemeldet“, nur der freie Kanal; der Ausweis ist gesperrt.
	pruefe((await mitAusweis('/api/vorgaenge/uebersicht', esAusweis)) === 200, 'ES256-Sitzung läuft noch');
	await cdp.senden('Network.setCookie', { name: '__Host-dashboard', value: esAusweis, url: BASIS, path: '/', secure: true, httpOnly: true, sameSite: 'Strict' });
	await cdp.oeffnen('/anmelden');
	await kopfUndKanal(cdp, 'offen', 'angemeldet', '/api/events', 'mit ES256 angemeldet');
	await cdp.bis(`[...document.querySelectorAll('button')].some((b) => b.textContent.trim() === 'Abmelden')`, 15000, 'Knopf „Abmelden“');
	await cdp.klick('Abmelden');
	await kopfUndKanal(cdp, 'zu', 'nicht_angemeldet', '/api/events/frei', 'nach dem Abmelden');
	pruefe((await mitAusweis('/api/vorgaenge/uebersicht', esAusweis)) === 401, 'Abmelden sperrt den Ausweis (401)');
	pruefe((await mitAusweis('/api/stand/voll', esAusweis)) === 401, 'abgemeldete Sitzung: /api/stand/voll 401');

	await serverStoppen(server);
	pruefe(
		cdp.gesperrt.length === 0,
		`Netzsperre: nur Anfragen an den eigenen Server${cdp.gesperrt.length > 0 ? ` (gesperrt: ${cdp.gesperrt.slice(0, 5).join(' | ')})` : ''}`
	);
	console.log(`\nBestanden: ${bestanden} Prüfungen.`);
}

let code = 1;
try {
	await main();
	code = 0;
} catch (e) {
	console.error(String(e?.stack ?? e));
	if (BEHALTEN) console.error(`Temp-Ordner bleibt: ${ordner}`);
} finally {
	await aufraeumen();
}
process.exit(code);
