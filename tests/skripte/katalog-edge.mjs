#!/usr/bin/env node
/**
 * Ende-zu-Ende-Probe für Etappe 3a-3 (Katalog, Grundteile auf Bits UI,
 * Widgets im Unter-Dashboard) mit Edge. Gleicher Weg wie
 * tests/skripte/zeitachse-edge.mjs: Wegwerf-Daten in einem Temp-Ordner,
 * Port 8788 (nie 8787, nie C:\ProgramData\Dashboard), Edge headless mit
 * Wegwerf-Profil, ohne Hintergrund-Netz und Komponenten-Updates, alle
 * Kindprozesse ohne Fenster, DevTools-Protokoll über den WebSocket von
 * Node 24. Kein Playwright, kein Browser-Download, keine Abhängigkeit.
 *
 * Netzsperre: Fetch.enable für alle Adressen; was nicht 127.0.0.1 oder
 * localhost ist, wird abgebrochen und lässt die Probe scheitern. Die Sperre
 * greift für die Seite und (seit 3a+, siehe unten) für alle anderen Ziele
 * des Browsers, auch Service-Worker; nur Anfragen des Browser-Prozesses
 * selbst nicht.
 *
 * Geprüft wird:
 *   - /katalog bei 360, 375 und 1280 px, je hell und dunkel: keine Fehler in
 *     der Konsole, kein seitliches Scrollen, keine überlappenden Kästen
 *     (Geschwister im Fluss, deren Rechtecke sich um mehr als 1 px
 *     überdecken), alle Abschnitte und Bausteine da, das Thema greift
 *   - Tastatur (1280 und 360 px): Knopf mit Enter, Fokus bleibt auf ihm,
 *     solange er läuft (aria-disabled statt disabled; Etappe 3a-4),
 *     Schalter mit Leertaste, Auswahl mit Enter,
 *     Pfeil und Enter (Liste im Bildschirm), Dialog mit Enter öffnen, Tab
 *     bleibt im Dialog, Escape schließt und der Fokus steht wieder auf dem
 *     Knopf, Kurzinfo beim Fokus, Reiter mit Pfeil, Aufklapper mit Enter,
 *     Tabelle: Tab-Stopp nur bei Überstand (360 px: Bereich, Tab auf den
 *     Rahmen, Pfeil rechts rollt, Schatten; bei 1280 px mit 60 rem Platz:
 *     passt, kein Bereich, Tab geht vorbei; danach wieder schmal)
 *   - jede CDP-Nachricht mit Zeitablauf (cdp-frist.mjs)
 *   - /projects/dashboard bei 360, 375 und 1280 px, hell und dunkel: die
 *     Widgets des Unter-Dashboards stehen da, kein seitliches Scrollen,
 *     keine Fehler in der Konsole; seit T4 (Zweig teilschritte) eine Arbeit
 *     mit drei Schritten (angelegt wie ein Chat über scripts\Api.ps1 am
 *     Testport): Zeile in Worten, beschriftete Liste, Stand und Warten als
 *     Wort, Link zur Aufgabe, keine Knöpfe, aufgeklappt ohne Überbreite
 *   - fünf Projektseiten mit „Wo was liegt“: nur als Hinweis (die
 *     Projektseiten stellen die Projekt-Chats auf <Tabelle> um)
 *
 * Aufruf aus dem Repo (vorher `cargo build` und `npm run build` in web\):
 *
 *     node tests\skripte\katalog-edge.mjs [--web <Ordner mit index.html>] [--bilder <Ordner>]
 *
 * Optional: --exe <dashboard.exe>, --edge <msedge.exe>, --port 8788,
 * --behalten (Temp-Ordner nicht löschen). Exit 0: alles bestanden, 1:
 * mindestens eine Prüfung nicht. Am Ende laufen weder Server noch Edge weiter.
 *
 * Netzsperre auf Browser-Ebene (Plan 3a+, edge-gemeinsam.mjs): Zusätzlich
 * zur Seite hängt sich eine zweite Verbindung mit Target.setAutoAttach
 * (flatten) an jedes andere Ziel, auch Service-Worker, Worker, weitere
 * Seiten und Frames, und sperrt dort ebenso mit Fetch.enable. Anfragen des
 * Browser-Prozesses selbst schalten die Startschalter ab. Strg+C räumt
 * Server, Edge und Temp-Ordner auf; ein belegter Port bricht vor dem Start ab.
 */
import { spawn, execFileSync } from "node:child_process";
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { antwortAbwarten } from "./cdp-frist.mjs";
import { beiAbbruch, browserSperren, mitFrist, portePruefen, inhaltOrt } from "./edge-gemeinsam.mjs";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
// Umzug des Inhalts: Kopien aus dashboard-inhalt, wenn es ihn gibt (edge-gemeinsam.mjs).
const INHALT = inhaltOrt(REPO);

function argument(name, standard) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : standard;
}

const PORT = Number(argument("port", "8788"));
if (PORT === 8787) {
  console.error("Port 8787 ist der echte Dienst. Bitte einen Testport nehmen.");
  process.exit(2);
}
const EXE = resolve(
  argument("exe", join(REPO, "target", "debug", "dashboard.exe")),
);
const WEB = resolve(argument("web", join(REPO, "web", "build")));
const EDGE = argument(
  "edge",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
);
const BILDER = argument("bilder", null);
const BEHALTEN = process.argv.includes("--behalten");
const DEBUG_PORT = PORT + 1000;
const BASIS = `http://localhost:${PORT}`;
const SYSTEM = (name) =>
  join(process.env.SystemRoot ?? "C:\\Windows", "System32", name);

const ordner = mkdtempSync(join(tmpdir(), "dashboard-katalog-e2e-"));
const prozesse = [];
/** Ob diese Probe ein Edge gestartet hat (nur das schließt das Aufräumen). */
let edgeGestartet = false;
// Ein Aufräumen für alle Wege (finally, Strg+C, unbehandelter Fehler).
const aufraeumen = beiAbbruch(aufraeumenJetzt);
let bestanden = 0;
const fehlgeschlagen = [];
const hinweise = [];

function pruefe(bedingung, satz, einzelheiten = "") {
  if (bedingung) {
    bestanden++;
    console.log(`  ok    ${satz}`);
  } else {
    fehlgeschlagen.push(einzelheiten ? `${satz} (${einzelheiten})` : satz);
    console.log(
      `  NEIN  ${satz}${einzelheiten ? `\n        ${einzelheiten}` : ""}`,
    );
  }
  return bedingung;
}

const pause = (ms) => new Promise((r) => setTimeout(r, ms));

async function aufraeumenJetzt() {
  try {
    // Nur ein Edge, das diese Probe gestartet hat; jeder Schritt mit Frist.
    if (!edgeGestartet) throw new Error("kein eigenes Edge");
    const v = await mitFrist(
      fetch(`http://127.0.0.1:${DEBUG_PORT}/json/version`).then((r) => r.json()),
      4000,
      "/json/version",
    );
    const b = await mitFrist(Cdp.verbinden(v.webSocketDebuggerUrl), 3000, "Verbinden");
    await Promise.race([b.senden("Browser.close", {}, 3000).catch(() => {}), pause(3000)]);
    await pause(1000);
  } catch {
    // Edge läuft nicht (mehr)
  }
  for (const p of prozesse) {
    try {
      execFileSync(
        SYSTEM("taskkill.exe"),
        ["/PID", String(p.pid), "/T", "/F"],
        {
          stdio: "ignore",
          windowsHide: true,
        },
      );
    } catch {
      // schon beendet
    }
  }
  prozesse.length = 0;
  try {
    const muster = ordner.replace(/'/g, "''");
    execFileSync(
      SYSTEM("WindowsPowerShell\\v1.0\\powershell.exe"),
      [
        "-NoProfile",
        "-Command",
        `Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains('${muster}') } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`,
      ],
      { stdio: "ignore", windowsHide: true },
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
  const zeile = execFileSync(
    SYSTEM("whoami.exe"),
    ["/user", "/fo", "csv", "/nh"],
    {
      encoding: "utf8",
      windowsHide: true,
    },
  );
  const sid = zeile.trim().split(",").pop().replace(/"/g, "");
  if (!/^S-1-\d+(-\d+)+$/.test(sid))
    throw new Error(`SID nicht lesbar: ${zeile}`);
  return sid;
}

async function serverStarten(inhaberSid) {
  const p = (rest) => join(ordner, rest).replace(/\\/g, "/");
  writeFileSync(
    join(ordner, "dienst.toml"),
    [
      "schema = 1",
      `repo = "${p("repo")}"`,
      `port = ${PORT}`,
      "",
      "[pfade]",
      `daten = "${p("daten/dashboard.redb")}"`,
      `sicherung = "${p("sicherung")}"`,
      `protokoll = "${p("protokoll")}"`,
      `web = "${WEB.replace(/\\/g, "/")}"`,
      `projekte = "${p("repo/projects")}"`,
      "",
      "[konten]",
      `inhaber_sid = "${inhaberSid}"`,
      "",
      "[bote]",
      `pipe = "dashboard-bote-katalog-${process.pid}"`,
      `programm = "${p("bin/gibt-es-nicht.exe")}"`,
      "",
    ].join("\n"),
  );
  const kind = spawn(
    join(ordner, "bin", "dashboard.exe"),
    ["--konsole", "--einstellungen", join(ordner, "dienst.toml")],
    {
      env: {
        ...process.env,
        USERPROFILE: join(ordner, "profil"),
        RUST_LOG: "warn",
      },
      stdio: ["ignore", "ignore", "pipe"],
      windowsHide: true,
    },
  );
  let fehler = "";
  kind.stderr.on("data", (d) => (fehler += d.toString()));
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

// ----- Edge über das DevTools-Protokoll -----

class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.nr = 0;
    this.offen = new Map();
    this.hoerer = [];
    this.gesperrt = [];
    ws.addEventListener("message", (e) => {
      const m = JSON.parse(String(e.data));
      if (m.id !== undefined && this.offen.has(m.id)) {
        const { ok, nein } = this.offen.get(m.id);
        this.offen.delete(m.id);
        if (m.error)
          nein(new Error(`${m.error.message} (${JSON.stringify(m.error)})`));
        else ok(m.result);
      } else if (m.method) {
        for (const h of [...this.hoerer]) h(m);
      }
    });
  }

  static async verbinden(url) {
    const ws = new WebSocket(url);
    await new Promise((ok, nein) => {
      ws.addEventListener("open", ok, { once: true });
      ws.addEventListener("error", nein, { once: true });
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

  /** Netzsperre: nur 127.0.0.1 und localhost; alles andere bricht ab und zählt als Fund. */
  async netzSperren() {
    this.hoerer.push((m) => {
      if (m.method !== "Fetch.requestPaused") return;
      const { requestId, request } = m.params;
      let host = "";
      try {
        host = new URL(request.url).hostname;
      } catch {
        // keine Adresse, gilt als fremd
      }
      if (host === "127.0.0.1" || host === "localhost" || host === "[::1]") {
        this.senden("Fetch.continueRequest", { requestId }).catch(() => {});
      } else {
        this.gesperrt.push(request.url);
        this.senden("Fetch.failRequest", {
          requestId,
          errorReason: "BlockedByClient",
        }).catch(() => {});
      }
    });
    await this.senden("Fetch.enable", { patterns: [{ urlPattern: "*" }] });
  }

  async auswerten(ausdruck) {
    const r = await this.senden("Runtime.evaluate", {
      expression: ausdruck,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r.exceptionDetails)
      throw new Error(`JS: ${JSON.stringify(r.exceptionDetails)}`);
    return r.result.value;
  }

  async oeffnen(pfad) {
    const geladen = this.warteAuf("Page.loadEventFired");
    // Scheitert das Navigieren, darf diese Frist nicht unbehandelt ablaufen.
    geladen.catch(() => {});
    await this.senden("Page.navigate", { url: `${BASIS}${pfad}` });
    await geladen;
    const ende = Date.now() + 8000;
    await pause(600);
    while (Date.now() < ende) {
      const laedt = await this.auswerten(
        `!document.querySelector('main') || document.querySelector('[aria-busy="true"]') !== null`,
      ).catch(() => true);
      if (!laedt) break;
      await pause(250);
    }
    await pause(500);
  }

  async taste(key, { code = key, vk = 0, text, shift = false } = {}) {
    const modifiers = shift ? 8 : 0;
    await this.senden("Input.dispatchKeyEvent", {
      type: "keyDown",
      key,
      code,
      windowsVirtualKeyCode: vk,
      modifiers,
      ...(text === undefined ? {} : { text }),
    });
    await this.senden("Input.dispatchKeyEvent", {
      type: "keyUp",
      key,
      code,
      windowsVirtualKeyCode: vk,
      modifiers,
    });
    await pause(150);
  }

  async breite(px) {
    await this.senden("Emulation.setDeviceMetricsOverride", {
      width: px,
      height: px < 600 ? 800 : 900,
      deviceScaleFactor: 1,
      mobile: px < 600,
      screenWidth: px,
      screenHeight: px < 600 ? 800 : 900,
    });
    await pause(300);
  }

  async thema(wert) {
    await this.senden("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: wert }],
    });
    await pause(150);
  }
}

const TASTEN = {
  leer: [" ", { code: "Space", vk: 32, text: " " }],
  enter: ["Enter", { vk: 13, text: "\r" }],
  escape: ["Escape", { vk: 27 }],
  tab: ["Tab", { vk: 9 }],
  shiftTab: ["Tab", { vk: 9, shift: true }],
  links: ["ArrowLeft", { vk: 37 }],
  rechts: ["ArrowRight", { vk: 39 }],
  runter: ["ArrowDown", { vk: 40 }],
};
async function druecke(cdp, name, mal = 1) {
  const [key, opt] = TASTEN[name];
  for (let i = 0; i < mal; i++) await cdp.taste(key, opt);
}

async function edgeStarten() {
  const profil = join(ordner, "edge-profil");
  mkdirSync(profil, { recursive: true });
  edgeGestartet = true;
  const edge = spawn(
    EDGE,
    [
      "--headless=new",
      `--remote-debugging-port=${DEBUG_PORT}`,
      `--user-data-dir=${profil}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-sync",
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-extensions",
      "--disable-component-extensions-with-background-pages",
      "--hide-scrollbars",
      "--auth-server-allowlist=localhost,127.0.0.1",
      "--window-size=1280,900",
      "about:blank",
    ],
    { stdio: "ignore", windowsHide: true },
  );
  prozesse.push(edge);
  for (let i = 0; i < 100; i++) {
    try {
      const liste = await (
        await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/list`)
      ).json();
      const seite = liste.find((t) => t.type === "page");
      if (seite) {
        const cdp = await Cdp.verbinden(seite.webSocketDebuggerUrl);
        await cdp.senden("Page.enable");
        await cdp.netzSperren();
        // Auch Service-Worker, Worker und weitere Ziele (edge-gemeinsam.mjs).
        cdp.browserSperre = await browserSperren(DEBUG_PORT, cdp.gesperrt, { ausser: seite.id });
        await cdp.senden("Runtime.enable");
        await cdp.senden("Log.enable");
        return cdp;
      }
    } catch {
      // Edge startet noch
    }
    await pause(200);
  }
  throw new Error("Edge antwortet nicht auf dem Debug-Port");
}

function konsoleMitschreiben(cdp) {
  const fehler = [];
  const h = (m) => {
    if (m.method === "Runtime.exceptionThrown") {
      const d = m.params.exceptionDetails;
      fehler.push(
        `Ausnahme: ${d.exception?.description ?? d.text}`.slice(0, 400),
      );
    } else if (
      m.method === "Runtime.consoleAPICalled" &&
      m.params.type === "error"
    ) {
      fehler.push(
        `console.error: ${m.params.args.map((a) => a.value ?? a.description ?? "").join(" ")}`.slice(
          0,
          400,
        ),
      );
    } else if (
      m.method === "Log.entryAdded" &&
      m.params.entry.level === "error"
    ) {
      fehler.push(
        `${m.params.entry.source}: ${m.params.entry.text} ${m.params.entry.url ?? ""}`.slice(
          0,
          400,
        ),
      );
    }
  };
  cdp.hoerer.push(h);
  return { fehler, stopp: () => cdp.hoerer.splice(cdp.hoerer.indexOf(h), 1) };
}

// ----- Prüfungen im Browser -----

/** Seitliches Scrollen (wie zeitachse-edge.mjs): Breite des Geräts zählt. */
const SEITLICH = `(() => {
	const geraet = screen.width;
	const zuBreit = [...document.querySelectorAll('main *')]
		.filter((e) => e.clientWidth > 0 && e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflowX === 'visible')
		.reverse()
		.slice(0, 4)
		.map((e) => e.tagName.toLowerCase() + (typeof e.className === 'string' && e.className ? '.' + e.className.trim().split(/\\s+/)[0] : '') + ' (' + e.scrollWidth + ' statt ' + e.clientWidth + ' px: ' + (e.textContent || '').trim().slice(0, 40) + ')');
	return { breit: document.documentElement.scrollWidth, fenster: window.innerWidth, geraet, zuBreit };
})()`;

/**
 * Überlappende Kästen: Geschwister im Fluss (nicht absolut oder fest
 * positioniert, nicht inline, sichtbar), deren Rechtecke sich in beiden
 * Richtungen um mehr als 1 px überdecken. Nur Kästen ab 4 px, damit Linien
 * und leere Hüllen nicht zählen. Dazu: Kästen, die rechts aus der Seite
 * ragen (rechter Rand größer als die Breite des Fensters), außer in einem
 * Rahmen, der selbst seitlich rollt.
 */
const UEBERLAPPUNG = `(() => {
	const imFluss = (e) => {
		const s = getComputedStyle(e);
		if (s.display === 'none' || s.display === 'contents' || s.display.startsWith('inline') || s.visibility === 'hidden') return false;
		if (s.position === 'absolute' || s.position === 'fixed' || s.float !== 'none') return false;
		if (e.classList.contains('nur-vorlesen')) return false;
		const b = e.getBoundingClientRect();
		return b.width >= 4 && b.height >= 4;
	};
	const rollt = (e) => {
		for (let p = e.parentElement; p && p !== document.body; p = p.parentElement) {
			const s = getComputedStyle(p);
			if (s.overflowX === 'auto' || s.overflowX === 'scroll' || s.overflowX === 'hidden') return true;
		}
		return false;
	};
	let paare = 0;
	let geprueft = 0;
	const beispiele = [];
	const name = (e) => e.tagName.toLowerCase() + (e.id ? '#' + e.id : '') + (typeof e.className === 'string' && e.className ? '.' + e.className.trim().split(/\\s+/).slice(0, 2).join('.') : '');
	for (const eltern of document.querySelectorAll('main, main *')) {
		const kinder = [...eltern.children].filter(imFluss);
		if (kinder.length < 2) continue;
		const r = kinder.map((k) => ({ k, b: k.getBoundingClientRect() }));
		geprueft += r.length;
		for (let i = 0; i < r.length; i++)
			for (let j = i + 1; j < r.length; j++) {
				const a = r[i].b, b = r[j].b;
				const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
				const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
				if (w > 1 && h > 1) {
					paare++;
					if (beispiele.length < 5) beispiele.push(name(r[i].k) + ' / ' + name(r[j].k));
				}
			}
	}
	const breite = document.documentElement.clientWidth;
	const raus = [...document.querySelectorAll('main *')]
		.filter((e) => imFluss(e) && !rollt(e) && e.getBoundingClientRect().right > breite + 1)
		.slice(0, 5)
		.map(name);
	return { geprueft, paare, beispiele, raus };
})()`;

const KATALOG = `(() => ({
	abschnitte: [...document.querySelectorAll('main section[id]')].map((s) => s.id),
	bausteine: document.querySelectorAll('main article.beispiel').length,
	gewaehlt: document.querySelectorAll('main .gewaehlt-schild').length,
	entwuerfe: document.querySelectorAll('main figure.entwurf').length,
	hintergrund: getComputedStyle(document.body).backgroundColor,
	fehlerBloecke: [...document.querySelectorAll('main .block-error')].map((e) => e.textContent.trim().slice(0, 120))
}))()`;

const aktiv = (cdp) =>
  cdp.auswerten(`(() => {
		const e = document.activeElement;
		return e ? { tag: e.tagName.toLowerCase(), rolle: e.getAttribute('role'), text: (e.textContent || '').trim().slice(0, 40), imDialog: !!e.closest('[role="dialog"]'), id: e.id } : null;
	})()`);

async function fokus(cdp, selektor) {
  return cdp.auswerten(`(() => {
		const e = document.querySelector(${JSON.stringify(selektor)});
		if (!e) return false;
		e.scrollIntoView({ block: 'center', behavior: 'instant' });
		e.focus();
		return document.activeElement === e;
	})()`);
}

const imBild = (selektor) => `(() => {
	const e = document.querySelector(${JSON.stringify(selektor)});
	if (!e) return null;
	const b = e.getBoundingClientRect();
	return { links: Math.round(b.left), rechts: Math.round(b.right), oben: Math.round(b.top), unten: Math.round(b.bottom), breite: innerWidth, hoehe: innerHeight, drin: b.left >= 0 && b.right <= innerWidth + 0.5 && b.top >= 0 && b.bottom <= innerHeight + 0.5 };
})()`;

/** Die Grundteile mit der Tastatur bedienen (auf der offenen Seite /katalog). */
async function grundteileBedienen(cdp, breite) {
  const w = `(${breite} px)`;
  // Knopf: Enter startet „läuft“ (1,5 s im Katalog-Beispiel). Der Knopf ist
  // dann nur aria-disabled, nicht disabled; der Fokus bleibt auf ihm (in
  // jsdom nicht beweisbar, darum hier im echten Edge).
  const KN = "#knopf .reihe button:nth-of-type(2)";
  if (pruefe(await fokus(cdp, KN), `Knopf: Fokus auf „Speichern“ ${w}`)) {
    await druecke(cdp, "enter");
    const knopf = `(() => { const e = document.querySelector('${KN}'); return { text: e.textContent.trim(), ariaDisabled: e.getAttribute('aria-disabled'), busy: e.getAttribute('aria-busy'), disabled: e.disabled, fokus: document.activeElement === e }; })()`;
    const waehrend = await cdp.auswerten(knopf);
    pruefe(
      waehrend.text === "wird gespeichert" &&
        waehrend.ariaDisabled === "true" &&
        waehrend.busy === "true" &&
        !waehrend.disabled,
      `Knopf: läuft, nur aria-disabled und aria-busy ${w}`,
      JSON.stringify(waehrend),
    );
    pruefe(
      waehrend.fokus,
      `Knopf: Fokus bleibt auf dem Knopf, solange er läuft ${w}`,
      JSON.stringify(await aktiv(cdp)),
    );
    await druecke(cdp, "enter");
    await pause(1800);
    const danach = await cdp.auswerten(knopf);
    pruefe(
      danach.text === "Speichern" &&
        danach.ariaDisabled === null &&
        danach.fokus,
      `Knopf: danach wieder bereit, Fokus noch auf ihm ${w}`,
      JSON.stringify(danach),
    );
  }

  // Schalter: Leertaste
  const S = '#schalter [role="switch"]';
  if (
    pruefe(await fokus(cdp, S), `Schalter: Fokus mit der Tastatur möglich ${w}`)
  ) {
    const vorher = await cdp.auswerten(
      `document.querySelector('${S}').getAttribute('aria-checked')`,
    );
    await druecke(cdp, "leer");
    const nachher = await cdp.auswerten(
      `document.querySelector('${S}').getAttribute('aria-checked')`,
    );
    pruefe(
      vorher !== nachher,
      `Schalter: Leertaste schaltet ${w}`,
      `${vorher} -> ${nachher}`,
    );
  }

  // Auswahl: Enter öffnet, Pfeil, Enter wählt, Fokus zurück
  const A = "#auswahl button[aria-haspopup]";
  if (pruefe(await fokus(cdp, A), `Auswahl: Fokus auf dem Knopf ${w}`)) {
    const vorher = await cdp.auswerten(
      `document.querySelector('${A}').textContent.trim()`,
    );
    await druecke(cdp, "enter");
    await pause(300);
    const liste = await cdp.auswerten(imBild('[role="listbox"]'));
    pruefe(liste !== null, `Auswahl: Enter öffnet die Liste ${w}`);
    if (liste)
      pruefe(
        liste.drin,
        `Auswahl: Liste ganz im Bildschirm ${w}`,
        JSON.stringify(liste),
      );
    await druecke(cdp, "runter");
    await druecke(cdp, "enter");
    await pause(300);
    const nachher = await cdp.auswerten(
      `document.querySelector('${A}').textContent.trim()`,
    );
    pruefe(
      vorher !== nachher,
      `Auswahl: Pfeil und Enter wählen ${w}`,
      `${vorher} -> ${nachher}`,
    );
    const zu = await cdp.auswerten(
      `document.querySelector('[role="listbox"]') === null`,
    );
    pruefe(zu, `Auswahl: Liste danach zu ${w}`);
    const a = await aktiv(cdp);
    pruefe(
      a?.tag === "button" &&
        a.text.includes(nachher.replace("▾", "").trim().slice(0, 8)),
      `Auswahl: Fokus wieder auf dem Knopf ${w}`,
      JSON.stringify(a),
    );
  }

  // Dialog: Enter öffnet, Tab bleibt drin, Escape schließt, Fokus zurück
  const D = '#dialog button[aria-haspopup="dialog"]';
  if (pruefe(await fokus(cdp, D), `Dialog: Fokus auf dem Knopf ${w}`)) {
    await druecke(cdp, "enter");
    await pause(400);
    const d = await cdp.auswerten(imBild('[role="dialog"]'));
    pruefe(d !== null, `Dialog: Enter öffnet ${w}`);
    if (d) {
      pruefe(d.drin, `Dialog: ganz im Bildschirm ${w}`, JSON.stringify(d));
      const name = await cdp.auswerten(
        `(() => { const d = document.querySelector('[role="dialog"]'); const id = d.getAttribute('aria-labelledby'); return id ? document.getElementById(id)?.textContent.trim() : null; })()`,
      );
      pruefe(
        name === "Eintrag löschen?",
        `Dialog: Name aus dem Titel ${w}`,
        String(name),
      );
      let drin = true;
      for (let i = 0; i < 4; i++) {
        await druecke(cdp, "tab");
        drin &&= (await aktiv(cdp))?.imDialog === true;
      }
      pruefe(drin, `Dialog: Tab bleibt im Dialog ${w}`);
      await druecke(cdp, "escape");
      await pause(400);
      pruefe(
        await cdp.auswerten(
          `document.querySelector('[role="dialog"]') === null`,
        ),
        `Dialog: Escape schließt ${w}`,
      );
      const a = await aktiv(cdp);
      pruefe(
        a?.text === "Löschen…",
        `Dialog: Fokus wieder auf dem Knopf ${w}`,
        JSON.stringify(a),
      );
    }
  }

  // Kurzinfo: erscheint beim Fokus, Escape schließt
  const K = "#kurzinfo .lebend button";
  // Fokus mit der Tastatur (Tab): Bits UI öffnet die Kurzinfo nur bei
  // Tastatur-Fokus, nicht bei focus() aus einem Skript.
  if (pruefe(await fokus(cdp, K), `Kurzinfo: Auslöser fokussierbar ${w}`)) {
    await druecke(cdp, "shiftTab");
    await druecke(cdp, "tab");
    pruefe(
      (await aktiv(cdp))?.text === "ⓘ",
      `Kurzinfo: mit Tab erreichbar ${w}`,
      JSON.stringify(await aktiv(cdp)),
    );
    await pause(700);
    const t = await cdp.auswerten(imBild('[role="tooltip"]'));
    pruefe(t !== null, `Kurzinfo: erscheint beim Fokus ${w}`);
    if (t) pruefe(t.drin, `Kurzinfo: im Bildschirm ${w}`, JSON.stringify(t));
    await druecke(cdp, "escape");
    await pause(300);
    pruefe(
      await cdp.auswerten(
        `document.querySelector('[role="tooltip"]') === null`,
      ),
      `Kurzinfo: Escape schließt ${w}`,
    );
  }

  // Reiter: Pfeil rechts
  const R = '#reiter [role="tab"]';
  if (pruefe(await fokus(cdp, R), `Reiter: Fokus auf dem ersten Reiter ${w}`)) {
    await druecke(cdp, "rechts");
    const gewaehlt = await cdp.auswerten(
      `[...document.querySelectorAll('${R}')].map((t) => t.getAttribute('aria-selected'))`,
    );
    pruefe(
      gewaehlt[1] === "true",
      `Reiter: Pfeil rechts wählt den nächsten ${w}`,
      JSON.stringify(gewaehlt),
    );
    await druecke(cdp, "links");
  }

  // Aufklapper: Enter
  const U = "#aufklapper button[aria-expanded]";
  if (pruefe(await fokus(cdp, U), `Aufklapper: Fokus auf dem Kopf ${w}`)) {
    await druecke(cdp, "enter");
    pruefe(
      (await cdp.auswerten(
        `document.querySelector('${U}').getAttribute('aria-expanded')`,
      )) === "true",
      `Aufklapper: Enter klappt auf ${w}`,
    );
    await druecke(cdp, "enter");
  }

  // Tabelle: Tab-Stopp nur bei Überstand (Auflage 3a-3). Schmal: die
  // Beispiel-Tabelle (mindestbreite 36 rem) rollt im Raster immer, also
  // Bereich mit Namen, Tab landet auf dem Rahmen, Pfeil rollt, Schatten.
  // Breit: bei 1280 px bekommt die Beispiel-Fläche 60 rem Platz, die Tabelle
  // passt, der ResizeObserver nimmt Bereich und tabindex weg und Tab geht
  // vorbei; danach wieder schmal, der Tab-Stopp kommt zurück.
  const RAHMEN = "#tabelle .scroll-x";
  await tabelleTabStopp(cdp, RAHMEN, true, `schmal ${w}`);
  if (breite >= 1000) {
    await cdp.auswerten(
      `(() => { const p = document.querySelector('${RAHMEN}').parentElement; p.style.width = '60rem'; p.style.maxWidth = 'none'; })()`,
    );
    await pause(400);
    await tabelleTabStopp(cdp, RAHMEN, false, `breit, 60 rem ${w}`);
    await cdp.auswerten(
      `(() => { const p = document.querySelector('${RAHMEN}').parentElement; p.style.width = ''; p.style.maxWidth = ''; })()`,
    );
    await pause(400);
    const zurueck = await cdp.auswerten(
      `document.querySelector('${RAHMEN}').getAttribute('tabindex')`,
    );
    pruefe(
      zurueck === "0",
      `Tabelle: wieder schmal, Tab-Stopp zurück ${w}`,
      String(zurueck),
    );
  }
}

/**
 * Tabelle im jetzigen Zustand: rollt sie wie `soll`, hat sie genau dann
 * Bereich, Namen und Tab-Stopp, und Tab vom Fokusziel davor landet genau
 * dann auf dem Rahmen. Rollt sie, rollt Pfeil rechts weiter.
 */
async function tabelleTabStopp(cdp, RAHMEN, soll, wie) {
  const stand = await cdp.auswerten(`(() => {
		const e = document.querySelector('${RAHMEN}');
		if (!e) return null;
		e.scrollLeft = 0;
		e.scrollIntoView({ block: 'center', behavior: 'instant' });
		const n = e.getAttribute('aria-labelledby');
		return {
			rollt: e.scrollWidth > e.clientWidth,
			rolle: e.getAttribute('role'),
			tabindex: e.getAttribute('tabindex'),
			name: n ? document.getElementById(n)?.textContent.trim() : null,
			rechts: e.classList.contains('schatten-rechts')
		};
	})()`);
  if (!pruefe(stand !== null, `Tabelle: Rahmen da (${wie})`)) return;
  pruefe(
    stand.rollt === soll,
    soll
      ? `Tabelle: rollt im eigenen Rahmen (${wie})`
      : `Tabelle: passt, rollt nicht (${wie})`,
    JSON.stringify(stand),
  );
  pruefe(
    stand.rollt
      ? stand.rolle === "region" &&
          stand.tabindex === "0" &&
          stand.name === "Wo was liegt"
      : stand.rolle === null && stand.tabindex === null,
    stand.rollt
      ? `Tabelle: rollt, also Bereich „Wo was liegt“ mit Tab-Stopp (${wie})`
      : `Tabelle: passt, also kein Bereich und kein Tab-Stopp (${wie})`,
    JSON.stringify(stand),
  );
  // Vom letzten Fokusziel vor dem Rahmen einmal Tab (von hinten das erste,
  // das den Fokus wirklich nimmt).
  const davor = await cdp.auswerten(`(() => {
		const r = document.querySelector('${RAHMEN}');
		const ziele = [...document.querySelectorAll('a[href], button, input, select, textarea, summary, [tabindex]')]
			.filter((e) => e !== r && !r.contains(e) && e.tabIndex >= 0)
			.filter((e) => r.compareDocumentPosition(e) & Node.DOCUMENT_POSITION_PRECEDING);
		for (const v of ziele.reverse()) {
			v.focus({ preventScroll: true });
			if (document.activeElement === v) return (v.textContent || v.tagName).trim().slice(0, 40);
		}
		return null;
	})()`);
  if (!pruefe(davor !== null, `Tabelle: Fokusziel vor dem Rahmen (${wie})`))
    return;
  await druecke(cdp, "tab");
  const aufRahmen = await cdp.auswerten(
    `document.activeElement === document.querySelector('${RAHMEN}')`,
  );
  pruefe(
    aufRahmen === stand.rollt,
    stand.rollt
      ? `Tabelle: Tab landet auf dem Rahmen (${wie})`
      : `Tabelle: Tab geht am Rahmen vorbei (${wie})`,
    `davor „${davor}“, danach ${JSON.stringify(await aktiv(cdp))}`,
  );
  if (stand.rollt && aufRahmen) {
    pruefe(
      stand.rechts,
      `Tabelle: Schatten rechts zeigt, dass mehr kommt (${wie})`,
    );
    await druecke(cdp, "rechts", 3);
    await pause(300);
    const nachher = await cdp.auswerten(
      `(() => { const e = document.querySelector('${RAHMEN}'); return { links: e.scrollLeft, schatten: e.classList.contains('schatten-links') }; })()`,
    );
    pruefe(
      nachher.links > 0,
      `Tabelle: Pfeil rechts rollt (${wie})`,
      JSON.stringify(nachher),
    );
    pruefe(nachher.schatten, `Tabelle: danach Schatten links (${wie})`);
  }
}

async function sicher(satz, f) {
  try {
    await f();
  } catch (e) {
    pruefe(false, satz, String(e?.message ?? e).slice(0, 300));
  }
}

async function seiteMessen(cdp, pfad, breite, thema) {
  const k = konsoleMitschreiben(cdp);
  await cdp.oeffnen(pfad);
  const w = await cdp.auswerten(SEITLICH);
  const u = await cdp.auswerten(UEBERLAPPUNG);
  k.stopp();
  const wo = `${pfad} (${breite} px, ${thema})`;
  pruefe(
    k.fehler.length === 0,
    `${wo}: keine Fehler in der Konsole`,
    k.fehler.join(" | "),
  );
  pruefe(
    w.breit <= w.geraet && w.fenster <= w.geraet,
    `${wo}: kein seitliches Scrollen`,
    `Seite ${w.breit} px, Fenster ${w.fenster} px, Gerät ${w.geraet} px; zu breit: ${w.zuBreit.join(", ")}`,
  );
  pruefe(
    u.paare === 0,
    `${wo}: keine überlappenden Kästen (${u.geprueft} geprüft)`,
    u.beispiele.join(" | "),
  );
  pruefe(
    u.raus.length === 0,
    `${wo}: nichts ragt rechts aus der Seite`,
    u.raus.join(" | "),
  );
  return { w, u };
}

// ----- Teilschritte (Zweig teilschritte, T4) -----

/**
 * Eine Anfrage wie ein Chat: scripts\Api.ps1 mit der Windows-Anmeldung, nur
 * an den Testserver dieser Probe (nie 8787).
 */
function wieEinChat(weg, koerper) {
  const aus = execFileSync(
    "pwsh.exe",
    ["-NoProfile", "-File", join(REPO, "scripts", "Api.ps1"), weg, "-Json", JSON.stringify(koerper), "-Server", `127.0.0.1:${PORT}`],
    { encoding: "utf8", windowsHide: true },
  );
  return JSON.parse(aus);
}

/**
 * Eine Arbeit mit drei Schritten in /projects/dashboard: 1 fertig, 2
 * angeboten an chat:architektur, 3 wartet auf dich (verweis auf eine Aufgabe).
 */
function schritteAnlegen() {
  const aufgabe = wieEinChat("api/aufgaben", { projekt: "dashboard", titel: "Edge-Probe T4: Antwort für Schritt 3" });
  const ar = wieEinChat("api/arbeiten", {
    projekt: "dashboard",
    von: "chat:dashboard",
    titel: "Edge-Probe T4: Arbeit mit Schritten",
    stand: "als-naechstes",
  }).id;
  for (const titel of ["bauen", "Abnahme mit einem längeren Titel, der bei 360 px umbrechen muss", "zusammenführen"])
    wieEinChat(`api/arbeiten/${ar}/schritte`, { von: "chat:dashboard", titel });
  const s = (nr, koerper) => wieEinChat(`api/arbeiten/${ar}/schritte/${nr}`, koerper);
  wieEinChat(`api/arbeiten/${ar}/schritte/1/uebernehmen`, { von: "chat:dashboard" });
  s(1, { von: "chat:dashboard", stand: "fertig" });
  s(2, { von: "chat:dashboard", anbieten: "chat:architektur" });
  wieEinChat(`api/arbeiten/${ar}/schritte/3/uebernehmen`, { von: "chat:hallo" });
  s(3, { von: "chat:hallo", stand: "wartet", wartet_auf: "inhaber", verweis: aufgabe.id });
  return { ar, aufgabe: aufgabe.id };
}

/** Wartet, bis der Kasten Arbeiten die Arbeit mit Schritten zeigt, klappt sie auf und liest sie. */
const SCHRITTE_LESEN = (ar) => `(async () => {
  for (let i = 0; i < 50; i++) {
    const d = document.getElementById(${JSON.stringify(`arbeit-${ar}`)});
    if (d?.querySelector('summary .kurz')) {
      d.open = true;
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      const liste = d.querySelector('ol[aria-label]');
      const punkte = [...(liste?.querySelectorAll(':scope > li') ?? [])];
      const rand = punkte.map((li) => getComputedStyle(li).borderLeftColor);
      return {
        kurz: d.querySelector('summary .kurz').textContent.replace(/\\s+/g, ' ').trim(),
        beschriftung: liste?.getAttribute('aria-label') ?? null,
        punkte: punkte.map((li) => li.textContent.replace(/\\s+/g, ' ').trim()),
        link: d.querySelector('ol a')?.getAttribute('href') ?? null,
        // T5: das Ziel des Links steht auf der Seite (nicht nur das href).
        ziel: ((h) => h !== null && document.getElementById(decodeURIComponent(h.slice(1))) !== null)(d.querySelector('ol a')?.getAttribute('href') ?? null),
        kopf: d.querySelector('summary').textContent.replace(/\\s+/g, ' ').trim(),
        trenner: d.querySelector('summary .nur-vorlesen')?.textContent ?? null,
        rand,
        knoepfe: d.querySelectorAll('button, [role=progressbar], progress').length,
        rechts: Math.max(...punkte.map((li) => li.getBoundingClientRect().right)),
        geraet: document.documentElement.clientWidth,
        breit: document.documentElement.scrollWidth,
      };
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  return null;
})()`;

async function main() {
  // Etappe 4b: Testserver und beide Edge-Debug-Ports, wie alle Proben.
  await portePruefen(PORT);
  if (!existsSync(EXE)) throw new Error(`Programm fehlt: ${EXE}`);
  if (!existsSync(join(WEB, "index.html")))
    throw new Error(`Oberfläche fehlt: ${WEB}`);
  mkdirSync(join(ordner, "bin"), { recursive: true });
  mkdirSync(join(ordner, "profil"), { recursive: true });
  copyFileSync(EXE, join(ordner, "bin", "dashboard.exe"));
  // KOPIEN von projects\ und data\: der Server darf darin nichts am Repo ändern.
  cpSync(INHALT.projects, join(ordner, "repo", "projects"), {
    recursive: true,
  });
  cpSync(INHALT.data, join(ordner, "repo", "data"), { recursive: true });
  console.log(`Temp-Ordner: ${ordner}`);

  await serverStarten(eigeneSid());
  const t4 = schritteAnlegen();
  const cdp = await edgeStarten();

  for (const thema of ["light", "dark"]) {
    await cdp.thema(thema);
    for (const breite of [360, 375, 1280]) {
      console.log(`\n== /katalog, ${breite} px, ${thema} ==`);
      await cdp.breite(breite);
      await sicher(`/katalog ${breite} ${thema}`, async () => {
        await seiteMessen(cdp, "/katalog", breite, thema);
        const k = await cdp.auswerten(KATALOG);
        pruefe(
          ["grundteile", "text", "daten", "zeit", "widgets"].every((s) =>
            k.abschnitte.includes(s),
          ),
          "/katalog: alle fünf Abschnitte",
          k.abschnitte.join(","),
        );
        pruefe(
          k.bausteine >= 30,
          `/katalog: ${k.bausteine} Bausteine mit Beispiel`,
        );
        pruefe(
          k.gewaehlt === 5 && k.entwuerfe === 15,
          "/katalog: 5 Widgets mit je 3 Entwürfen, je einer gewählt",
          JSON.stringify(k),
        );
        pruefe(
          k.fehlerBloecke.length === 0,
          "/katalog: keine Fehlerkästen",
          k.fehlerBloecke.join(" | "),
        );
        const dunkel = /rgb\(15, 20, 32\)/.test(k.hintergrund);
        pruefe(
          thema === "dark" ? dunkel : !dunkel,
          `/katalog: Thema ${thema} greift`,
          k.hintergrund,
        );
        if (BILDER !== null && breite !== 375) {
          mkdirSync(BILDER, { recursive: true });
          const m = await cdp.senden("Page.getLayoutMetrics");
          const g = m.cssContentSize ?? m.contentSize;
          const { data } = await cdp.senden("Page.captureScreenshot", {
            format: "png",
            captureBeyondViewport: true,
            clip: {
              x: 0,
              y: 0,
              width: breite,
              height: Math.min(Math.ceil(g.height), 16000),
              scale: 1,
            },
          });
          const datei = join(BILDER, `katalog-${breite}-${thema}.png`);
          writeFileSync(datei, Buffer.from(data, "base64"));
          console.log(`  Bild  ${datei}`);
        }
      });
    }
  }

  await cdp.thema("light");
  for (const breite of [1280, 360]) {
    console.log(`\n== Tastatur, ${breite} px ==`);
    await cdp.breite(breite);
    const k = konsoleMitschreiben(cdp);
    await cdp.oeffnen("/katalog");
    await sicher(`Grundteile bedienen (${breite} px)`, () =>
      grundteileBedienen(cdp, breite),
    );
    k.stopp();
    pruefe(
      k.fehler.length === 0,
      `Tastatur ${breite} px: keine Fehler in der Konsole`,
      k.fehler.join(" | "),
    );
  }

  for (const thema of ["light", "dark"]) {
    await cdp.thema(thema);
    for (const breite of [1280, 375, 360]) {
      console.log(`\n== Unter-Dashboard, ${breite} px, ${thema} ==`);
      await cdp.breite(breite);
      await sicher(`/projects/dashboard ${breite} ${thema}`, async () => {
        await seiteMessen(cdp, "/projects/dashboard", breite, thema);
        const n = await cdp.auswerten(
          `[...document.querySelectorAll('.unter .widget h2')].map((h) => h.textContent.replace(/\\s+/g, ' ').trim())`,
        );
        pruefe(
          n.length > 0 && n[0] === "Stand",
          "/projects/dashboard: Widgets des Unter-Dashboards da",
          n.join(", "),
        );
        // T4: die Arbeit mit Schritten, aufgeklappt.
        const s = await cdp.auswerten(SCHRITTE_LESEN(t4.ar));
        const wo = `Schritte (${breite} px, ${thema})`;
        if (!pruefe(s !== null, `${wo}: die Arbeit zeigt die Zeile in Worten`)) return;
        pruefe(
          s.kurz === "1 von 3 fertig · dran: Chat hallo · angeboten an Chat architektur, noch nicht übernommen",
          `${wo}: „1 von 3 fertig · dran: Chat hallo · angeboten an Chat architektur, noch nicht übernommen“`,
          s.kurz,
        );
        pruefe(
          s.beschriftung === "Schritte von „Edge-Probe T4: Arbeit mit Schritten“" && s.punkte.length === 3,
          `${wo}: beschriftete Liste mit drei Schritten`,
          `${s.beschriftung}; ${s.punkte.join(" | ")}`,
        );
        pruefe(
          /^1\. bauen fertig · erledigt von Chat dashboard/.test(s.punkte[0] ?? "") &&
            /als Nächstes · angeboten an Chat architektur, noch nicht übernommen/.test(s.punkte[1] ?? "") &&
            /wartet auf dich · dran: Chat hallo .*zur Aufgabe/.test(s.punkte[2] ?? ""),
          `${wo}: je Schritt Stand, wer dran ist und worauf er wartet in Worten`,
          s.punkte.join(" | "),
        );
        pruefe(s.link === `#aufgabe-${t4.aufgabe}`, `${wo}: wartet auf dich führt zur Aufgabe`, String(s.link));
        pruefe(s.ziel, `${wo}: das Ziel des Links (die Aufgabe) steht auf der Seite`, String(s.link));
        pruefe(
          s.kopf.startsWith("Edge-Probe T4: Arbeit mit Schritten, 1 von 3 fertig") && !s.kopf.includes("▸") && s.trenner === ", ",
          `${wo}: Zeile ohne zweites Dreieck, mit Komma nur zum Vorlesen`,
          s.kopf,
        );
        pruefe(s.knoepfe === 0, `${wo}: keine Knöpfe, keine Fortschrittsleiste`, String(s.knoepfe));
        pruefe(new Set(s.rand).size === 3, `${wo}: Rand je Stand in eigener Farbe (zusätzlich zum Wort)`, s.rand.join(", "));
        pruefe(
          s.breit <= s.geraet && s.rechts <= s.geraet,
          `${wo}: aufgeklappt kein seitliches Scrollen`,
          `Seite ${s.breit} px, Liste bis ${Math.round(s.rechts)} px, Gerät ${s.geraet} px`,
        );
        if (BILDER !== null && breite === 360) {
          mkdirSync(BILDER, { recursive: true });
          await cdp.auswerten(`document.getElementById(${JSON.stringify(`arbeit-${t4.ar}`)}).scrollIntoView({ block: 'start' })`);
          const { data } = await cdp.senden("Page.captureScreenshot", { format: "png" });
          const datei = join(BILDER, `schritte-${breite}-${thema}.png`);
          writeFileSync(datei, Buffer.from(data, "base64"));
          console.log(`  Bild  ${datei}`);
        }
      });
    }
  }

  console.log(
    "\n== Projektseiten mit „Wo was liegt“ (Hinweis, Umstellung bei den Projekt-Chats) ==",
  );
  await cdp.thema("light");
  await cdp.breite(375);
  // Einige Projekte der Kopie (ohne feste Namen; Umzug des Inhalts).
  const mitNeu = readdirSync(join(ordner, "repo", "projects"))
    .filter((s) => /^[a-z0-9-]+$/.test(s) && existsSync(join(ordner, "repo", "projects", s, "projekt.toml")))
    .slice(0, 5);
  for (const slug of mitNeu) {
    await cdp.oeffnen(`/projects/${slug}`);
    const w = await cdp.auswerten(SEITLICH);
    const satz = `/projects/${slug} (375 px): ${w.breit <= w.geraet ? "kein seitliches Scrollen" : `Seite ${w.breit} px breit (${w.zuBreit.slice(0, 2).join(", ")})`}`;
    hinweise.push(satz);
    console.log(`  Hinweis ${satz}`);
  }

  pruefe(
    cdp.gesperrt.length === 0,
    "Netzsperre: die Seiten fragen nur beim eigenen Server an",
    cdp.gesperrt.slice(0, 5).join(" | "),
  );
  console.log(
    `\nBestanden: ${bestanden} Prüfungen. Nicht bestanden: ${fehlgeschlagen.length}.`,
  );
  for (const f of fehlgeschlagen) console.log(`  - ${f}`);
  if (hinweise.length > 0)
    console.log(`Hinweise:\n${hinweise.map((h) => `  ${h}`).join("\n")}`);
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
