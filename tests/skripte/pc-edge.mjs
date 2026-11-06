#!/usr/bin/env node
/**
 * Ende-zu-Ende-Probe für Etappe 3a-4 (Urteile nach der Grenze) mit Edge.
 * Gleicher Weg wie tests/skripte/katalog-edge.mjs und zeitachse-edge.mjs:
 * Wegwerf-Daten in einem Temp-Ordner (Kopien von projects\ und data\),
 * Port 8788 (nie 8787, nie C:\ProgramData\Dashboard), Edge headless mit
 * Wegwerf-Profil, ohne Hintergrund-Netz und Komponenten-Updates, Anmeldung
 * nur für localhost und 127.0.0.1, alle Kindprozesse ohne Fenster,
 * DevTools-Protokoll über den WebSocket von Node 24 (Senden mit
 * Zeitablauf). Kein Playwright, kein Browser-Download, keine Abhängigkeit.
 *
 * Netzsperre: Fetch.enable für alle Adressen auf der Seite, dazu auf
 * Browser-Ebene (edge-gemeinsam.mjs, Target.setAutoAttach mit flatten) für
 * Service-Worker, Worker, weitere Seiten und Frames; was nicht 127.0.0.1
 * oder localhost ist, wird abgebrochen und lässt die Probe scheitern.
 * Anfragen des Browser-Prozesses selbst schalten die Startschalter ab.
 *
 * Vor dem Start (Auflagen aus 3a-4): Ist Port 8788 (oder ein Debug-Port)
 * belegt, bricht die Probe mit klarem Satz ab. Aus data\ kommen nur die
 * eingecheckten Dateien (git ls-files data) in den Temp-Ordner, nie etwas
 * Ungeprüftes daneben. Strg+C räumt Server, Edge und Temp-Ordner auf; das
 * Aufräumen selbst hat Fristen (/json/version, Browser.close).
 *
 * Freier Weg (Auflage aus 3a-4): Ein zweites Edge mit eigenem Profil, das
 * sich bei localhost nicht bei Windows anmeldet (--auth-server-allowlist
 * auf einen Namen, den es nicht gibt; Anmelde-Rückfragen bricht die Probe
 * ab). Dort zeigt das Cockpit die Werte des freien Wegs (Schlüssel
 * unbekannt), /pc den Satz zur fehlenden Anmeldung und keine geschützten
 * Daten (keine Starts, keine Ordner, Verlauf nur frei).
 *
 * Geprüft wird auf /pc und auf der Startseite (Cockpit), bei 375 und
 * 1280 px, hell und dunkel:
 *   - Die Ampeln Defender, Neustart und Schlüssel zeigen die Fachgröße, die
 *     /api/pc und /api/pc/benutzer liefern (`befund`, `neustart`,
 *     `vollstaendig`; bei 401 oder 403 unbekannt), und ihre Farbe passt
 *     dazu; `unbekannt` ist nie grün und hat einen Satz.
 *   - Die Ringe zeigen die Stufe der Belegung vom Server (`cpu_belegung`,
 *     `mem_belegung`, `disks[0].belegung`) und malen in der passenden Farbe.
 *   - Keine Fehler in der Konsole, kein seitliches Scrollen bei 375 px.
 *
 * Aufruf aus dem Repo (vorher `cargo build` und `npm run build` in web\):
 *
 *     node tests\skripte\pc-edge.mjs [--web <Ordner mit index.html>]
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
const EXE = resolve(
  argument("exe", join(REPO, "target", "debug", "dashboard.exe")),
);
const WEB = resolve(argument("web", join(REPO, "web", "build")));
const EDGE = argument(
  "edge",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
);
const BEHALTEN = process.argv.includes("--behalten");
const DEBUG_PORT = PORT + 1000;
const BASIS = `http://localhost:${PORT}`;
const SYSTEM = (name) =>
  join(process.env.SystemRoot ?? "C:\\Windows", "System32", name);

const ordner = mkdtempSync(join(tmpdir(), "dashboard-pc-e2e-"));
/** Debug-Port des zweiten Edge für den freien Weg (ohne Windows-Anmeldung). */
const DEBUG_PORT_FREI = DEBUG_PORT + 1;
const prozesse = [];
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

/** Debug-Ports der Edge-Prozesse, die diese Probe selbst gestartet hat. */
const eigeneDebugPorts = new Set();

/**
 * Edge über den Debug-Port schließen; jeder Schritt mit Frist (Auflage aus
 * 3a-4). Nur ein Edge, das diese Probe gestartet hat: Auf einem belegten
 * Port läuft vielleicht die Probe eines anderen.
 */
async function edgeSchliessen(port) {
  if (!eigeneDebugPorts.has(port)) return;
  try {
    const v = await mitFrist(
      fetch(`http://127.0.0.1:${port}/json/version`, {
        signal: AbortSignal.timeout(3000),
      }).then((r) => r.json()),
      4000,
      "/json/version",
    );
    const b = await mitFrist(Cdp.verbinden(v.webSocketDebuggerUrl), 3000, "Verbinden");
    await Promise.race([b.senden("Browser.close", {}, 3000).catch(() => {}), pause(3000)]);
    await pause(1000);
  } catch {
    // Edge läuft nicht (mehr) oder antwortet nicht; taskkill räumt auf
  }
}

async function aufraeumenJetzt() {
  await edgeSchliessen(DEBUG_PORT);
  await edgeSchliessen(DEBUG_PORT_FREI);
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
      `pipe = "dashboard-bote-pc-${process.pid}"`,
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

/**
 * Edge headless mit Wegwerf-Profil. Ohne Angaben das Profil der Probe mit
 * Windows-Anmeldung für localhost; `freierWeg.starten()` nimmt ein zweites
 * Profil auf eigenem Debug-Port ohne Anmeldung.
 */
async function edgeStarten({
  port = DEBUG_PORT,
  profilName = "edge-profil",
  anmeldungFuer = "localhost,127.0.0.1",
} = {}) {
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
      const liste = await (
        await fetch(`http://127.0.0.1:${port}/json/list`)
      ).json();
      seite = liste.find((t) => t.type === "page") ?? null;
    } catch {
      // Edge startet noch
    }
    if (seite === null) await pause(200);
  }
  if (seite === null) throw new Error("Edge antwortet nicht auf dem Debug-Port");
  const cdp = await Cdp.verbinden(seite.webSocketDebuggerUrl);
  await cdp.senden("Page.enable");
  await cdp.netzSperren();
  // Auch Service-Worker, Worker und weitere Ziele (edge-gemeinsam.mjs).
  cdp.browserSperre = await browserSperren(port, cdp.gesperrt, { ausser: seite.id });
  await cdp.senden("Runtime.enable");
  await cdp.senden("Log.enable");
  return cdp;
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
 * Fakten vom Server (mit derselben Anmeldung wie die Seite) und was das
 * Cockpit zeigt: je Ampel die Fachgröße (data-*), die Bedeutung (Klasse
 * des Punkts) und der Satz; je Ring die Stufe und die Klasse des Bogens.
 */
const COCKPIT = `(async () => {
	const pc = await (await fetch('/api/pc', { headers: { accept: 'application/json' } })).json();
	const b = await fetch('/api/pc/benutzer', { headers: { accept: 'application/json' } });
	const benutzer = b.ok ? await b.json() : null;
	const pille = (name) => {
		const e = document.querySelector('.cockpit [data-pruefung="' + name + '"]');
		if (!e) return null;
		const punkt = e.querySelector('.dot');
		const ton = punkt ? [...punkt.classList].find((k) => k.startsWith('b-')) ?? null : null;
		return { daten: { ...e.dataset }, ton, text: e.textContent.replace(/\\s+/g, ' ').trim() };
	};
	const ringe = [...document.querySelectorAll('.cockpit .ring')].map((r) => ({
		name: r.getAttribute('aria-label'),
		belegung: r.dataset.belegung,
		ton: [...(r.querySelector('circle.fg')?.classList ?? [])].find((k) => k.startsWith('b-')) ?? null
	}));
	return {
		fakten: {
			defender: pc.snapshot.defender.befund,
			defenderGruende: pc.snapshot.defender.gruende,
			neustart: pc.snapshot.neustart.neustart,
			cpu: pc.snapshot.cpu_belegung,
			mem: pc.snapshot.mem_belegung,
			platte: pc.snapshot.disks[0]?.belegung ?? null,
			schwellen: pc.snapshot.belegung_schwellen,
			benutzerStatus: b.status,
			vollstaendig: benutzer ? benutzer.schluessel.vollstaendig : 'unbekannt'
		},
		defender: pille('defender'),
		neustart: pille('neustart'),
		schluessel: pille('schluessel'),
		ringe
	};
})()`;

/** Welche Farbe zu einer Fachgröße gehört (wie web/src/lib/pc.ts). */
const TON = {
  defender: { "in-ordnung": "b-ok", ansehen: "b-problem", unbekannt: "b-neutral" },
  neustart: { "nicht-noetig": "b-ok", aussteht: "b-wartet", unbekannt: "b-neutral" },
  vollstaendig: { ja: "b-ok", nein: "b-problem", unbekannt: "b-neutral" },
  belegung: { normal: "b-ok", hoch: "b-wartet", kritisch: "b-problem", unbekannt: "b-neutral" },
};

async function sicher(satz, f) {
  try {
    await f();
  } catch (e) {
    pruefe(false, satz, String(e?.message ?? e).slice(0, 300));
  }
}

/** Wartet, bis der Server gemessen und den Defender einmal gelesen hat. */
async function bisGemessen() {
  for (let i = 0; i < 120; i++) {
    try {
      const pc = await (await fetch(`http://127.0.0.1:${PORT}/api/pc`)).json();
      if (pc.snapshot.taken_at && pc.snapshot.defender.checked_at) return true;
    } catch {
      // noch nicht da
    }
    await pause(500);
  }
  hinweise.push("Der Defender war nach 60 s noch nicht gelesen; geprüft wird trotzdem (unbekannt).");
  return false;
}

function pillePruefen(wo, name, pille, wert, tabelle) {
  if (!pruefe(pille !== null, `${wo}: Ampel ${name} steht da`)) return;
  const schluessel = { defender: "befund", neustart: "neustart", schluessel: "vollstaendig" }[name];
  pruefe(
    pille.daten[schluessel] === wert,
    `${wo}: ${name} zeigt ${wert} wie der Server`,
    JSON.stringify(pille.daten),
  );
  pruefe(
    pille.ton === tabelle[wert] && pille.daten.bedeutung === tabelle[wert].slice(2),
    `${wo}: ${name} hat die Farbe zu ${wert}`,
    `${pille.ton} / ${pille.daten.bedeutung}`,
  );
  pruefe(pille.text.length > 3, `${wo}: ${name} hat einen Satz`, pille.text);
  if (wert === "unbekannt")
    pruefe(
      pille.ton !== "b-ok" && !/aktuell|kein Neustart nötig|alle \d+ gespeichert/.test(pille.text),
      `${wo}: ${name} unbekannt ist nie wie in Ordnung`,
      `${pille.ton}: ${pille.text}`,
    );
}

async function cockpitPruefen(cdp, pfad, breite, thema) {
  const k = konsoleMitschreiben(cdp);
  await cdp.oeffnen(pfad);
  const w = await cdp.auswerten(SEITLICH);
  const c = await cdp.auswerten(COCKPIT);
  k.stopp();
  const wo = `${pfad} (${breite} px, ${thema})`;
  pruefe(k.fehler.length === 0, `${wo}: keine Fehler in der Konsole`, k.fehler.join(" | "));
  if (breite <= 375)
    pruefe(
      w.breit <= w.geraet && w.fenster <= w.geraet,
      `${wo}: kein seitliches Scrollen`,
      `Seite ${w.breit} px, Fenster ${w.fenster} px, Gerät ${w.geraet} px; zu breit: ${w.zuBreit.join(", ")}`,
    );
  const f = c.fakten;
  pruefe(
    f.schwellen?.hoch_ab === 70 && f.schwellen?.kritisch_ab === 90,
    `${wo}: Schwellen der Belegung kommen als Fakt`,
    JSON.stringify(f.schwellen),
  );
  pillePruefen(wo, "defender", c.defender, f.defender, TON.defender);
  pillePruefen(wo, "neustart", c.neustart, f.neustart, TON.neustart);
  pillePruefen(wo, "schluessel", c.schluessel, f.vollstaendig, TON.vollstaendig);
  if (f.benutzerStatus !== 200)
    pruefe(
      f.vollstaendig === "unbekannt",
      `${wo}: Schlüssel ohne /api/pc/benutzer (${f.benutzerStatus}) unbekannt`,
    );
  pruefe(c.ringe.length === 3, `${wo}: drei Ringe`, String(c.ringe.length));
  // Prozessor und Speicher ändern sich alle 5 s; geprüft wird, dass Stufe und
  // Farbe zusammenpassen. Die Platte muss genau der Fakt sein.
  for (const r of c.ringe)
    pruefe(
      r.belegung in TON.belegung && r.ton === TON.belegung[r.belegung],
      `${wo}: Ring „${(r.name ?? "").split(":")[0]}“ malt die Stufe ${r.belegung}`,
      `${r.ton}`,
    );
  const platte = c.ringe[2];
  if (platte)
    pruefe(
      platte.belegung === f.platte,
      `${wo}: Ring der Platte zeigt ${f.platte} wie der Server`,
      String(platte.belegung),
    );
  const speicher = c.ringe[1];
  if (speicher && speicher.belegung !== f.mem)
    hinweise.push(`${wo}: Arbeitsspeicher wechselte zwischen Seite und Abfrage (${speicher.belegung} / ${f.mem})`);
  return c;
}

/**
 * /schluessel?gespeichert=… (Hotfix 3a-4): Live lief die Seite in eine
 * Endlosschleife von Navigationen (goto mit shallow rief afterNavigate immer
 * wieder). Erwartet nach 3 s: Adresse ohne Parameter, genau ein neuer
 * Verlaufseintrag, kein Throttling, Bestätigung sichtbar, Konfetti genau einmal.
 */
async function schluesselPruefen(cdp) {
  const wo = "/schluessel?gespeichert=probe-schluessel";
  const meldungen = [];
  const navigationen = [];
  const h = (m) => {
    if (m.method === "Log.entryAdded") meldungen.push(m.params.entry.text ?? "");
    else if (m.method === "Runtime.consoleAPICalled")
      meldungen.push(m.params.args.map((a) => a.value ?? a.description ?? "").join(" "));
    else if (m.method === "Page.frameNavigated" && !m.params.frame.parentId)
      navigationen.push(`voll ${m.params.frame.url}`);
    else if (m.method === "Page.navigatedWithinDocument")
      navigationen.push(`flach ${m.params.url}`);
  };
  // Konfetti zählen, bevor die Seite läuft: jeder neue .confetti-Knoten.
  const { identifier } = await cdp.senden("Page.addScriptToEvaluateOnNewDocument", {
    source: `window.__konfetti = 0; const gesehen = new WeakSet();
      new MutationObserver((ms) => { for (const m of ms) for (const n of m.addedNodes)
        if (n.nodeType === 1) for (const c of [n, ...n.querySelectorAll('.confetti')]) if (c.matches('.confetti') && !gesehen.has(c)) { gesehen.add(c); window.__konfetti++; } })
        .observe(document, { childList: true, subtree: true });`,
  });
  try {
    await cdp.oeffnen("/pc");
    const vorher = await cdp.auswerten("history.length");
    cdp.hoerer.push(h);
    await cdp.senden("Page.navigate", { url: `${BASIS}/schluessel?gespeichert=probe-schluessel` });
    await pause(3000);
    cdp.hoerer.splice(cdp.hoerer.indexOf(h), 1);
    const s = await cdp.auswerten(`({
      suche: location.search,
      verlauf: history.length,
      bestaetigt: document.body.innerText.includes('„probe-schluessel“ ist gespeichert'),
      konfetti: window.__konfetti,
    })`);
    pruefe(s.suche === "", `${wo}: Adresse ohne Parameter`, s.suche);
    pruefe(
      s.verlauf === vorher + 1,
      `${wo}: genau ein neuer Verlaufseintrag`,
      `${vorher} → ${s.verlauf}`,
    );
    const voll = navigationen.filter((n) => n.startsWith("voll"));
    const flach = navigationen.filter((n) => n.startsWith("flach"));
    // SvelteKit schreibt beim Start den eigenen Zustand per replaceState (flach,
    // gleiche Adresse); zählt nur, wohin die Adresse wechselt.
    const ohne = flach.filter((n) => !n.includes("gespeichert="));
    pruefe(
      voll.length === 1 && ohne.length === 1 && flach.length < 6,
      `${wo}: eine Navigation, die Adresse wechselt genau einmal ohne Parameter`,
      navigationen.slice(0, 6).join(" | "),
    );
    const drossel = meldungen.filter((m) => /throttl/i.test(m));
    pruefe(drossel.length === 0, `${wo}: kein Throttling in der Konsole`, drossel[0] ?? "");
    pruefe(s.bestaetigt, `${wo}: Bestätigung sichtbar`);
    pruefe(s.konfetti === 1, `${wo}: Konfetti genau einmal`, String(s.konfetti));
  } finally {
    await cdp.senden("Page.removeScriptToEvaluateOnNewDocument", { identifier });
  }
}

// ----- Vor dem Start (Auflagen aus 3a-4) -----

/** Bricht mit klarem Satz ab, wenn der Testport oder ein Debug-Port belegt ist. */
async function portePruefen() {
  // Etappe 4b: gemeinsam mit allen Proben (edge-gemeinsam.mjs).
  await portePruefenGemeinsam(PORT);
}

/**
 * KOPIEN von data\: nur die eingecheckten Dateien (git ls-files data), nie
 * etwas daneben (etwa Tresor-Dateien eines echten Laufs). Der Server darf
 * darin nichts am Repo ändern.
 */
function datenKopieren() {
  const liste = execFileSync("git", ["-C", INHALT.wurzel, "ls-files", "-z", "--", "data"], {
    encoding: "utf8",
    windowsHide: true,
  })
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

// ----- Freier Weg im Browser (Auflage aus 3a-4) -----

/** Was der freie Weg im Browser zeigt: Antworten der geschützten Wege und die Seite. */
const FREI = `(async () => {
	const status = async (p) => (await fetch(p, { headers: { accept: 'application/json' } })).status;
	const text = document.querySelector('main')?.innerText ?? '';
	const verlauf = document.querySelector('[data-zeit="verlauf"]');
	return {
		benutzer: await status('/api/pc/benutzer'),
		verlaufVoll: await status('/api/pc/verlauf/voll'),
		anstehend: await status('/api/anstehend'),
		satzVerlauf: text.includes('Starts, Meldungen und Vorgänge zeigt der Verlauf nur mit Anmeldung.'),
		satzBenutzer: text.includes('Diese Werte zeigt das Dashboard nur nach Windows-Anmeldung'),
		ordner: document.querySelectorAll('table.folders').length,
		filter: [...(verlauf?.querySelectorAll('.filter button') ?? [])].map((b) => b.textContent.trim()),
		quellen: [...(verlauf?.querySelectorAll('.quelle') ?? [])].map((q) => q.textContent.trim()),
		verlaufDa: verlauf !== null,
	};
})()`;

const freierWeg = {
  /** Zweites Edge, eigenes Profil, keine Windows-Anmeldung für localhost. */
  async starten() {
    const cdp = await edgeStarten({
      port: DEBUG_PORT_FREI,
      profilName: "edge-profil-frei",
      anmeldungFuer: "nicht-dieser-pc.invalid",
    });
    // Fragt der Browser doch nach einer Anmeldung, bricht die Probe sie ab.
    cdp.anmeldeRueckfragen = 0;
    cdp.hoerer.push((m) => {
      if (m.method !== "Fetch.authRequired") return;
      cdp.anmeldeRueckfragen++;
      cdp
        .senden("Fetch.continueWithAuth", {
          requestId: m.params.requestId,
          authChallengeResponse: { response: "CancelAuth" },
        })
        .catch(() => {});
    });
    await cdp.senden("Fetch.enable", { patterns: [{ urlPattern: "*" }], handleAuthRequests: true });
    return cdp;
  },

  /** Cockpit mit den Werten des freien Wegs, /pc ohne geschützte Daten. */
  async pruefen(cdp) {
    await cdp.thema("light");
    await cdp.breite(1280);
    for (const pfad of ["/pc", "/"]) {
      const wo = `freier Weg ${pfad}`;
      const fehler = [];
      const h = (m) => {
        if (m.method === "Runtime.exceptionThrown")
          fehler.push(`Ausnahme: ${m.params.exceptionDetails.text}`.slice(0, 300));
        else if (m.method === "Log.entryAdded" && m.params.entry.level === "error") {
          // 401/403 der geschützten Wege gehören zum freien Weg.
          if (!/status of 40[13]/.test(m.params.entry.text ?? ""))
            fehler.push(`${m.params.entry.text} ${m.params.entry.url ?? ""}`.slice(0, 300));
        }
      };
      cdp.hoerer.push(h);
      await cdp.oeffnen(pfad);
      const c = await cdp.auswerten(COCKPIT);
      const f = await cdp.auswerten(FREI);
      cdp.hoerer.splice(cdp.hoerer.indexOf(h), 1);
      pruefe(fehler.length === 0, `${wo}: keine Fehler außer 401/403 in der Konsole`, fehler.join(" | "));
      pruefe(
        f.benutzer === 401 && f.verlaufVoll === 401 && f.anstehend === 401,
        `${wo}: ohne Windows-Anmeldung (geschützte Wege 401)`,
        JSON.stringify({ benutzer: f.benutzer, verlaufVoll: f.verlaufVoll, anstehend: f.anstehend }),
      );
      pruefe(
        c.fakten.benutzerStatus !== 200 && c.fakten.vollstaendig === "unbekannt",
        `${wo}: Fakt des freien Wegs: Schlüssel unbekannt`,
        `${c.fakten.benutzerStatus} ${c.fakten.vollstaendig}`,
      );
      pillePruefen(wo, "schluessel", c.schluessel, "unbekannt", TON.vollstaendig);
      pillePruefen(wo, "defender", c.defender, c.fakten.defender, TON.defender);
      pillePruefen(wo, "neustart", c.neustart, c.fakten.neustart, TON.neustart);
      if (pfad === "/pc") {
        pruefe(f.satzVerlauf, `${wo}: Satz „nur mit Anmeldung“ beim Verlauf`);
        pruefe(f.satzBenutzer, `${wo}: Satz zur Windows-Anmeldung bei den Benutzer-Daten`);
        pruefe(f.ordner === 0, `${wo}: keine Ordner (Benutzer-Daten)`, String(f.ordner));
        pruefe(
          f.verlaufDa &&
            !f.filter.some((t) => /^(Starts|Meldungen|Vorgänge)\b/.test(t)) &&
            !f.quellen.some((q) => ["Start", "Meldung", "Vorgang"].includes(q)),
          `${wo}: Verlauf ohne Starts, Meldungen und Vorgänge`,
          `Filter: ${f.filter.join(", ")}; Quellen: ${[...new Set(f.quellen)].join(", ")}`,
        );
      }
    }
    console.log(`  (Anmelde-Rückfragen des Browsers abgebrochen: ${cdp.anmeldeRueckfragen})`);
  },
};

async function main() {
  if (!existsSync(EXE)) throw new Error(`Programm fehlt: ${EXE}`);
  if (!existsSync(join(WEB, "index.html")))
    throw new Error(`Oberfläche fehlt: ${WEB}`);
  await portePruefen();
  mkdirSync(join(ordner, "bin"), { recursive: true });
  mkdirSync(join(ordner, "profil"), { recursive: true });
  copyFileSync(EXE, join(ordner, "bin", "dashboard.exe"));
  // KOPIEN von projects\ und data\: der Server darf darin nichts am Repo ändern.
  cpSync(INHALT.projects, join(ordner, "repo", "projects"), {
    recursive: true,
  });
  datenKopieren();
  console.log(`Temp-Ordner: ${ordner}`);

  await serverStarten(eigeneSid());
  await bisGemessen();
  const cdp = await edgeStarten();

  let fakten = null;
  for (const thema of ["light", "dark"]) {
    await cdp.thema(thema);
    for (const breite of [375, 1280]) {
      await cdp.breite(breite);
      for (const pfad of ["/pc", "/"]) {
        console.log(`\n== ${pfad}, ${breite} px, ${thema} ==`);
        await sicher(`${pfad} ${breite} ${thema}`, async () => {
          const c = await cockpitPruefen(cdp, pfad, breite, thema);
          fakten ??= c.fakten;
        });
      }
    }
  }
  if (fakten) console.log(`\nFakten des Servers: ${JSON.stringify(fakten)}`);

  console.log("\n== /schluessel nach dem Speichern ==");
  await cdp.thema("light");
  await cdp.breite(1280);
  await sicher("/schluessel?gespeichert", () => schluesselPruefen(cdp));

  console.log("\n== Freier Weg (zweites Profil ohne Windows-Anmeldung) ==");
  let frei = null;
  await sicher("freier Weg", async () => {
    frei = await freierWeg.starten();
    await freierWeg.pruefen(frei);
  });

  const gesperrt = [...cdp.gesperrt, ...(frei?.gesperrt ?? [])];
  console.log(`\n${cdp.browserSperre.bericht()}`);
  pruefe(
    gesperrt.length === 0,
    "Netzsperre: die Seiten fragen nur beim eigenen Server an",
    gesperrt.slice(0, 5).join(" | "),
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
