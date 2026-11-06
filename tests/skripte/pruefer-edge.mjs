#!/usr/bin/env node
/**
 * Ende-zu-Ende-Probe für Etappe 3b A (Funde des Prüfers) mit Edge.
 *
 * Gleicher Weg wie pc-edge.mjs, katalog-edge.mjs und zeitachse-edge.mjs:
 * Wegwerf-Daten in einem Temp-Ordner (Kopien von projects\ und den
 * eingecheckten Dateien aus data\), Port 8788 (nie 8787, nie
 * C:\ProgramData\Dashboard), Edge headless mit Wegwerf-Profil, ohne
 * Hintergrund-Netz und Komponenten-Updates, alle Kindprozesse ohne Fenster,
 * DevTools-Protokoll über den WebSocket von Node 24 (Senden mit Frist aus
 * cdp-frist.mjs). Kein Playwright, kein Browser-Download, keine Abhängigkeit.
 *
 * Netzsperre wie in den anderen Proben: Fetch.enable auf der Seite und auf
 * Browser-Ebene (edge-gemeinsam.mjs); was nicht 127.0.0.1 oder localhost
 * ist, bricht ab und lässt die Probe scheitern. Ist Port 8788 oder ein
 * Debug-Port belegt, bricht die Probe vor dem Start ab. Strg+C räumt
 * Server, Edge und Temp-Ordner auf.
 *
 * Zwei Edge: eines meldet sich bei localhost bei Windows an (die Seiten mit
 * Anmeldung), das zweite nie (der freie Weg über 127.0.0.1, wie ein Browser
 * ohne Anmeldung).
 *
 * Je Fund des Prüfers (Projekt der Prüfer-Rolle, Zuständig Rust-Dashboard) eine
 * Prüfung; die Zeilen beginnen mit dem Kürzel des Funds:
 *   breite      Seiten bei 375 und 360 px ohne seitliches Scrollen (ab 4d
 *               auch /anleitung, /zielbild, /entscheidungen); /zielbild mit
 *               Testdaten: Tabelle und Beispiel rollen bei 360 px im
 *               eigenen Rahmen, ein kaputter Block steht als Hinweis da;
 *               /anleitung (6b) mit den echten docs/anleitung: angemeldet
 *               alle Abschnitte, zuerst die acht Schritte für einen Chat,
 *               bei 360 px ohne Überbreite, ohne Anmeldung nur der Weg
 *               über Edge
 *   nichts      „nichts für dich zu tun“ nie neben Fragen an dich
 *   zeitachse   keine Beschriftung über einer anderen, nichts über den Rand
 *               (/pc, /, eine Projektseite der neuen Form, bei 1280 und 375 px)
 *   fehlerseite 404 vom Server, deutscher Satz, Tab-Titel, Link zur
 *               Übersicht, keine Pfade, Versionen oder „cargo“
 *   api-kopf    /api/projekte/<slug> und seine daten-Adressen passen zusammen
 *   anstehend   Stufen-Knöpfe beschriftet und groß genug, Edge-Hinweis einmal
 *               mit Link, „Stufe“ verständlich
 *   bedienung   Heute auf jedem Zeitbaustein, Zoom um heute, Regler-Schritt,
 *               Zoom übersteht den Reiterwechsel, Konfetti räumt auf
 *   handy       Gantt am Handy: Breite, Knöpfe 44 px, Achse im Rahmen
 *   anmeldung   127.0.0.1 ohne Anmeldung: höchstens ein 401 je Laden;
 *               geschützte Adressen geben weiter 401
 *   du          /schluessel spricht mit „du“
 *   fachsprache Fußzeile ohne Dateinamen und Technik
 *   neue-form   (3b-2, seit 6a jedes Projekt) Projekte zeigen
 *               Seite.svelte direkt unter /projects/<slug>, ohne Hinweis auf
 *               eine neue Ansicht; /neu und /neu/<name> leiten nur noch
 *               dorthin weiter (Server 307, Browser landet dort; unbekanntes
 *               Projekt 404); die Seite fragt
 *               nur beim eigenen Server an (Netzsperre)
 *   kopfleiste-2 (Kopfleiste, Fassung 2) mit und ohne Anmeldung bei 360, 375
 *               und 1280 px: Live-Punkt als Link zu /pc#dashboard mit dem
 *               passenden Zustand, Schloss offen oder zu mit Ziel auf der
 *               Passkey-Seite, beide mindestens 44 px, nichts überlappt,
 *               kein seitliches Scrollen; Klick auf beide landet richtig;
 *               je Seite genau eine offene EventSource (angemeldet der
 *               geschützte Kanal, sonst der freie; gezählt über eine Hülle
 *               um window.EventSource, Abnahme); seit Fassung 3
 *               rechts in der Gruppe mit PC-Symbol und Glocke
 *   kopfleiste-3 (Kopfleiste, Fassung 3) mit und ohne Anmeldung bei 360,
 *               375, 768 und 1280 px, hell und dunkel: Brotkrumen (Symbol,
 *               Glieder, Blatt; am Handy „…“), rechts PC, Glocke, Live-Punkt,
 *               Schloss in dieser Reihenfolge, nichts überlappt, Ziele ab
 *               44 px, kein seitliches Scrollen, genau eine EventSource;
 *               unter 1024 px die Schublade (Tastatur: Enter öffnet, Escape
 *               schließt, Fokus zurück; nach einem Seitenwechsel aus der
 *               Schublade ist sie zu und die Seite bedienbar und rollt), ab
 *               1024 px die feste Leiste ohne PC; die Glocke (mit Anmeldung
 *               eine Zahl oder „keine“, ohne nur der Hinweis) öffnet ein
 *               Klappfeld ohne Seitensperre; das Blatt scrollt nach oben,
 *               fragt /api/stand und lädt die Seite nicht neu; das PC-Symbol
 *               zeigt Werte auch ohne Anmeldung. Auflagen aus der Abnahme
 *               (bei 768 px): Schublade offen und Fenster auf 1280 px: sie
 *               schließt, der Fokus steht sichtbar auf dem Inhalt (nie auf
 *               dem versteckten Knopf); ein Klick daneben schließt; ein
 *               Doppelklick auf den Knopf lässt sie offen; das Blatt hat
 *               eine Beschreibung für Vorlese-Programme; / holt
 *               /api/projects genau einmal
 *
 * Aufruf aus dem Repo (vorher `cargo build` und `npm run build` in web\):
 *
 *     node tests\skripte\pruefer-edge.mjs [--web <Ordner mit index.html>]
 *
 * Optional: --exe <dashboard.exe>, --edge <msedge.exe>, --port 8788,
 * --behalten (Temp-Ordner nicht löschen). Exit 0: alles bestanden, 1:
 * mindestens eine Prüfung nicht. Am Ende laufen weder Server noch Edge weiter.
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
import { beiAbbruch, browserSperren, mitFrist, portePruefen as portePruefenGemeinsam, inhaltOrt } from "./edge-gemeinsam.mjs";

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
const EXE = resolve(argument("exe", join(REPO, "target", "debug", "dashboard.exe")));
const WEB = resolve(argument("web", join(REPO, "web", "build")));
const EDGE = argument("edge", "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe");
const BEHALTEN = process.argv.includes("--behalten");
const DEBUG_PORT = PORT + 1000;
const DEBUG_PORT_FREI = DEBUG_PORT + 1;
const BASIS = `http://localhost:${PORT}`;
const BASIS_FREI = `http://127.0.0.1:${PORT}`;
const SYSTEM = (name) => join(process.env.SystemRoot ?? "C:\\Windows", "System32", name);

const ordner = mkdtempSync(join(tmpdir(), "dashboard-pruefer-e2e-"));
const prozesse = [];
// Ein Aufräumen für alle Wege (finally, Strg+C, unbehandelter Fehler).
const aufraeumen = beiAbbruch(aufraeumenJetzt);
let bestanden = 0;
const fehlgeschlagen = [];
const hinweise = [];
/** Messwerte je Fund, am Ende als Liste (für die Etappen-Notiz). */
const messwerte = [];

function pruefe(bedingung, satz, einzelheiten = "") {
  if (bedingung) {
    bestanden++;
    console.log(`  ok    ${satz}${einzelheiten ? ` [${einzelheiten}]` : ""}`);
  } else {
    fehlgeschlagen.push(einzelheiten ? `${satz} (${einzelheiten})` : satz);
    console.log(`  NEIN  ${satz}${einzelheiten ? `\n        ${einzelheiten}` : ""}`);
  }
  return bedingung;
}

function messen(fund, wert) {
  messwerte.push(`${fund}: ${wert}`);
}

const pause = (ms) => new Promise((r) => setTimeout(r, ms));
const eigeneDebugPorts = new Set();

async function edgeSchliessen(port) {
  if (!eigeneDebugPorts.has(port)) return;
  try {
    const v = await mitFrist(
      fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(3000) }).then((r) => r.json()),
      4000,
      "/json/version",
    );
    const b = await mitFrist(Cdp.verbinden(v.webSocketDebuggerUrl), 3000, "Verbinden");
    await Promise.race([b.senden("Browser.close", {}, 3000).catch(() => {}), pause(3000)]);
    await pause(1000);
  } catch {
    // Edge läuft nicht (mehr); taskkill räumt auf
  }
}

async function aufraeumenJetzt() {
  await edgeSchliessen(DEBUG_PORT);
  await edgeSchliessen(DEBUG_PORT_FREI);
  for (const p of prozesse) {
    try {
      execFileSync(SYSTEM("taskkill.exe"), ["/PID", String(p.pid), "/T", "/F"], {
        stdio: "ignore",
        windowsHide: true,
      });
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
  const zeile = execFileSync(SYSTEM("whoami.exe"), ["/user", "/fo", "csv", "/nh"], {
    encoding: "utf8",
    windowsHide: true,
  });
  const sid = zeile.trim().split(",").pop().replace(/"/g, "");
  if (!/^S-1-\d+(-\d+)+$/.test(sid)) throw new Error(`SID nicht lesbar: ${zeile}`);
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
      `pipe = "dashboard-bote-pruefer-${process.pid}"`,
      `programm = "${p("bin/gibt-es-nicht.exe")}"`,
      "",
    ].join("\n"),
  );
  const kind = spawn(join(ordner, "bin", "dashboard.exe"), ["--konsole", "--einstellungen", join(ordner, "dienst.toml")], {
    env: { ...process.env, USERPROFILE: join(ordner, "profil"), RUST_LOG: "warn" },
    stdio: ["ignore", "ignore", "pipe"],
    windowsHide: true,
  });
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
    this.basis = BASIS;
    ws.addEventListener("message", (e) => {
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
      ws.addEventListener("open", ok, { once: true });
      ws.addEventListener("error", nein, { once: true });
    });
    return new Cdp(ws);
  }

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

  async netzSperren({ anmeldungAbbrechen = false } = {}) {
    this.anmeldeRueckfragen = 0;
    this.hoerer.push((m) => {
      if (m.method === "Fetch.authRequired") {
        this.anmeldeRueckfragen++;
        this.senden("Fetch.continueWithAuth", {
          requestId: m.params.requestId,
          authChallengeResponse: { response: "CancelAuth" },
        }).catch(() => {});
        return;
      }
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
        this.senden("Fetch.failRequest", { requestId, errorReason: "BlockedByClient" }).catch(() => {});
      }
    });
    await this.senden("Fetch.enable", {
      patterns: [{ urlPattern: "*" }],
      ...(anmeldungAbbrechen ? { handleAuthRequests: true } : {}),
    });
  }

  /** Eine Funktion im Browser ausführen (als Text, mit JSON-Argument). */
  async rufe(fn, arg) {
    const r = await this.senden("Runtime.evaluate", {
      expression: `(${fn.toString()})(${JSON.stringify(arg ?? null)})`,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r.exceptionDetails) throw new Error(`JS: ${JSON.stringify(r.exceptionDetails).slice(0, 600)}`);
    return r.result.value;
  }

  async oeffnen(pfad) {
    const geladen = this.warteAuf("Page.loadEventFired");
    // Scheitert das Navigieren, darf diese Frist nicht unbehandelt ablaufen.
    geladen.catch(() => {});
    await this.senden("Page.navigate", { url: `${this.basis}${pfad}` });
    await geladen;
    const ende = Date.now() + 8000;
    await pause(600);
    while (Date.now() < ende) {
      const laedt = await this.rufe(
        () => !document.querySelector("main") || document.querySelector('[aria-busy="true"]') !== null,
      ).catch(() => true);
      if (!laedt) break;
      await pause(250);
    }
    await pause(700);
  }

  async taste(key, { code = key, vk = 0, text, ctrl = false } = {}) {
    const modifiers = ctrl ? 2 : 0;
    await this.senden("Input.dispatchKeyEvent", {
      type: "keyDown",
      key,
      code,
      windowsVirtualKeyCode: vk,
      modifiers,
      ...(text === undefined ? {} : { text }),
    });
    await this.senden("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: vk, modifiers });
    await pause(200);
  }

  /** Ein Mausklick an (x, y) über das DevTools-Protokoll; `anzahl` 2: der zweite Klick eines Doppelklicks. */
  async klickBei(x, y, anzahl = 1) {
    await this.senden("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await this.senden("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: anzahl });
    await this.senden("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: anzahl });
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

async function edgeStarten({ port = DEBUG_PORT, profilName = "edge-profil", anmeldungFuer = "localhost,127.0.0.1", frei = false } = {}) {
  const profil = join(ordner, profilName);
  mkdirSync(profil, { recursive: true });
  eigeneDebugPorts.add(port);
  const edge = spawn(
    EDGE,
    [
      "--headless=new",
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${profil}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-sync",
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-extensions",
      "--disable-component-extensions-with-background-pages",
      "--hide-scrollbars",
      `--auth-server-allowlist=${anmeldungFuer}`,
      "--window-size=1280,900",
      "about:blank",
    ],
    { stdio: "ignore", windowsHide: true },
  );
  prozesse.push(edge);
  let seite = null;
  for (let i = 0; i < 100 && seite === null; i++) {
    try {
      const liste = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      seite = liste.find((t) => t.type === "page") ?? null;
    } catch {
      // Edge startet noch
    }
    if (seite === null) await pause(200);
  }
  if (seite === null) throw new Error("Edge antwortet nicht auf dem Debug-Port");
  const cdp = await Cdp.verbinden(seite.webSocketDebuggerUrl);
  if (frei) cdp.basis = BASIS_FREI;
  await cdp.senden("Page.enable");
  // Kopfleiste, Fassung 2: offene EventSource je Seite zählen (Hülle um window.EventSource).
  await cdp.senden("Page.addScriptToEvaluateOnNewDocument", {
    source: `(() => {
      const O = window.EventSource;
      if (!O) return;
      const alle = new Set();
      window.__eventQuellen = () => [...alle].filter((q) => q.readyState !== 2).map((q) => new URL(q.url, location.href).pathname);
      window.EventSource = class extends O {
        constructor(u, o) { super(u, o); alle.add(this); }
        close() { alle.delete(this); super.close(); }
      };
    })()`,
  });
  await cdp.netzSperren({ anmeldungAbbrechen: frei });
  cdp.browserSperre = await browserSperren(port, cdp.gesperrt, { ausser: seite.id });
  await cdp.senden("Runtime.enable");
  await cdp.senden("Log.enable");
  await cdp.senden("Network.enable");
  return cdp;
}

/** Fehler in der Konsole mitschreiben (401/403 des freien Wegs zählen nicht). */
function konsoleMitschreiben(cdp, { ohne401 = false } = {}) {
  const fehler = [];
  const h = (m) => {
    if (m.method === "Runtime.exceptionThrown") {
      const d = m.params.exceptionDetails;
      fehler.push(`Ausnahme: ${d.exception?.description ?? d.text}`.slice(0, 300));
    } else if (m.method === "Log.entryAdded" && m.params.entry.level === "error") {
      const t = m.params.entry.text ?? "";
      if (ohne401 && /status of 40[134]/.test(t)) return;
      fehler.push(`${m.params.entry.source}: ${t} ${m.params.entry.url ?? ""}`.slice(0, 300));
    }
  };
  cdp.hoerer.push(h);
  return { fehler, stopp: () => cdp.hoerer.splice(cdp.hoerer.indexOf(h), 1) };
}

/** Antworten mit Code zählen, solange `stopp` nicht gerufen ist. */
function antwortenZaehlen(cdp) {
  const antworten = [];
  const h = (m) => {
    if (m.method === "Network.responseReceived") {
      antworten.push({ status: m.params.response.status, url: m.params.response.url });
    }
  };
  cdp.hoerer.push(h);
  return { antworten, stopp: () => cdp.hoerer.splice(cdp.hoerer.indexOf(h), 1) };
}

async function sicher(satz, f) {
  try {
    await f();
  } catch (e) {
    pruefe(false, satz, String(e?.message ?? e).slice(0, 400));
  }
}

// ----- Messungen im Browser (als Funktionen, damit kein Rückwärtsstrich verloren geht) -----

/** Seitliches Scrollen bei schmaler Breite (wie pc-edge.mjs). */
function SEITLICH() {
  const geraet = screen.width;
  const zuBreit = [...document.querySelectorAll("body *")]
    .filter((e) => e.clientWidth > 0 && e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflowX === "visible")
    .reverse()
    .slice(0, 4)
    .map((e) => `${e.tagName.toLowerCase()}${typeof e.className === "string" && e.className ? "." + e.className.trim().split(/\s+/)[0] : ""} (${e.scrollWidth} statt ${e.clientWidth} px: ${(e.textContent || "").trim().slice(0, 40)})`);
  // Dazu, was über den rechten Rand ragt (auch ohne überlaufende Eltern, etwa
  // absolut gesetzt): die innersten solchen Elemente.
  const rand = document.documentElement.clientWidth + 1;
  // Was in einem Rahmen mit eigenem Überlauf (hidden, auto, clip) steckt, zählt nicht.
  const imRahmen = (e) => {
    for (let p = e.parentElement; p && p !== document.body; p = p.parentElement) if (getComputedStyle(p).overflowX !== "visible") return true;
    return false;
  };
  const ragend = [...document.querySelectorAll("body *")].filter((e) => e.getBoundingClientRect().right > rand && !imRahmen(e));
  const ragen = ragend
    .filter((e) => !ragend.some((k) => k !== e && e.contains(k)))
    .slice(0, 4)
    .map((e) => {
      const r = e.getBoundingClientRect();
      const name = `${e.tagName.toLowerCase()}${typeof e.className === "string" && e.className ? "." + e.className.trim().split(/\s+/)[0] : ""}`;
      return `${name} ragt bis ${Math.round(r.right)} px (${Math.round(r.width)} breit, ${getComputedStyle(e).position}: ${(e.textContent || "").trim().slice(0, 30)})`;
    });
  return { breit: document.documentElement.scrollWidth, fenster: window.innerWidth, geraet, zuBreit: [...zuBreit, ...ragen] };
}

/**
 * Überlappende Texte in den Zeitbausteinen ([data-zeit]): je Baustein jedes
 * Paar sichtbarer Textstücke (Elemente mit eigenem Text), das sich um mehr
 * als 1 px überdeckt, und Texte, die über den Rand ihres Bausteins oder der
 * Seite hinausragen. Ein Element, das ein anderes enthält, zählt nicht als
 * Paar; ebenso nicht Text, den der Baustein bewusst abschneidet (Ellipse).
 */
function ZEITACHSE() {
  // Die Achse gehört zu ihrem Gantt oder Dichtestreifen und wird mit ihm geprüft.
  const bausteine = [...document.querySelectorAll('[data-zeit="gantt"], [data-zeit="dichte"], [data-zeit="verlauf"]')];
  const breite = document.documentElement.clientWidth;
  let paare = 0;
  let geprueft = 0;
  let ueberRand = 0;
  const beispiele = [];
  const rand = [];
  const sichtbar = (e) => {
    const s = getComputedStyle(e);
    return s.visibility !== "hidden" && s.display !== "none" && Number(s.opacity) > 0 && !e.closest('[aria-hidden="true"][class*="unsichtbar"]');
  };
  const verborgen = (e) => {
    // Bildschirmleser-Text (1 px, clip) zählt nicht.
    const r = e.getBoundingClientRect();
    return r.width <= 1 || r.height <= 1;
  };
  for (const b of bausteine) {
    const br = b.getBoundingClientRect();
    if (br.width === 0) continue;
    const texte = [...b.querySelectorAll("*")].filter(
      (e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim() !== "") && sichtbar(e) && !verborgen(e),
    );
    // Was ein Vorfahr mit eigenem Abschneiden (overflow, Zeilenbegrenzung) verbirgt, zählt nicht.
    const sichtbarerTeil = (e, x) => {
      let l = x.left, o = x.top, rr = x.right, u = x.bottom;
      for (let p = e; p && p !== b.parentElement; p = p.parentElement) {
        const s = getComputedStyle(p);
        if (s.overflowX !== "visible" || s.overflowY !== "visible") {
          const pr = p.getBoundingClientRect();
          l = Math.max(l, pr.left); o = Math.max(o, pr.top); rr = Math.min(rr, pr.right); u = Math.min(u, pr.bottom);
        }
      }
      return rr - l > 0.5 && u - o > 0.5 ? { left: l, top: o, right: rr, bottom: u } : null;
    };
    const r = texte.map((e) => {
      // Die Fläche des Textes selbst (Range), nicht des Kastens.
      const range = document.createRange();
      range.selectNodeContents(e);
      const roh = [...range.getClientRects()].filter((x) => x.width > 0 && x.height > 0);
      const rects = roh.map((x) => sichtbarerTeil(e, x)).filter((x) => x !== null);
      const kasten = e.getBoundingClientRect();
      const abgeschnitten = getComputedStyle(e).textOverflow === "ellipsis";
      return { e, t: e.textContent.trim().slice(0, 30), rects, roh, kasten, abgeschnitten };
    });
    geprueft += r.length;
    for (let i = 0; i < r.length; i++) {
      for (let j = i + 1; j < r.length; j++) {
        if (r[i].e.contains(r[j].e) || r[j].e.contains(r[i].e)) continue;
        let deckt = false;
        for (const a of r[i].rects)
          for (const c of r[j].rects) {
            const w = Math.min(a.right, c.right) - Math.max(a.left, c.left);
            const h = Math.min(a.bottom, c.bottom) - Math.max(a.top, c.top);
            if (w > 1 && h > 1) deckt = true;
          }
        if (deckt) {
          paare++;
          if (beispiele.length < 4) beispiele.push(`${r[i].t} / ${r[j].t}`);
        }
      }
    }
    for (const x of r) {
      if (x.abgeschnitten) continue;
      // Was ein Rahmen mit eigenem Bildlauf oder Abschneiden zeigt, zählt bis zu dessen Rand.
      let grenzeL = Math.max(br.left, 0);
      let grenzeR = Math.min(br.right, breite);
      for (let p = x.e.parentElement; p && p !== b.parentElement; p = p.parentElement) {
        const s = getComputedStyle(p);
        if (s.overflowX !== "visible") {
          const pr = p.getBoundingClientRect();
          grenzeL = Math.max(grenzeL, pr.left);
          grenzeR = Math.min(grenzeR, pr.right);
        }
      }
      for (const a of x.roh) {
        if (a.right > grenzeR + 1 || a.left < grenzeL - 1) {
          ueberRand++;
          if (rand.length < 4) rand.push(`${x.t} (${Math.round(a.left)}–${Math.round(a.right)} in ${Math.round(grenzeL)}–${Math.round(grenzeR)})`);
          break;
        }
      }
    }
  }
  return { bausteine: bausteine.length, geprueft, paare, beispiele, ueberRand, rand };
}

/** Kopfleiste, Fassung 2: Live-Punkt, Schloss und ob sich im Kopf etwas überlappt. */
function KOPFLEISTE_2() {
  const kopf = document.querySelector("header");
  const live = kopf?.querySelector('.rechts a[href$="#dashboard"]') ?? null;
  const schloss = kopf?.querySelector(".rechts a[data-schloss]") ?? null;
  const masse = (e) => {
    if (!e) return null;
    const r = e.getBoundingClientRect();
    return { links: r.left, rechts: r.right, oben: r.top, unten: r.bottom, breite: r.width, hoehe: r.height };
  };
  // Alles, was im Kopf sichtbar steht (Fassung 3): Knopf der Schublade, Symbol,
  // Glieder und Blatt der Brotkrumen, PC, Glocke, Live-Punkt, Schloss.
  const teile = [
    ["Schublade", kopf?.querySelector(".schublade button")],
    ["Symbol", kopf?.querySelector('a[aria-label="Startseite"]')],
    ...[...(kopf?.querySelectorAll(".krumen .mitte a") ?? [])].map((e) => [e.textContent.trim(), e]),
    ["Blatt", kopf?.querySelector(".krumen .blatt button")],
    ["PC", kopf?.querySelector(".rechts a.pc")],
    ["Glocke", kopf?.querySelector('.rechts button[aria-haspopup="dialog"]')],
    ["Live", live],
    ["Schloss", schloss],
  ].filter(([, e]) => {
    if (!e || e.getClientRects().length === 0) return false;
    const r = e.getBoundingClientRect();
    return r.width > 1 && r.height > 1;
  });
  const ueber = [];
  for (let i = 0; i < teile.length; i++) {
    for (let j = i + 1; j < teile.length; j++) {
      const a = teile[i][1].getBoundingClientRect();
      const b = teile[j][1].getBoundingClientRect();
      const x = Math.min(a.right, b.right) - Math.max(a.left, b.left);
      const y = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
      if (x > 0.5 && y > 0.5) ueber.push(`${teile[i][0]} / ${teile[j][0]}`);
    }
  }
  const imBild = [live, schloss].every((e) => {
    if (!e) return false;
    const r = e.getBoundingClientRect();
    return r.left >= -0.5 && r.right <= window.innerWidth + 0.5;
  });
  return {
    live: live ? { name: live.getAttribute("aria-label"), text: live.textContent.trim(), href: live.getAttribute("href"), zustand: live.dataset.zustand, ...masse(live) } : null,
    schloss: schloss ? { name: schloss.getAttribute("aria-label"), stand: schloss.dataset.schloss, href: schloss.getAttribute("href"), ...masse(schloss) } : null,
    titel: masse(kopf?.querySelector(".krumen .blatt button")),
    ueber,
    imBild,
    seitlich: document.documentElement.scrollWidth - window.innerWidth,
    quellen: window.__eventQuellen ? window.__eventQuellen() : null,
  };
}

/** Kopfleiste, Fassung 3: alles im Kopf, die Leiste, die Schublade und Sperren am <body>. */
function KOPFLEISTE_3() {
  const kopf = document.querySelector("header");
  const sichtbar = (e) => {
    if (!e || e.getClientRects().length === 0) return false;
    const r = e.getBoundingClientRect();
    const s = getComputedStyle(e);
    return r.width > 1 && r.height > 1 && s.visibility !== "hidden" && s.display !== "none";
  };
  const m = (e) => {
    const r = e.getBoundingClientRect();
    return { links: r.left, rechts: r.right, oben: r.top, unten: r.bottom, breite: r.width, hoehe: r.height };
  };
  const knopf = kopf?.querySelector(".schublade button") ?? null;
  const pc = kopf?.querySelector(".rechts a.pc") ?? null;
  const glocke = kopf?.querySelector('.rechts button[aria-haspopup="dialog"]') ?? null;
  const live = kopf?.querySelector('.rechts a[href$="#dashboard"]') ?? null;
  const schloss = kopf?.querySelector(".rechts a[data-schloss]") ?? null;
  const blatt = kopf?.querySelector(".krumen .blatt button") ?? null;
  const ziele = [
    ["Schublade", knopf],
    ["Symbol", kopf?.querySelector('a[aria-label="Startseite"]')],
    ...[...(kopf?.querySelectorAll(".krumen .mitte a") ?? [])].map((e) => [e.textContent.trim(), e]),
    ["Blatt", blatt],
    ["PC", pc],
    ["Glocke", glocke],
    ["Live", live],
    ["Schloss", schloss],
  ].filter(([, e]) => sichtbar(e));
  const klein = ziele.filter(([, e]) => {
    const r = e.getBoundingClientRect();
    return r.height < 44 - 0.5 || r.width < 44 - 0.5;
  }).map(([n, e]) => `${n} ${Math.round(e.getBoundingClientRect().width)}×${Math.round(e.getBoundingClientRect().height)}`);
  const ueber = [];
  for (let i = 0; i < ziele.length; i++)
    for (let j = i + 1; j < ziele.length; j++) {
      const a = ziele[i][1].getBoundingClientRect();
      const b = ziele[j][1].getBoundingClientRect();
      if (Math.min(a.right, b.right) - Math.max(a.left, b.left) > 0.5 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 0.5)
        ueber.push(`${ziele[i][0]} / ${ziele[j][0]}`);
    }
  const rechts = [pc, glocke, live, schloss];
  const reihenfolge = rechts.every((e) => e !== null) && rechts.every((e, i) => i === 0 || rechts[i - 1].getBoundingClientRect().right <= e.getBoundingClientRect().left + 0.5);
  const krumen = [...(kopf?.querySelectorAll(".krumen li") ?? [])].filter((e) => sichtbar(e)).map((e) => {
    // Nur was man sieht: ohne den Text für Vorlese-Programme.
    const k = e.cloneNode(true);
    k.querySelectorAll(".nur-vorlesen").forEach((x) => x.remove());
    return k.textContent.replace(/\s+/g, " ").trim();
  });
  const fest = document.querySelector(".leiste-fest");
  const festNav = fest?.querySelector('nav[aria-label="Seiten"]') ?? null;
  const festLinks = festNav ? [...festNav.querySelectorAll("a")].map((a) => a.textContent.replace(/\s+/g, " ").trim()) : [];
  const schublade = document.querySelector('[role="dialog"] nav[aria-label="Seiten"]');
  const aktiv = document.activeElement;
  const s = getComputedStyle(aktiv ?? document.body);
  return {
    krumen,
    blatt: blatt ? { text: blatt.textContent.trim(), aktuell: blatt.getAttribute("aria-current"), beschreibung: document.getElementById(blatt.getAttribute("aria-describedby") ?? "")?.textContent.trim() ?? null, ...m(blatt) } : null,
    pc: pc ? { name: pc.getAttribute("aria-label"), href: pc.getAttribute("href"), belegung: pc.dataset.belegung, punkte: Number(pc.querySelector("svg")?.dataset.werte ?? 0) } : null,
    glocke: glocke ? { name: glocke.getAttribute("aria-label"), zahl: glocke.querySelector(".zahl")?.textContent.trim() ?? null, offen: glocke.getAttribute("aria-expanded") } : null,
    knopf: knopf && sichtbar(knopf) ? { name: knopf.getAttribute("aria-label"), ...m(knopf) } : null,
    klein,
    ueber,
    reihenfolge,
    seitlich: document.documentElement.scrollWidth - window.innerWidth,
    festSichtbar: sichtbar(fest),
    festLinks,
    festAktuell: festNav ? [...festNav.querySelectorAll("[aria-current]")].map((a) => a.textContent.replace(/\s+/g, " ").trim()) : [],
    schubladeOffen: schublade !== null,
    klappfeldOffen: [...document.querySelectorAll('[role="dialog"]')].some((d) => d.querySelector("h2")?.textContent.trim() === "Meldungen"),
    klappfeldText: [...document.querySelectorAll('[role="dialog"]')].find((d) => d.querySelector("h2")?.textContent.trim() === "Meldungen")?.innerText.slice(0, 200) ?? null,
    fokus: aktiv ? { name: aktiv.getAttribute("aria-label") ?? aktiv.textContent.trim().slice(0, 40), tag: aktiv.tagName.toLowerCase(), id: aktiv.id, da: aktiv !== document.body && aktiv.getClientRects().length > 0, sichtbar: (s.outlineStyle !== "none" && parseFloat(s.outlineWidth) >= 1) || (s.boxShadow !== "none" && s.boxShadow !== "") } : null,
    body: { pe: getComputedStyle(document.body).pointerEvents, of: document.body.style.overflow, ofBerechnet: getComputedStyle(document.body).overflowY },
    quellen: window.__eventQuellen ? window.__eventQuellen() : null,
    hintergrund: getComputedStyle(document.body).backgroundColor,
  };
}

/** Die Fehlerseite im Browser. */
function FEHLERSEITE() {
  const main = document.querySelector("main");
  const text = main?.innerText ?? "";
  const links = [...(main?.querySelectorAll("a") ?? [])].map((a) => ({ href: a.getAttribute("href"), text: a.textContent.trim() }));
  return {
    titel: document.title,
    h1: main?.querySelector("h1")?.textContent.trim() ?? "",
    text: text.slice(0, 400),
    sprache: document.documentElement.lang,
    links,
  };
}

/** Stufen-Knöpfe auf /anstehend. */
function STUFEN() {
  const gruppen = [...document.querySelectorAll('.stufen[role="group"]')];
  const knoepfe = gruppen.flatMap((g) => [...g.querySelectorAll("button")]);
  const name = (e) => (e.getAttribute("aria-label") ?? e.textContent ?? "").replace(/\s+/g, " ").trim();
  const gruppenName = (g) => {
    const ids = g.getAttribute("aria-labelledby");
    if (ids) return ids.split(/\s+/).map((id) => document.getElementById(id)?.textContent.trim() ?? "").join(" ").trim();
    return g.getAttribute("aria-label") ?? "";
  };
  const groessen = knoepfe.map((k) => k.getBoundingClientRect());
  return {
    gruppen: gruppen.length,
    knoepfe: knoepfe.length,
    ohneName: knoepfe.filter((k) => name(k) === "").length,
    gruppeOhneName: gruppen.filter((g) => gruppenName(g) === "").length,
    sichtbareBeschriftung: gruppen.filter((g) => {
      const ids = g.getAttribute("aria-labelledby");
      const el = ids ? document.getElementById(ids.split(/\s+/)[0]) : null;
      return el !== null && el.getClientRects().length > 0 && el.getBoundingClientRect().width > 2;
    }).length,
    kleinsteHoehe: groessen.length ? Math.min(...groessen.map((r) => r.height)) : null,
    kleinsteBreite: groessen.length ? Math.min(...groessen.map((r) => r.width)) : null,
    namen: [...new Set(knoepfe.map(name))].slice(0, 6),
    beispielGruppe: gruppen[0] ? gruppenName(gruppen[0]) : null,
  };
}

/** Texte der Seite (main, Fußzeile). */
function TEXTE() {
  return {
    main: document.querySelector("main")?.innerText ?? "",
    fuss: document.querySelector("footer")?.innerText ?? "",
    hinweisLinks: [...document.querySelectorAll("main a")].filter((a) => /localhost/.test(a.getAttribute("href") ?? "")).length,
  };
}

/** Gantt-Bausteine: Ausschnitt, Knöpfe, Größen. */
function GANTT() {
  return [...document.querySelectorAll('[data-zeit="gantt"]')].map((g, i) => {
    const knoepfe = [...g.querySelectorAll(".leiste button, .stufen button")];
    const heute = knoepfe.find((k) => /heute/i.test(k.textContent));
    const name = g.querySelector("[data-balken-name]");
    const spur = g.querySelector(".reihen .spur");
    const achse = g.querySelector('[data-zeit="achse"]');
    const gr = g.getBoundingClientRect();
    const ar = achse?.getBoundingClientRect();
    const texte = achse ? [...achse.querySelectorAll("[data-beschriftung]")] : [];
    const abgeschnitten = texte.filter((t) => {
      const r = t.getBoundingClientRect();
      return r.right > ar.right + 0.5 || r.left < ar.left - 0.5;
    }).map((t) => t.textContent);
    return {
      i,
      von: Number(g.dataset.von),
      bis: Number(g.dataset.bis),
      stufe: g.dataset.stufe,
      heuteAusserhalb: g.dataset.heuteAusserhalb === "ja",
      heuteKnopf: heute ? { hoehe: heute.getBoundingClientRect().height } : null,
      knopfHoehen: knoepfe.map((k) => Math.round(k.getBoundingClientRect().height)),
      nameBreite: name ? Math.round(name.getBoundingClientRect().width) : null,
      spurBreite: spur ? Math.round(spur.getBoundingClientRect().width) : null,
      achseRechts: ar ? Math.round(ar.right - gr.right) : null,
      abgeschnitten,
      regler: [...g.querySelectorAll('input[type="range"]')].map((r) => ({ min: r.min, max: r.max, step: r.step })),
    };
  });
}

/** Welche Zeitbausteine einen Heute-Knopf haben. */
function HEUTE_KNOEPFE() {
  return [...document.querySelectorAll('[data-zeit="gantt"], [data-zeit="dichte"], [data-zeit="verlauf"]')].map((b) => {
    const art = b.dataset.zeit;
    // Der Knopf steht im Baustein oder direkt davor (Dichtestreifen über dem Verlauf).
    const umgebung = b.closest("section, .card, article") ?? b.parentElement;
    const knopf = [...(umgebung?.querySelectorAll("button") ?? [])].find((k) => /^\s*(zu\s+)?heute\b/i.test(k.textContent));
    const eintraege = b.querySelectorAll("[data-verlauf-ziel]:not([data-heute])").length;
    const projekt = b.classList.contains("projekt");
    const heuteDrin = !(b.dataset.heuteAusserhalb === "ja");
    return { art, projekt, eintraege, knopf: Boolean(knopf), heuteDrin };
  });
}

/** Heute-Linie im Gantt sichtbar? */
function HEUTE_LINIE() {
  return [...document.querySelectorAll('[data-zeit="gantt"]')].map((g) => Boolean(g.querySelector("[data-heute-linie]")));
}

// ----- Prüfungen -----

/** Projekte aus der Kopie: Slugs mit projekt.toml und daten-Dateien. */
function projekteDerKopie() {
  const wurzel = join(ordner, "repo", "projects");
  return readdirSync(wurzel, { withFileTypes: true })
    .filter((d) => d.isDirectory() && !d.name.startsWith("_") && !d.name.startsWith("."))
    .map((d) => {
      const p = join(wurzel, d.name);
      const daten = existsSync(join(p, "daten"))
        ? readdirSync(join(p, "daten")).filter((n) => n.endsWith(".toml")).map((n) => n.slice(0, -5))
        : [];
      return {
        slug: d.name,
        projektToml: existsSync(join(p, "projekt.toml")),
        daten,
      };
    });
}

async function serverPruefen(projekte) {
  console.log("\n== Server: Fehlerseite, Kopf der neuen Form, geschützte Adressen ==");
  const holen = (pfad, kopf = {}) => fetch(`http://127.0.0.1:${PORT}${pfad}`, { headers: { accept: "text/html", ...kopf }, redirect: "manual" });
  // Etappe 6a: /projects/<slug>/neu und /neu/<name> sind für jedes Projekt
  // nur noch eine Weiterleitung (dauerhaft, ohne Vorschau).
  const mitKopf = projekte.filter((p) => p.projektToml).map((p) => p.slug);
  const nurNeu = mitKopf[0] ?? "dashboard";
  const weiter = [];
  for (const [pfad, ziel] of [
    ...mitKopf.map((s) => [`/projects/${s}/neu`, `/projects/${s}`]),
    [`/projects/${nurNeu}/neu/gibt-es-nicht`, `/projects/${nurNeu}/gibt-es-nicht`],
  ]) {
    const r = await holen(pfad);
    await r.text();
    weiter.push(`${pfad} ${r.status} → ${r.headers.get("location")}`);
    pruefe(r.status === 307 && r.headers.get("location") === ziel, `neue-form: ${pfad} leitet nach ${ziel}`, `${r.status} ${r.headers.get("location")}`);
  }
  messen("neue-form (Server)", weiter.join(", "));
  const faelle = [
    ["/gibt-es-nicht", 404],
    ["/pc/gibt-es-nicht", 404],
    ["/projects/gibt-es-nicht", 404],
    ["/projects/gibt-es-nicht/neu", 404],
    ["/projects/gibt-es-nicht/neu/x", 404],
    ["/", 200],
    ["/pc", 200],
    ["/anstehend", 200],
    ["/katalog", 200],
    // Etappe 4d: System-Seiten 200, ihre Unteradressen 404.
    ["/anleitung", 200],
    ["/zielbild", 200],
    ["/entscheidungen", 200],
    ["/entscheidungen/gibt-es-nicht", 404],
    [`/projects/${nurNeu}`, 200],
    [`/projects/${nurNeu}/gibt-es-nicht`, 404],
  ];
  const codes = [];
  for (const [pfad, soll] of faelle) {
    const r = await holen(pfad);
    const text = await r.text();
    codes.push(`${pfad} ${r.status}`);
    pruefe(r.status === soll, `fehlerseite: ${pfad} antwortet ${soll}`, `${r.status}`);
    if (soll === 404)
      pruefe(
        /<html|<!doctype/i.test(text) && !/cargo|\\src\\|src\/|panicked|rust/i.test(text),
        `fehlerseite: ${pfad} liefert die Oberfläche ohne Pfade oder Technik`,
        text.slice(0, 120),
      );
  }
  messen("fehlerseite (Server)", codes.join(", "));

  // /api/projekte/<slug> gegen /api/projekte/<slug>/daten/<name>
  const widersprueche = [];
  let ohne = 0;
  for (const p of projekte) {
    const kopf = (await fetch(`http://127.0.0.1:${PORT}/api/projekte/${p.slug}`)).status;
    if (!p.projektToml) ohne++;
    for (const name of p.daten.slice(0, 2)) {
      const d = (await fetch(`http://127.0.0.1:${PORT}/api/projekte/${p.slug}/daten/${name}`)).status;
      if ((kopf === 404) !== (d === 404)) widersprueche.push(`${p.slug}: Kopf ${kopf}, daten/${name} ${d}`);
    }
  }
  messen("api-kopf", `${ohne} Projekte ohne projekt.toml, Widersprüche: ${widersprueche.length} (${widersprueche.slice(0, 6).join("; ")})`);
  pruefe(widersprueche.length === 0, "api-kopf: Kopf und daten-Adressen passen zusammen", widersprueche.slice(0, 6).join("; "));

  // Geschützte Adressen ohne Anmeldung: weiter 401 (die Schleusen bleiben).
  const geschuetzt = [
    "/api/events",
    "/api/vorgaenge/uebersicht",
    "/api/aufgaben/anzahl",
    "/api/aufgaben?offen=1",
    "/api/anstehend",
    "/api/pc/benutzer",
    "/api/pc/verlauf/voll",
    "/api/schluessel/passkeys/anzahl",
    "/api/meldungen?projekt=dashboard",
    `/api/projekte/${nurNeu}/stand/voll`,
    // Abnahme 4d: Entscheidungen und Zielbild nur mit Anmeldung.
    "/api/entscheidungen",
    "/api/zielbild",
    // Etappe 6b: die Anleitung ebenso.
    "/api/anleitung",
  ];
  const nicht401 = [];
  for (const pfad of geschuetzt) {
    const r = await fetch(`http://127.0.0.1:${PORT}${pfad}`, { headers: { accept: "application/json" } });
    if (r.status !== 401) nicht401.push(`${pfad} ${r.status}`);
    r.body?.cancel().catch(() => {});
  }
  pruefe(nicht401.length === 0, `anmeldung: ${geschuetzt.length} geschützte Adressen geben ohne Anmeldung 401`, nicht401.join(", "));
}

/** 375 px (iPhone) und 360 px (kleine Android-Telefone, Abnahme 3b: „/“ war dort 13 px zu breit). */
async function breitenPruefen(cdp, seiten) {
  for (const px of [375, 360]) {
    console.log(`\n== Breite bei ${px} px ==`);
    await cdp.breite(px);
    const zuBreit = [];
    for (const pfad of seiten) {
      await sicher(`breite: ${pfad} (${px})`, async () => {
        await cdp.oeffnen(pfad);
        const w = await cdp.rufe(SEITLICH);
        const gut = w.breit <= w.geraet && w.fenster <= w.geraet;
        if (!gut) zuBreit.push(`${pfad} ${w.breit} px`);
        pruefe(gut, `breite: ${pfad} bei ${px} px ohne seitliches Scrollen`, gut ? "" : `${w.breit} px; ${w.zuBreit.join(", ")}`);
      });
    }
    messen(`breite ${px} px`, zuBreit.length === 0 ? `alle ${seiten.length} Seiten passen` : zuBreit.join(", "));
  }
}

async function zeitachsePruefen(cdp, seiten) {
  console.log("\n== Zeitachse: keine Überlappung, nichts über den Rand ==");
  for (const breite of [1280, 375]) {
    await cdp.breite(breite);
    for (const pfad of seiten) {
      await sicher(`zeitachse: ${pfad} ${breite}`, async () => {
        await cdp.oeffnen(pfad);
        const u = await cdp.rufe(ZEITACHSE);
        messen(`zeitachse ${pfad} ${breite} px`, `${u.paare} Paare übereinander, ${u.ueberRand} über den Rand (${u.geprueft} Texte in ${u.bausteine} Bausteinen)`);
        pruefe(u.bausteine > 0, `zeitachse: ${pfad} (${breite} px) hat Zeitbausteine`, String(u.bausteine));
        pruefe(u.paare === 0, `zeitachse: ${pfad} (${breite} px) keine Texte übereinander`, `${u.paare}: ${u.beispiele.join(" | ")}`);
        pruefe(u.ueberRand === 0, `zeitachse: ${pfad} (${breite} px) nichts über den Rand`, `${u.ueberRand}: ${u.rand.join(" | ")}`);
      });
    }
  }
}

async function fehlerseitePruefen(cdp) {
  console.log("\n== Fehlerseite im Browser ==");
  await cdp.breite(1280);
  for (const pfad of ["/gibt-es-nicht", "/projects/gibt-es-nicht"]) {
    await sicher(`fehlerseite ${pfad}`, async () => {
      await cdp.oeffnen(pfad);
      await pause(500);
      const f = await cdp.rufe(FEHLERSEITE);
      messen(`fehlerseite ${pfad}`, `Titel „${f.titel}“, h1 „${f.h1}“, Text „${f.text.replace(/\s+/g, " ").slice(0, 120)}“`);
      pruefe(f.titel.trim() !== "" && f.titel.trim() !== "Dashboard", `fehlerseite: ${pfad} hat einen eigenen Tab-Titel`, f.titel);
      pruefe(!/not found|cargo|rust|svelte|stack|\.rs\b|\.ts\b|version/i.test(f.text + f.h1), `fehlerseite: ${pfad} ohne Englisch, Pfade und Technik`, f.text.slice(0, 160));
      pruefe(/[äöüß]|nicht|gibt es|Seite/i.test(f.h1 + f.text) && f.sprache === "de", `fehlerseite: ${pfad} auf Deutsch`, `${f.sprache}: ${f.h1}`);
      pruefe(f.links.some((l) => l.href === "/" || l.href === "./" || l.href?.endsWith(":8788/")), `fehlerseite: ${pfad} mit Link zur Übersicht`, JSON.stringify(f.links));
    });
  }
}

/** Projektseite der neuen Form im Browser (Etappe 3b-2). */
function NEUE_FORM() {
  const main = document.querySelector("main");
  const inhalt = main?.querySelector(".inhalt");
  const text = main?.innerText ?? "";
  return {
    pfad: location.pathname,
    h1: main?.querySelector("h1")?.textContent.replace(/\s+/g, " ").trim() ?? "",
    inhalt: (inhalt?.innerText ?? "").replace(/\s+/g, " ").trim().slice(0, 160),
    laedt: inhalt?.querySelector('[aria-busy="true"]') != null,
    fehlerbild: inhalt?.querySelector(".fehlerbild") != null,
    standardaufbau: inhalt?.querySelector("[data-standardaufbau]") != null,
    hinweisNeu: /Neue Ansicht ansehen|Zur Hauptseite|Die neue Ansicht/.test(text),
    unterseiten: [...(main?.querySelectorAll('nav[aria-label="Seiten des Projekts"] a') ?? [])].map((a) => a.getAttribute("href")),
  };
}

async function neueFormPruefen(cdp, projekte) {
  console.log("\n== Projektseiten direkt unter /projects/<slug> (3b-2, seit 6a jedes Projekt) ==");
  await cdp.breite(1280);
  const nurNeu = projekte.filter((p) => p.projektToml).map((p) => p.slug);
  pruefe(nurNeu.length > 0, "neue-form: mindestens ein Projekt mit projekt.toml", nurNeu.join(", "));
  for (const slug of nurNeu) {
    await sicher(`neue-form ${slug}`, async () => {
      const vorher = cdp.gesperrt.length;
      const z = antwortenZaehlen(cdp);
      await cdp.oeffnen(`/projects/${slug}`);
      // Seite.svelte lädt nach (Bauliste, ladeDaten): warten, bis sie steht.
      let f = await cdp.rufe(NEUE_FORM);
      for (let i = 0; i < 20 && (f.laedt || f.inhalt === ""); i++) {
        await pause(250);
        f = await cdp.rufe(NEUE_FORM);
      }
      z.stopp();
      // Eigener Server: 127.0.0.1 oder localhost (das angemeldete Edge) auf dem Testport.
      const eigen = (url) => {
        try {
          const u = new URL(url);
          return u.protocol === "data:" || (["127.0.0.1", "localhost", "[::1]"].includes(u.hostname) && u.port === String(PORT));
        } catch {
          return false;
        }
      };
      const fremd = z.antworten.filter((a) => !eigen(a.url));
      messen(`neue-form /projects/${slug}`, `h1 „${f.h1}“, Inhalt „${f.inhalt.slice(0, 80)}“, Unterseiten ${f.unterseiten.join(" ") || "keine"}, Antworten ${z.antworten.length}, fremd ${fremd.length}`);
      pruefe(f.pfad === `/projects/${slug}`, `neue-form: /projects/${slug} bleibt dort`, f.pfad);
      pruefe(f.inhalt !== "" && !f.laedt && !f.fehlerbild && !f.standardaufbau, `neue-form: /projects/${slug} zeigt Seite.svelte direkt`, JSON.stringify(f));
      pruefe(!f.hinweisNeu, `neue-form: /projects/${slug} ohne „Neue Ansicht ansehen“ und „Zur Hauptseite“`, f.inhalt);
      pruefe(f.unterseiten.every((h) => !h.includes("/neu")), `neue-form: /projects/${slug} verlinkt Unterseiten ohne /neu`, f.unterseiten.join(" "));
      pruefe(cdp.gesperrt.length === vorher && fremd.length === 0, `neue-form: Netzsperre hält auf /projects/${slug} (nur der eigene Server)`, [...cdp.gesperrt.slice(vorher), ...fremd.map((a) => a.url)].slice(0, 4).join(" | "));
      // Die alte Adresse /neu landet auf der Hauptseite.
      await cdp.oeffnen(`/projects/${slug}/neu`);
      await pause(300);
      const n = await cdp.rufe(NEUE_FORM);
      pruefe(n.pfad === `/projects/${slug}`, `neue-form: /projects/${slug}/neu landet auf /projects/${slug}`, n.pfad);
    });
  }
}

/** Die Seite ist bedienbar: keine Sperre am <body>, sie rollt, ein Klick kommt an. */
async function bedienbar(cdp) {
  return cdp.rufe(async () => {
    const pe = getComputedStyle(document.body).pointerEvents;
    const of = document.body.style.overflow;
    const vorher = window.scrollY;
    window.scrollTo({ top: 200, behavior: "instant" });
    await new Promise((r) => setTimeout(r, 100));
    const rollt = document.documentElement.scrollHeight <= window.innerHeight + 1 || window.scrollY > 0;
    window.scrollTo({ top: vorher, behavior: "instant" });
    const x = Math.round(window.innerWidth / 2);
    const y = Math.round(window.innerHeight / 2);
    const getroffen = document.elementFromPoint(x, y);
    const schleier = getroffen?.closest('[data-dialog-overlay], [data-popover-overlay]') ?? null;
    return { pe, of, rollt, schleier: schleier !== null };
  });
}

/** Wartet, bis `f` im Browser wahr ist (höchstens `ms`). */
async function bisWahr(cdp, f, ms = 4000) {
  const ende = Date.now() + ms;
  while (Date.now() < ende) {
    if (await cdp.rufe(f).catch(() => false)) return true;
    await pause(150);
  }
  return false;
}

/**
 * Kopfleiste, Fassung 3: mit Anmeldung (`cdp`, localhost) und ohne (`frei`,
 * 127.0.0.1) bei 360, 375, 768 und 1280 px, hell und dunkel.
 * `unterseite`: eine Unterseite eines Projekts nur in neuer Form (Brotkrumen
 * mit zwei mittleren Gliedern), sonst `null`.
 */
async function kopfleiste3Pruefen(cdp, mitAnmeldung, unterseite) {
  const wer = mitAnmeldung ? "angemeldet" : "ohne Anmeldung";
  console.log(`\n== Kopfleiste, Fassung 3 (${wer}) ==`);
  for (const thema of ["light", "dark"]) {
    await cdp.thema(thema);
    for (const breite of [360, 375, 768, 1280]) {
      const t = `kopfleiste-3 (${wer}, ${thema === "light" ? "hell" : "dunkel"}, ${breite} px)`;
      await sicher(t, async () => {
        await cdp.breite(breite);
        await cdp.oeffnen("/anstehend");
        // PC-Werte kommen alle 5 s über den Kanal; der erste gleich nach dem Verbinden.
        await bisWahr(cdp, () => Number(document.querySelector("header a.pc svg")?.dataset.werte ?? 0) > 0, 7000);
        let k = await cdp.rufe(KOPFLEISTE_3);
        messen(t, `Brotkrumen ${JSON.stringify(k.krumen)}, PC „${k.pc?.name}“ ${k.pc?.punkte} Werte, Glocke „${k.glocke?.name}“, zu klein: ${k.klein.join(", ") || "nichts"}, Überlappung: ${k.ueber.join(", ") || "keine"}, seitlich ${k.seitlich} px, Leiste fest ${k.festSichtbar}`);
        pruefe(k.ueber.length === 0, `${t}: nichts überlappt`, k.ueber.join(", "));
        pruefe(k.klein.length === 0, `${t}: Ziele mindestens 44 px`, k.klein.join(", "));
        pruefe(k.seitlich <= 0, `${t}: kein seitliches Scrollen`, `${k.seitlich} px`);
        pruefe(k.reihenfolge, `${t}: rechts PC, Glocke, Live-Punkt, Schloss in dieser Reihenfolge`);
        const kanal = mitAnmeldung ? "/api/events" : "/api/events/frei";
        pruefe(JSON.stringify(k.quellen) === JSON.stringify([kanal]), `${t}: genau eine EventSource (${kanal})`, JSON.stringify(k.quellen));
        pruefe(k.blatt?.text === "Anstehend" && k.blatt.aktuell === "page" && k.krumen.length === 2, `${t}: Brotkrumen Symbol › Anstehend, das Blatt ist die aktuelle Seite`, JSON.stringify(k.krumen));
        pruefe(k.pc !== null && k.pc.href === "/pc" && /^PC: Prozessor \d+ %$/.test(k.pc.name ?? "") && k.pc.punkte > 0 && k.pc.belegung !== "unbekannt", `${t}: PC-Symbol mit Wert und Linie (Link zu /pc)`, JSON.stringify(k.pc));
        if (mitAnmeldung) {
          pruefe(/^Meldungen: (\d+ ungelesen|keine ungelesenen)$/.test(k.glocke?.name ?? ""), `${t}: Glocke mit Zahl aus „Anstehend“`, JSON.stringify(k.glocke));
        } else {
          pruefe(k.glocke?.name === "Meldungen: erst nach der Anmeldung" && k.glocke.zahl === null, `${t}: Glocke ohne Zahl, nur der Hinweis`, JSON.stringify(k.glocke));
        }
        pruefe((await cdp.rufe(() => document.querySelector(".band") === null && !/Meldungen für jetzt/.test(document.querySelector("main")?.textContent ?? ""))), `${t}: kein Band mehr über dem Inhalt`);
        pruefe(k.hintergrund !== "", `${t}: Thema ${thema}`, k.hintergrund);

        if (breite < 1024) {
          // Unter 1024 px: Knopf der Schublade, keine feste Leiste.
          pruefe(k.knopf !== null && !k.festSichtbar, `${t}: Knopf der Schublade (44 px), keine feste Leiste`, JSON.stringify(k.knopf));
          if (k.knopf === null) return;
          // Tastatur: Fokus auf den Knopf, Enter öffnet, Escape schließt, Fokus zurück.
          await cdp.rufe(() => document.querySelector("header .schublade button")?.focus());
          await cdp.taste("Enter", { vk: 13, text: "\r" });
          await bisWahr(cdp, () => document.querySelector('[role="dialog"] nav[aria-label="Seiten"]') !== null);
          k = await cdp.rufe(KOPFLEISTE_3);
          pruefe(k.schubladeOffen, `${t}: Enter öffnet die Schublade`);
          let b = await bedienbar(cdp);
          pruefe(b.pe !== "none" && b.of !== "hidden", `${t}: offene Schublade sperrt <body> nicht`, JSON.stringify(b));
          await cdp.taste("Escape", { vk: 27 });
          await pause(400);
          k = await cdp.rufe(KOPFLEISTE_3);
          b = await bedienbar(cdp);
          pruefe(!k.schubladeOffen && k.fokus?.name === "Seiten und Projekte", `${t}: Escape schließt, Fokus zurück auf den Knopf`, JSON.stringify(k.fokus));
          pruefe(b.pe !== "none" && b.of !== "hidden" && b.rollt && !b.schleier, `${t}: nach Escape bleibt die Seite bedienbar`, JSON.stringify(b));
          // Seitenwechsel aus der Schublade: zu, Seite bedienbar, Schublade öffnet wieder.
          await cdp.rufe(() => document.querySelector("header .schublade button")?.click());
          await bisWahr(cdp, () => document.querySelector('[role="dialog"] nav[aria-label="Seiten"]') !== null);
          await cdp.rufe(() => [...document.querySelectorAll('[role="dialog"] nav[aria-label="Seiten"] a')].find((a) => a.textContent.trim() === "Hallo")?.click());
          const da = await bisWahr(cdp, () => location.pathname === "/hallo" && document.querySelector('[role="dialog"]') === null, 6000);
          await pause(500);
          b = await bedienbar(cdp);
          pruefe(da, `${t}: Link in der Schublade wechselt die Seite, die Schublade ist zu`);
          pruefe(b.pe !== "none" && b.of !== "hidden" && b.rollt && !b.schleier, `${t}: nach dem Seitenwechsel bleibt die Seite bedienbar`, JSON.stringify(b));
          await cdp.rufe(() => document.querySelector("header .schublade button")?.click());
          const wieder = await bisWahr(cdp, () => document.querySelector('[role="dialog"] nav[aria-label="Seiten"]') !== null);
          pruefe(wieder, `${t}: die Schublade öffnet nach dem Wechsel wieder`);
          await cdp.taste("Escape", { vk: 27 });
          await pause(300);

          // Auflagen aus der Abnahme (einmal je Thema und Anmeldung, bei 768 px).
          if (breite === 768) {
            const offen = () => document.querySelector('[role="dialog"] nav[aria-label="Seiten"]') !== null;
            // Vergrößern: Schublade per Tastatur auf, Fenster auf 1280 px.
            await cdp.rufe(() => document.querySelector("header .schublade button")?.focus());
            await cdp.taste("Enter", { vk: 13, text: "\r" });
            pruefe(await bisWahr(cdp, offen), `${t}: Schublade offen vor dem Vergrößern`);
            await cdp.breite(1280);
            const zu = await bisWahr(cdp, () => document.querySelector('[role="dialog"]') === null, 3000);
            await pause(300);
            k = await cdp.rufe(KOPFLEISTE_3);
            b = await bedienbar(cdp);
            messen(`${t} vergrößert`, `Fokus ${JSON.stringify(k.fokus)}`);
            pruefe(zu && k.festSichtbar, `${t}: beim Vergrößern auf 1280 px schließt die Schublade, die feste Leiste steht`);
            pruefe(k.fokus?.id === "inhalt" && k.fokus.da, `${t}: danach Fokus auf dem Inhalt, nicht auf dem versteckten Knopf`, JSON.stringify(k.fokus));
            pruefe(b.pe !== "none" && b.of !== "hidden" && b.rollt && !b.schleier, `${t}: nach dem Vergrößern bleibt die Seite bedienbar`, JSON.stringify(b));
            await cdp.breite(breite);
            // Klick daneben (rechts neben der Schublade auf den Schleier): zu.
            const mitte = await cdp.rufe(() => {
              const r = document.querySelector("header .schublade button").getBoundingClientRect();
              return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
            });
            await cdp.klickBei(mitte.x, mitte.y);
            pruefe(await bisWahr(cdp, offen), `${t}: Mausklick auf den Knopf öffnet die Schublade`);
            await pause(600);
            await cdp.klickBei(breite - 12, 500);
            const daneben = await bisWahr(cdp, () => document.querySelector('[role="dialog"]') === null, 3000);
            await pause(300);
            b = await bedienbar(cdp);
            pruefe(daneben && b.pe !== "none" && !b.schleier, `${t}: ein Klick daneben schließt die Schublade`, JSON.stringify(b));
            // Doppelklick auf den Knopf: öffnet und bleibt offen.
            await cdp.klickBei(mitte.x, mitte.y, 1);
            await cdp.klickBei(mitte.x, mitte.y, 2);
            await pause(700);
            const nochOffen = await cdp.rufe(offen);
            b = await bedienbar(cdp);
            pruefe(nochOffen && b.pe !== "none" && b.of !== "hidden", `${t}: Doppelklick auf den Knopf lässt die Schublade offen, ohne Sperre`, JSON.stringify(b));
            await cdp.taste("Escape", { vk: 27 });
            await pause(300);
            pruefe(!(await cdp.rufe(offen)), `${t}: nach dem Doppelklick schließt Escape wie sonst`);
          }
        } else {
          // Ab 1024 px: die feste Leiste, ohne PC, mit Anstehend markiert; kein Knopf.
          pruefe(k.knopf === null && k.festSichtbar, `${t}: feste Leiste, kein Knopf der Schublade`);
          pruefe(/^Anstehend/.test(k.festLinks[0] ?? "") && k.festLinks.some((l) => l.startsWith("Entscheidungen")) && !k.festLinks.some((l) => /^PC\b/.test(l)), `${t}: Leiste oben Anstehend, unten Entscheidungen, kein PC`, k.festLinks.slice(0, 4).join(", "));
          pruefe(k.festAktuell.length === 1 && /^Anstehend/.test(k.festAktuell[0]), `${t}: in der Leiste ist „Anstehend“ markiert`, JSON.stringify(k.festAktuell));
        }

        // Glocke: Klappfeld ohne Seitensperre, Escape schließt.
        await cdp.rufe(() => document.querySelector('header .rechts button[aria-haspopup="dialog"]')?.focus());
        await cdp.taste("Enter", { vk: 13, text: "\r" });
        await bisWahr(cdp, () => [...document.querySelectorAll('[role="dialog"]')].some((d) => d.querySelector("h2")?.textContent.trim() === "Meldungen"));
        k = await cdp.rufe(KOPFLEISTE_3);
        let b = await bedienbar(cdp);
        pruefe(k.klappfeldOffen, `${t}: die Glocke öffnet die Liste`, k.klappfeldText ?? "");
        if (!mitAnmeldung) pruefe(/anmelden|Microsoft Edge/i.test(k.klappfeldText ?? ""), `${t}: ohne Anmeldung steht in der Liste der Hinweis zur Anmeldung`, k.klappfeldText ?? "");
        pruefe(b.pe !== "none" && b.of !== "hidden" && b.rollt, `${t}: offene Glocke sperrt die Seite nicht`, JSON.stringify(b));
        await cdp.taste("Escape", { vk: 27 });
        await pause(300);
        k = await cdp.rufe(KOPFLEISTE_3);
        pruefe(!k.klappfeldOffen && /^Meldungen/.test(k.fokus?.name ?? ""), `${t}: Escape schließt die Liste, Fokus zurück auf die Glocke`, JSON.stringify(k.fokus));

        // Das Blatt: nach oben, Stand vom Server, kein Neuladen der Browserseite.
        // Eine lange Seite: angemeldet /anleitung, ohne Anmeldung (dort steht seit
        // Etappe 6b nur der Weg über Edge) der freie Katalog.
        await cdp.oeffnen(mitAnmeldung ? "/anleitung" : "/katalog");
        const zaehler = antwortenZaehlen(cdp);
        await cdp.rufe(() => {
          window.__keinNeuladen = true;
          window.scrollTo({ top: 600, behavior: "instant" });
        });
        await pause(200);
        const unten = await cdp.rufe(() => window.scrollY);
        // Erst eine Taste, damit Edge den Fokus als Tastatur-Fokus zeigt (:focus-visible).
        await cdp.taste("Tab", { vk: 9 });
        await cdp.rufe(() => document.querySelector("header .krumen .blatt button")?.focus());
        const fokusSichtbar = (await cdp.rufe(KOPFLEISTE_3)).fokus?.sichtbar === true;
        await cdp.taste("Enter", { vk: 13, text: "\r" });
        const oben = await bisWahr(cdp, () => window.scrollY === 0, 4000);
        await pause(800);
        zaehler.stopp();
        const nach = await cdp.rufe(() => ({ marke: window.__keinNeuladen === true, y: window.scrollY, zeichen: document.querySelector("header .krumen .blatt .zeichen")?.textContent.trim() ?? "" }));
        const stand = zaehler.antworten.filter((a) => /\/api\/stand(\/voll)?$/.test(new URL(a.url).pathname));
        messen(`${t} Blatt`, `von ${unten} px nach ${nach.y} px, Stand: ${stand.map((a) => `${new URL(a.url).pathname} ${a.status}`).join(", ")}, Zeichen „${nach.zeichen}“`);
        pruefe(unten > 0 && oben && nach.y === 0, `${t}: Klick auf das Blatt scrollt nach oben`, `${unten} → ${nach.y}`);
        pruefe(nach.marke, `${t}: das Blatt lädt die Browserseite nicht neu`);
        pruefe(stand.some((a) => a.status === 200 && new URL(a.url).pathname === (mitAnmeldung ? "/api/stand/voll" : "/api/stand")), `${t}: das Blatt fragt den Stand beim Server (${mitAnmeldung ? "/api/stand/voll" : "/api/stand"})`, stand.map((a) => `${a.url} ${a.status}`).join(", "));
        pruefe(fokusSichtbar, `${t}: Fokus auf dem Blatt sichtbar`);
        k = await cdp.rufe(KOPFLEISTE_3);
        pruefe(k.blatt?.beschreibung === "Seite auf den neuesten Stand bringen und nach oben", `${t}: das Blatt hat eine Beschreibung für Vorlese-Programme`, JSON.stringify(k.blatt?.beschreibung));

        // Die Startseite holt /api/projects nur einmal (Layout und Seite teilen die Liste).
        if (breite === 1280) {
          const z = antwortenZaehlen(cdp);
          await cdp.oeffnen("/");
          z.stopp();
          const listen = z.antworten.filter((a) => new URL(a.url).pathname === "/api/projects");
          pruefe(listen.length === 1, `${t}: / holt /api/projects genau einmal`, listen.map((a) => `${a.status}`).join(", "));
        }

        // Brotkrumen einer Unterseite: am Handy „…“, sonst alle Glieder.
        if (unterseite !== null) {
          await cdp.oeffnen(unterseite.pfad);
          k = await cdp.rufe(KOPFLEISTE_3);
          const soll = breite <= 600 ? ["", "…", unterseite.blatt] : ["", "Projekte", unterseite.projekt, unterseite.blatt];
          const ist = k.krumen.map((x) => x.replace(/^›\s*/, ""));
          pruefe(JSON.stringify(ist) === JSON.stringify(soll), `${t}: Brotkrumen der Unterseite ${breite <= 600 ? "mit „…“" : "vollständig"}`, JSON.stringify(k.krumen));
          pruefe(k.ueber.length === 0 && k.seitlich <= 0, `${t}: Unterseite ohne Überlappung, ohne seitliches Scrollen`, k.ueber.join(", "));
          if (breite >= 1024) {
            pruefe(k.festAktuell.length === 1 && k.festAktuell[0] === unterseite.blatt, `${t}: in der Leiste ist die Unterseite markiert (eingerückt unter dem Projekt)`, JSON.stringify(k.festAktuell));
          }
        }
      });
    }
  }
  await cdp.thema("light");
  await cdp.breite(1280);
}

/**
 * Kopfleiste, Fassung 2: mit Anmeldung (`cdp`, localhost) und ohne (`frei`,
 * 127.0.0.1, Windows-Anmeldung abgebrochen) bei 360, 375 und 1280 px.
 */
async function kopfleiste2Pruefen(cdp, mitAnmeldung) {
  const wer = mitAnmeldung ? "angemeldet" : "ohne Anmeldung";
  console.log(`\n== Kopfleiste, Fassung 2 (${wer}) ==`);
  const soll = mitAnmeldung
    ? { zustand: "angemeldet", text: "live, angemeldet", schloss: "offen", anker: "#passkey-anlegen" }
    : { zustand: "nicht_angemeldet", text: "live, nicht angemeldet", schloss: "zu", anker: "#mit-passkey-anmelden" };
  for (const breite of [360, 375, 1280]) {
    await sicher(`kopfleiste-2 ${wer} ${breite}`, async () => {
      await cdp.breite(breite);
      await cdp.oeffnen("/anstehend");
      // Der freie Kanal braucht einen Moment (erster Versuch, dann das Wort des Servers).
      let k = await cdp.rufe(KOPFLEISTE_2);
      for (let i = 0; i < 20 && k.live?.zustand !== soll.zustand; i++) {
        await pause(250);
        k = await cdp.rufe(KOPFLEISTE_2);
      }
      const t = `kopfleiste-2 (${wer}, ${breite} px)`;
      messen(`${t} EventSource`, JSON.stringify(k.quellen));
      messen(t, `Live „${k.live?.text}“ ${Math.round(k.live?.breite ?? 0)}×${Math.round(k.live?.hoehe ?? 0)} px, Schloss ${k.schloss?.stand} ${Math.round(k.schloss?.breite ?? 0)}×${Math.round(k.schloss?.hoehe ?? 0)} px, Titel ${Math.round(k.titel?.breite ?? 0)} px, Überlappung: ${k.ueber.join(", ") || "keine"}, seitlich ${k.seitlich} px`);
      pruefe(k.live !== null && k.schloss !== null, `${t}: Live-Punkt und Schloss da`, JSON.stringify(k).slice(0, 300));
      if (k.live === null || k.schloss === null) return;
      pruefe(k.live.zustand === soll.zustand && k.live.text === soll.text, `${t}: Zustand „${soll.text}“`, `${k.live.zustand}: ${k.live.text}`);
      pruefe(k.live.href === "/pc#dashboard" && k.live.name === `Verbindung: ${soll.text}, Stand ansehen`, `${t}: Live-Punkt ist ein Link zum Stand mit Namen`, `${k.live.href} „${k.live.name}“`);
      pruefe(k.schloss.stand === soll.schloss && k.schloss.href.startsWith("/anmelden") && k.schloss.href.endsWith(soll.anker), `${t}: Schloss ${soll.schloss}, Ziel Passkey-Seite ${soll.anker}`, `${k.schloss.stand} ${k.schloss.href}`);
      pruefe(k.live.hoehe >= 44 - 0.5 && k.live.breite >= 44 - 0.5 && k.schloss.hoehe >= 44 - 0.5 && k.schloss.breite >= 44 - 0.5, `${t}: Ziele mindestens 44 px`, `Live ${k.live.breite}×${k.live.hoehe}, Schloss ${k.schloss.breite}×${k.schloss.hoehe}`);
      pruefe(k.ueber.length === 0, `${t}: nichts überlappt`, k.ueber.join(", "));
      const kanal = mitAnmeldung ? "/api/events" : "/api/events/frei";
      pruefe(JSON.stringify(k.quellen) === JSON.stringify([kanal]), `${t}: genau eine EventSource (${kanal})`, JSON.stringify(k.quellen));
      pruefe(k.imBild && k.seitlich <= 0, `${t}: im Bild, kein seitliches Scrollen`, `seitlich ${k.seitlich} px`);
      pruefe((k.titel?.breite ?? 0) >= 40, `${t}: Blatt (Ort) bleibt sichtbar`, `${Math.round(k.titel?.breite ?? 0)} px`);
      // Klick auf den Live-Punkt: /pc, Abschnitt „Dashboard“.
      await cdp.rufe(() => document.querySelector('header .rechts a[href$="#dashboard"]').click());
      await pause(1500);
      const ort = await cdp.rufe(() => ({ pfad: location.pathname, anker: location.hash, abschnitt: document.querySelector("#dashboard h2")?.textContent.trim() ?? null, bote: /Bote/.test(document.querySelector("#dashboard")?.textContent ?? "") }));
      pruefe(ort.pfad === "/pc" && ort.anker === "#dashboard" && ort.abschnitt === "Dashboard" && ort.bote, `${t}: Live-Punkt führt zum Abschnitt „Dashboard“ auf /pc`, JSON.stringify(ort));
      // Klick auf das Schloss: Passkey-Seite im passenden Zustand.
      await cdp.rufe(() => document.querySelector("header .rechts a[data-schloss]").click());
      await pause(1500);
      const ziel = await cdp.rufe(() => ({ pfad: location.pathname, anker: location.hash, suche: location.search, abschnitt: location.hash ? document.getElementById(location.hash.slice(1))?.querySelector("h2")?.textContent.trim() ?? null : null }));
      pruefe(ziel.pfad === "/anmelden" && ziel.anker === soll.anker && ziel.suche.includes("ziel=%2Fpc"), `${t}: Schloss führt zur Passkey-Seite (${soll.anker}), zurück nach /pc`, JSON.stringify(ziel));
      if (breite === 1280) {
        const teile = await cdp.rufe(() => ({
          antrag: document.getElementById("passkey-anlegen") !== null,
          anmelden: document.getElementById("mit-passkey-anmelden") !== null,
          liste: /Passkeys für andere Windows-Konten/.test(document.querySelector("main")?.textContent ?? ""),
        }));
        messen(`${t} /anmelden`, JSON.stringify(teile));
        pruefe(teile.antrag && teile.anmelden, `${t}: /anmelden bietet Anmelden und Antrag (live)`, JSON.stringify(teile));
        pruefe(teile.liste === mitAnmeldung, `${t}: Liste der Passkeys nur angemeldet`, JSON.stringify(teile));
      }
    });
  }
  await cdp.breite(1280);
}

async function anstehendPruefen(cdp) {
  console.log("\n== /anstehend und die Fragekästen ==");
  await cdp.breite(375);
  await cdp.oeffnen("/anstehend");
  const s = await cdp.rufe(STUFEN);
  messen("anstehend Stufen", `${s.gruppen} Gruppen, ${s.knoepfe} Knöpfe, ohne Namen ${s.ohneName}, Gruppe ohne Namen ${s.gruppeOhneName}, sichtbare Beschriftung ${s.sichtbareBeschriftung}, kleinster Knopf ${s.kleinsteBreite?.toFixed(0)}×${s.kleinsteHoehe?.toFixed(0)} px, Namen ${s.namen.join(" / ")}, Gruppe „${s.beispielGruppe}“`);
  pruefe(s.gruppen > 0, "anstehend: es gibt Einträge mit Stufen (Fragen der Projekte)", String(s.gruppen));
  pruefe(s.ohneName === 0 && s.gruppeOhneName === 0, "anstehend: jeder Stufen-Knopf und jede Gruppe hat einen Namen");
  pruefe(s.sichtbareBeschriftung === s.gruppen, "anstehend: jede Stufen-Gruppe hat eine sichtbare Beschriftung", `${s.sichtbareBeschriftung} von ${s.gruppen}`);
  pruefe(s.kleinsteHoehe >= 44 - 0.5 && s.kleinsteBreite >= 44 - 0.5, "anstehend: Stufen-Knöpfe mindestens 44 × 44 px", `${s.kleinsteBreite?.toFixed(1)} × ${s.kleinsteHoehe?.toFixed(1)}`);
  const t = await cdp.rufe(TEXTE);
  pruefe(!/Vorschlag des Chats\)/.test(t.main), "anstehend: kein „(Vorschlag des Chats)“ ohne Satz", "");
}

async function nichtsPruefen(cdp) {
  console.log("\n== Startseite: „nichts zu tun“ nie neben Fragen ==");
  await cdp.breite(1280);
  await cdp.oeffnen("/");
  const r = await cdp.rufe(() => {
    const text = document.querySelector("main")?.innerText ?? "";
    const fragen = document.querySelectorAll("section").length;
    return {
      nichts: /nichts für dich zu tun/i.test(text),
      jetztFuerDich: /Jetzt für dich/.test(text),
      fragen,
    };
  });
  messen("nichts", `„nichts für dich zu tun“: ${r.nichts ? "ja" : "nein"}, „Jetzt für dich“: ${r.jetztFuerDich ? "ja" : "nein"}`);
  pruefe(!(r.nichts && r.jetztFuerDich), "nichts: „nichts für dich zu tun“ steht nie über „Jetzt für dich“");
}

async function bedienungPruefen(cdp, seitenMitZeit) {
  console.log("\n== Gantt und Zeitstrahl: Bedienung ==");
  await cdp.breite(1280);
  const ohneHeute = [];
  let ganttGesehen = 0;
  for (const pfad of seitenMitZeit) {
    await sicher(`bedienung ${pfad}`, async () => {
      await cdp.oeffnen(pfad);
      const h = await cdp.rufe(HEUTE_KNOEPFE);
      for (const b of h) {
        const braucht = b.art === "gantt" ? b.heuteDrin : b.art === "dichte" || (b.art === "verlauf" && b.projekt && b.eintraege >= 6);
        if (braucht && !b.knopf) ohneHeute.push(`${pfad} ${b.art}`);
      }
      const g = await cdp.rufe(GANTT);
      for (const x of g) {
        ganttGesehen++;
        pruefe(x.regler.every((r) => (Number(r.max) - Number(r.min)) / Number(r.step || 1) <= 400), `bedienung: ${pfad} Gantt ${x.i}: Regler mit sinnvollem Schritt`, JSON.stringify(x.regler));
        if (x.heuteAusserhalb || x.heuteKnopf === null) continue;
        // Heute im Grundzustand (Alles): der Ausschnitt ändert sich und enthält heute.
        const vorher = `${x.von}-${x.bis}`;
        await cdp.rufe((i) => {
          const g = document.querySelectorAll('[data-zeit="gantt"]')[i];
          [...g.querySelectorAll("button")].find((k) => /heute/i.test(k.textContent))?.click();
        }, x.i);
        await pause(400);
        const nach = (await cdp.rufe(GANTT))[x.i];
        const heuteTag = await cdp.rufe(() => Math.floor((Date.now() - new Date().getTimezoneOffset() * 60000) / 86400000));
        pruefe(`${nach.von}-${nach.bis}` !== vorher && nach.von <= heuteTag + 1 && nach.bis >= heuteTag, `bedienung: ${pfad} Gantt ${x.i}: „Heute“ wirkt im Grundzustand`, `${vorher} → ${nach.von}-${nach.bis} (heute ${heuteTag})`);
        // ＋ zoomt um heute: heute bleibt im Bild.
        await cdp.rufe((i) => {
          const b = document.querySelectorAll('[data-zeit="gantt"]')[i].querySelector("[data-zeit-buehne]");
          b.focus();
        }, x.i);
        for (let n = 0; n < 3; n++) await cdp.taste("+", { code: "Equal", text: "+" });
        const zoom = (await cdp.rufe(GANTT))[x.i];
        pruefe(zoom.von <= heuteTag + 1 && zoom.bis >= heuteTag, `bedienung: ${pfad} Gantt ${x.i}: ＋ zoomt um heute, heute bleibt im Bild`, `${zoom.stufe} ${zoom.von}-${zoom.bis}`);
      }
    });
  }
  messen("bedienung heute", ohneHeute.length === 0 ? "jeder Zeitbaustein hat „Heute“" : `ohne „Heute“: ${ohneHeute.join(", ")}`);
  pruefe(ohneHeute.length === 0, "bedienung: jeder Zeitbaustein mit Ausschnitt oder langer Liste hat „Heute“", ohneHeute.join(", "));
  pruefe(ganttGesehen > 0, "bedienung: mindestens ein Gantt geprüft", String(ganttGesehen));
}

/** Zoom über einen Reiterwechsel: auf Seiten mit Gantt in einem Reiter. */
async function reiterPruefen(cdp, kandidaten) {
  console.log("\n== Zoom übersteht den Reiterwechsel ==");
  await cdp.breite(1280);
  let geprueft = false;
  for (const pfad of kandidaten) {
    if (geprueft) break;
    await sicher(`reiter ${pfad}`, async () => {
      await cdp.oeffnen(pfad);
      const lage = await cdp.rufe(() => {
        for (const liste of document.querySelectorAll('[role="tablist"]')) {
          const reiter = [...liste.querySelectorAll('[role="tab"]')];
          for (let i = 0; i < reiter.length; i++) {
            reiter[i].click();
          }
        }
        return document.querySelectorAll('[role="tablist"]').length;
      });
      if (lage === 0) return;
      // Den Reiter mit einem Gantt suchen.
      const fund = await cdp.rufe(async () => {
        const warte = () => new Promise((r) => setTimeout(r, 250));
        for (const [li, liste] of [...document.querySelectorAll('[role="tablist"]')].entries()) {
          const reiter = [...liste.querySelectorAll('[role="tab"]')];
          for (let i = 0; i < reiter.length; i++) {
            reiter[i].click();
            await warte();
            const panel = liste.closest(".reiter")?.querySelector('[role="tabpanel"]:not([hidden])');
            if (panel?.querySelector('[data-zeit="gantt"]') && reiter.length > 1) return { li, i, anzahl: reiter.length };
          }
        }
        return null;
      });
      if (fund === null) return;
      geprueft = true;
      const zustand = () =>
        cdp.rufe((li) => {
          const liste = document.querySelectorAll('[role="tablist"]')[li];
          const g = liste.closest(".reiter")?.querySelector('[role="tabpanel"]:not([hidden]) [data-zeit="gantt"]');
          return g ? `${g.dataset.stufe} ${g.dataset.von}-${g.dataset.bis}` : null;
        }, fund.li);
      await cdp.rufe((li) => {
        const liste = document.querySelectorAll('[role="tablist"]')[li];
        const g = liste.closest(".reiter").querySelector('[role="tabpanel"]:not([hidden]) [data-zeit="gantt"]');
        g.querySelector("[data-zeit-buehne]").focus();
      }, fund.li);
      await cdp.taste("+", { code: "Equal", text: "+" });
      await cdp.taste("+", { code: "Equal", text: "+" });
      const vorher = await zustand();
      await cdp.rufe(async (f) => {
        const reiter = [...document.querySelectorAll('[role="tablist"]')[f.li].querySelectorAll('[role="tab"]')];
        reiter[(f.i + 1) % f.anzahl].click();
        await new Promise((r) => setTimeout(r, 300));
        reiter[f.i].click();
        await new Promise((r) => setTimeout(r, 300));
      }, fund);
      const nachher = await zustand();
      messen("bedienung reiter", `${pfad}: vor dem Wechsel ${vorher}, danach ${nachher}`);
      pruefe(vorher !== null && vorher === nachher && !vorher.startsWith("alles"), `bedienung: ${pfad} Zoom übersteht den Reiterwechsel`, `${vorher} → ${nachher}`);
    });
  }
  if (!geprueft) hinweise.push("Kein Gantt in einem Reiter gefunden; Reiterwechsel nicht geprüft.");
}

async function konfettiPruefen(cdp) {
  console.log("\n== Konfetti räumt auf ==");
  await cdp.breite(1280);
  await cdp.oeffnen("/pc");
  await cdp.senden("Page.navigate", { url: `${BASIS}/schluessel?gespeichert=probe-schluessel` });
  await pause(800);
  const waehrend = await cdp.rufe(() => document.querySelectorAll(".confetti span").length);
  await pause(4000);
  const danach = await cdp.rufe(() => document.querySelectorAll(".confetti, .confetti span").length);
  messen("bedienung konfetti", `${waehrend} Stücke während, ${danach} nach 4,8 s`);
  pruefe(danach === 0, "bedienung: Konfetti ist nach der Animation aus dem Dokument", `${waehrend} → ${danach}`);
  // /pc rieselt nur auf Anlass; danach bleibt nichts liegen.
  await cdp.oeffnen("/pc");
  await pause(3800);
  const pc = await cdp.rufe(() => document.querySelectorAll(".confetti span").length);
  pruefe(pc === 0, "bedienung: /pc lässt kein Konfetti im Dokument", String(pc));
}

async function handyPruefen(cdp, seitenMitGantt) {
  console.log("\n== Gantt am Handy (375 px) ==");
  await cdp.breite(375);
  for (const pfad of seitenMitGantt) {
    await sicher(`handy ${pfad}`, async () => {
      await cdp.oeffnen(pfad);
      const g = await cdp.rufe(GANTT);
      for (const x of g) {
        messen(`handy ${pfad} Gantt ${x.i}`, `Name ${x.nameBreite} px, Fläche ${x.spurBreite} px, Knöpfe ${x.knopfHoehen.join("/")} px, Achse rechts ${x.achseRechts} px, abgeschnitten: ${x.abgeschnitten.join(", ") || "nichts"}`);
        pruefe(x.spurBreite >= 250, `handy: ${pfad} Gantt ${x.i} Zeichenfläche breit genug`, `${x.spurBreite} px`);
        pruefe(x.knopfHoehen.every((h) => h >= 44), `handy: ${pfad} Gantt ${x.i} Knöpfe mindestens 44 px`, x.knopfHoehen.join(", "));
        pruefe(x.achseRechts !== null && x.achseRechts <= 0, `handy: ${pfad} Gantt ${x.i} Achse im Rahmen`, `${x.achseRechts} px`);
        pruefe(x.abgeschnitten.length === 0, `handy: ${pfad} Gantt ${x.i} keine abgeschnittene Jahreszahl`, x.abgeschnitten.join(", "));
      }
    });
  }
}

/** Eine Projektseite der Kopie; gesetzt in main. */
let projektPfad = "/projects/dashboard";

async function freierWegPruefen(frei) {
  console.log("\n== 127.0.0.1 ohne Anmeldung ==");
  await frei.breite(1280);
  const zahlen = [];
  for (const pfad of ["/", "/pc", "/anstehend", projektPfad]) {
    await sicher(`anmeldung ${pfad}`, async () => {
      await frei.oeffnen("/hallo"); // etwas anderes zuerst, damit das Laden voll zählt
      const z = antwortenZaehlen(frei);
      const k = konsoleMitschreiben(frei, { ohne401: true });
      await frei.oeffnen(pfad);
      await pause(1500);
      z.stopp();
      k.stopp();
      const vier = z.antworten.filter((a) => a.status === 401);
      zahlen.push(`${pfad} ${vier.length}`);
      pruefe(vier.length <= 1, `anmeldung: ${pfad} höchstens ein 401 je Laden`, `${vier.length}: ${vier.map((a) => new URL(a.url).pathname).join(", ")}`);
      pruefe(k.fehler.length === 0, `anmeldung: ${pfad} keine anderen Fehler in der Konsole`, k.fehler.join(" | "));
    });
  }
  messen("anmeldung 401 je Laden", zahlen.join(", "));
  await sicher("anstehend Edge-Hinweis", async () => {
    await frei.oeffnen("/anstehend");
    const t = await frei.rufe(TEXTE);
    const mal = (t.main.match(/Microsoft Edge/g) ?? []).length;
    messen("anstehend Edge-Hinweis", `${mal}× im Text, ${t.hinweisLinks} Links auf localhost`);
    pruefe(mal === 1, "anstehend: der Edge-Hinweis steht genau einmal", String(mal));
    pruefe(t.hinweisLinks >= 1, "anstehend: der Edge-Hinweis hat einen Link", String(t.hinweisLinks));
  });
  // Abnahme 4d: /entscheidungen und /zielbild ohne Anmeldung zeigen den Weg
  // über Edge (wie /anstehend), nie „lässt sich gerade nicht laden“; seit 6b
  // auch /anleitung (ohne Anmeldung kein Abschnitt).
  for (const pfad of ["/entscheidungen", "/zielbild", "/anleitung"]) {
    await sicher(`system-seite ${pfad} ohne Anmeldung`, async () => {
      await frei.oeffnen(pfad);
      await pause(800);
      const t = await frei.rufe(TEXTE);
      const mal = (t.main.match(/Microsoft Edge/g) ?? []).length;
      messen(`system-seite ${pfad} ohne Anmeldung`, `${mal}× Edge-Hinweis, ${t.hinweisLinks} Links auf localhost`);
      pruefe(mal === 1 && t.hinweisLinks >= 1, `anmeldung: ${pfad} zeigt ohne Anmeldung den Edge-Hinweis mit Link`, t.main.replace(/\s+/g, " ").slice(0, 160));
      pruefe(!/nicht laden/.test(t.main), `anmeldung: ${pfad} ohne Anmeldung nicht „lässt sich nicht laden“`, "");
      if (pfad === "/anleitung") {
        const h2 = await frei.rufe(() => document.querySelectorAll("main h2").length);
        pruefe(h2 === 0, "anmeldung: /anleitung zeigt ohne Anmeldung keinen Abschnitt", String(h2));
      }
    });
  }
  console.log(`  (Anmelde-Rückfragen des Browsers abgebrochen: ${frei.anmeldeRueckfragen})`);
}

/**
 * Testdaten für /zielbild: ein Abschnitt mit allen vier Blöcken, eine breite
 * Tabelle und ein Beispiel mit langer Zeile; dazu ein kaputter Block.
 */
const ZIELBILD_PROBE = [
  'titel = "Blöcke der Probe"',
  "reihenfolge = 1",
  'text = "Text vor den Blöcken."',
  "",
  "[[block]]",
  'titel = "Etappen"',
  "[block.tabelle]",
  'spalten = ["Etappe", "Was", "Warum", "Wer", "Wann"]',
  "zeilen = [",
  '  ["1a", "Das Zielbild in TOML-Dateien lesen", "Markdown raus, mit Vorrang", "Architektur", "November"],',
  '  ["1b", "Tabellen und Beispiele als Blöcke", "Ein Format für beide System-Seiten", "Dashboard", "November"],',
  "]",
  "",
  "[[block]]",
  'titel = "Aufruf"',
  "beispiel = '''",
  "pwsh -NoProfile -File %USERPROFILE%\\dev\\dashboard\\scripts\\Api.ps1 \"api/vorgaenge?fuer=dashboard&offen=1&noch-etwas-langes=ja\"",
  "  zweite Zeile, eingerückt'''",
  "",
  "[[block]]",
  'punkte = ["Eins", "Zwei"]',
  "",
  "[[block]]",
  'titel = "Ohne Inhalt"',
  "",
].join("\n");

/**
 * Bei 360 px auf /zielbild: Tabelle und Beispiel rollen je im eigenen
 * Rahmen (breiter Inhalt, overflow-x auto), die Seite nicht; der kaputte
 * Block steht als Hinweis da.
 */
function ZIELBILD_RAHMEN() {
  const rahmen = (el) => {
    if (!el) return null;
    const s = getComputedStyle(el);
    return { rollt: el.scrollWidth > el.clientWidth + 1, overflow: s.overflowX, rechts: Math.round(el.getBoundingClientRect().right) };
  };
  const tabelle = document.querySelector("main table")?.closest(".scroll-x") ?? null;
  return {
    tabelle: rahmen(tabelle),
    beispiel: rahmen(document.querySelector("main pre.beispiel")),
    hinweis: [...document.querySelectorAll("main [role=note]")].some((n) => /^Block 4: /.test(n.textContent.trim())),
    seite: document.documentElement.scrollWidth,
    geraet: window.innerWidth,
  };
}

/** Abnahme 4d: angemeldet laden /entscheidungen und /zielbild ihren Inhalt. */
async function systemSeitenAngemeldet(cdp) {
  console.log("\n== /zielbild: Blöcke bei 360 px ==");
  await cdp.breite(360);
  await sicher("system-seite /zielbild Blöcke 360", async () => {
    await cdp.oeffnen("/zielbild");
    await pause(800);
    const r = await cdp.rufe(ZIELBILD_RAHMEN);
    messen("zielbild-bloecke 360 px", JSON.stringify(r));
    for (const [name, x] of [["Tabelle", r.tabelle], ["Beispiel", r.beispiel]]) {
      pruefe(x !== null && x.rollt && x.overflow === "auto" && x.rechts <= r.geraet, `breite: /zielbild ${name} rollt bei 360 px im eigenen Rahmen`, JSON.stringify(x));
    }
    pruefe(r.seite <= r.geraet, "breite: /zielbild mit Blöcken bei 360 px ohne seitliches Scrollen", `${r.seite} px`);
    pruefe(r.hinweis, "zielbild: der kaputte Block steht als Hinweis „Block 4: …“ da", "");
  });
  // Etappe 6b: /anleitung mit den echten Abschnitten aus docs/anleitung.
  await sicher("system-seite /anleitung 360", async () => {
    await cdp.oeffnen("/anleitung");
    await pause(1200);
    const r = await cdp.rufe(() => {
      const rahmen = [...document.querySelectorAll("main pre.beispiel, main .scroll-x")];
      const erste = document.querySelector("main section h2");
      return {
        erster: erste?.textContent.trim() ?? "",
        schritte: [...(erste?.closest("section")?.querySelectorAll("h3") ?? [])].filter((h) => /^[0-9][.] /.test(h.textContent.trim())).length,
        abschnitte: document.querySelectorAll("main h2").length,
        tabellen: document.querySelectorAll("main table").length,
        beispiele: document.querySelectorAll("main pre.beispiel").length,
        hinweise: [...document.querySelectorAll("main [role=note]")].map((n) => n.textContent.trim()).filter((t) => /^Block \d+:|nicht laden|nicht lesen|Microsoft Edge/.test(t)),
        rahmenDraussen: rahmen.filter((el) => Math.round(el.getBoundingClientRect().right) > window.innerWidth).length,
        rollen: rahmen.filter((el) => el.scrollWidth > el.clientWidth + 1).length,
        seite: document.documentElement.scrollWidth,
        geraet: window.innerWidth,
      };
    });
    messen("anleitung 360 px", JSON.stringify(r));
    pruefe(r.abschnitte >= 30 && r.tabellen >= 20 && r.beispiele >= 50, "anleitung: angemeldet alle Abschnitte mit Tabellen und Beispielen", JSON.stringify(r));
    pruefe(r.erster === "Die wichtigsten Schritte für einen Chat" && r.schritte === 8, "anleitung: zuerst die acht Schritte für einen Chat", `${r.erster}, ${r.schritte} Schritte`);
    pruefe(r.hinweise.length === 0, "anleitung: kein Hinweis zu einem Block, kein Fehler, kein Edge-Hinweis", r.hinweise.slice(0, 3).join(" | "));
    pruefe(r.seite <= r.geraet && r.rahmenDraussen === 0, "breite: /anleitung bei 360 px ohne Überbreite (Tabellen und Beispiele rollen im eigenen Rahmen)", `${r.seite} px, ${r.rahmenDraussen} Rahmen draußen, ${r.rollen} rollen`);
  });
  console.log("\n== /entscheidungen und /zielbild angemeldet ==");
  await cdp.breite(1280);
  for (const [pfad, soll] of [["/entscheidungen", /Entscheidungen durchsuchen|von \d+/], ["/zielbild", /Zielbild zieht gerade|Zielbild/]]) {
    await sicher(`system-seite ${pfad} angemeldet`, async () => {
      await cdp.oeffnen(pfad);
      await pause(800);
      const t = await cdp.rufe(TEXTE);
      const suche = await cdp.rufe(() => document.querySelector('input[type="search"]') !== null);
      messen(`system-seite ${pfad} angemeldet`, t.main.replace(/\s+/g, " ").slice(0, 120));
      pruefe(!/Microsoft Edge unter/.test(t.main) && !/nicht laden/.test(t.main), `anmeldung: ${pfad} lädt angemeldet ohne Edge-Hinweis`, t.main.replace(/\s+/g, " ").slice(0, 160));
      if (pfad === "/entscheidungen") pruefe(suche, "anmeldung: /entscheidungen zeigt angemeldet die Liste mit Suche", "");
      else pruefe(soll.test(t.main), "anmeldung: /zielbild zeigt angemeldet den Inhalt", "");
    });
  }
}

async function textePruefen(cdp, neuSeite) {
  console.log("\n== Du statt Sie, Fachsprache ==");
  await cdp.breite(1280);
  await sicher("du", async () => {
    await cdp.oeffnen("/schluessel");
    const t = await cdp.rufe(TEXTE);
    const sie = t.main.match(/(?<![.!?:]\s)\b(Sie|Ihnen|Ihre[mnrs]?|Ihr)\b/g) ?? [];
    messen("du /schluessel", `${sie.length}× Sie/Ihnen/Ihr`);
    pruefe(sie.length === 0, "du: /schluessel spricht mit „du“", sie.join(", "));
  });
  await sicher("fachsprache", async () => {
    await cdp.oeffnen(neuSeite);
    const t = await cdp.rufe(TEXTE);
    const technik = /projekt\.toml|Seite\.svelte|page\.md|daten\/|\.toml|Endpunkt|SvelteKit|Rust/;
    messen("fachsprache", `Fußzeile auf ${neuSeite} „${t.fuss.replace(/\s+/g, " ")}“`);
    pruefe(!technik.test(t.fuss), "fachsprache: Fußzeile ohne Technik", t.fuss);
  });
}

// ----- Ablauf -----

async function portePruefen() {
  // Etappe 4b: gemeinsam mit allen Proben (edge-gemeinsam.mjs).
  await portePruefenGemeinsam(PORT);
}

function datenKopieren() {
  const liste = execFileSync("git", ["-C", INHALT.wurzel, "ls-files", "-z", "--", "data"], { encoding: "utf8", windowsHide: true })
    .split("\0")
    .filter((d) => d !== "");
  if (liste.length === 0) throw new Error("git ls-files data: keine eingecheckten Dateien");
  for (const datei of liste) {
    const quelle = join(INHALT.wurzel, ...datei.split("/"));
    if (!existsSync(quelle)) continue;
    const ziel = join(ordner, "repo", ...datei.split("/"));
    mkdirSync(dirname(ziel), { recursive: true });
    copyFileSync(quelle, ziel);
  }
  console.log(`data\\: ${liste.length} eingecheckte Dateien kopiert`);
}

/** Eine Unterseite eines Projekts nur in neuer Form (für die Brotkrumen), über die freie API. */
async function unterseiteFinden() {
  try {
    const liste = await (await fetch(`${BASIS_FREI}/api/projects`)).json();
    for (const p of liste.projects) {
      const k = await (await fetch(`${BASIS_FREI}/api/projekte/${p.slug}`)).json();
      const s = [...(k.kopf?.seiten ?? [])].sort((a, b) => a.order - b.order)[0];
      if (!k.vorschau && s) return { pfad: `/projects/${p.slug}/${s.name}`, projekt: k.kopf.title, blatt: s.title };
    }
  } catch (e) {
    hinweise.push(`Keine Unterseite für die Brotkrumen gefunden: ${e}`);
  }
  return null;
}

async function main() {
  if (!existsSync(EXE)) throw new Error(`Programm fehlt: ${EXE}`);
  if (!existsSync(join(WEB, "index.html"))) throw new Error(`Oberfläche fehlt: ${WEB}`);
  await portePruefen();
  mkdirSync(join(ordner, "bin"), { recursive: true });
  mkdirSync(join(ordner, "profil"), { recursive: true });
  copyFileSync(EXE, join(ordner, "bin", "dashboard.exe"));
  cpSync(INHALT.projects, join(ordner, "repo", "projects"), { recursive: true });
  // Etappe 4d: die Entscheidungen (nur gelesen) für /entscheidungen. Statt
  // des echten docs\zielbild eigene Testdaten mit breiter Tabelle und
  // breitem Beispiel (Blöcke der System-Seiten, Breite bei 360 px).
  if (existsSync(INHALT.entscheidungen))
    cpSync(INHALT.entscheidungen, join(ordner, "repo", "docs", "entscheidungen"), { recursive: true });
  mkdirSync(join(ordner, "repo", "docs", "zielbild"), { recursive: true });
  writeFileSync(join(ordner, "repo", "docs", "zielbild", "a-bloecke.toml"), ZIELBILD_PROBE);
  // Etappe 6b: die echte Anleitung aus diesem Repo (docs/anleitung) für /anleitung.
  cpSync(join(REPO, "docs", "anleitung"), join(ordner, "repo", "docs", "anleitung"), { recursive: true });
  datenKopieren();
  console.log(`Temp-Ordner: ${ordner}`);

  const projekte = projekteDerKopie();
  const neu = projekte.filter((p) => p.projektToml);
  const mitZeitNeu = neu.filter((p) => p.daten.some((d) => /gantt|zeitstrahl/.test(d))).map((p) => `/projects/${p.slug}`);
  const mitGanttNeu = neu.filter((p) => p.daten.some((d) => /gantt/.test(d))).map((p) => `/projects/${p.slug}`);
  // Die erste Projektseite mit Zeitdaten.
  const neuSeite = mitZeitNeu[0];
  if (neu[0]) projektPfad = `/projects/${neu[0].slug}`;

  await serverStarten(eigeneSid());
  await serverPruefen(projekte);

  const cdp = await edgeStarten();
  await cdp.thema("light");
  const k = konsoleMitschreiben(cdp);

  await breitenPruefen(cdp, ["/", "/pc", "/anstehend", "/schluessel", "/hallo", "/vorgaenge", "/daten", "/katalog", "/anleitung", "/zielbild", "/entscheidungen", ...new Set(["/projects/dashboard", ...neu.map((p) => `/projects/${p.slug}`)])]);
  await zeitachsePruefen(cdp, ["/pc", "/", neuSeite, "/projects/dashboard"]);
  await nichtsPruefen(cdp);
  await fehlerseitePruefen(cdp);
  await neueFormPruefen(cdp, projekte);
  const unterseite = await unterseiteFinden();
  await kopfleiste2Pruefen(cdp, true);
  await kopfleiste3Pruefen(cdp, true, unterseite);
  await anstehendPruefen(cdp);
  await systemSeitenAngemeldet(cdp);
  await bedienungPruefen(cdp, ["/pc", "/", ...mitZeitNeu]);
  await reiterPruefen(cdp, mitGanttNeu);
  await konfettiPruefen(cdp);
  await handyPruefen(cdp, mitGanttNeu);
  await textePruefen(cdp, neuSeite);
  k.stopp();
  const eigene = k.fehler.filter((f) => !/status of 404/.test(f));
  pruefe(eigene.length === 0, "Konsole: keine Fehler auf den Seiten mit Anmeldung", eigene.slice(0, 4).join(" | "));

  let frei = null;
  await sicher("freier Weg", async () => {
    frei = await edgeStarten({ port: DEBUG_PORT_FREI, profilName: "edge-profil-frei", anmeldungFuer: "nicht-dieser-pc.invalid", frei: true });
    await freierWegPruefen(frei);
    await kopfleiste2Pruefen(frei, false);
    await kopfleiste3Pruefen(frei, false, null);
  });

  const gesperrt = [...cdp.gesperrt, ...(frei?.gesperrt ?? [])];
  console.log(`\n${cdp.browserSperre.bericht()}`);
  pruefe(gesperrt.length === 0, "Netzsperre: die Seiten fragen nur beim eigenen Server an", gesperrt.slice(0, 5).join(" | "));

  console.log("\nMesswerte:");
  for (const m of messwerte) console.log(`  ${m}`);
  console.log(`\nBestanden: ${bestanden} Prüfungen. Nicht bestanden: ${fehlgeschlagen.length}.`);
  for (const f of fehlgeschlagen) console.log(`  - ${f}`);
  if (hinweise.length > 0) console.log(`Hinweise:\n${hinweise.map((h) => `  ${h}`).join("\n")}`);
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
