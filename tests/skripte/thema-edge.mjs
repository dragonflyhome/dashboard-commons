#!/usr/bin/env node
/**
 * Edge-Probe für Etappe 3b (Schönheit, Teil B): Schalter Hell/Dunkel,
 * Startseite als Spalten, Textfassung im Katalog, Logo-Entwürfe. Gleicher
 * Weg wie katalog-edge.mjs und pc-edge.mjs: Kopie der exe, Kopie von
 * projects\ und den eingecheckten Dateien aus data\ (git ls-files) in einem
 * Temp-Ordner, Port 8788 (nie 8787, nie C:\ProgramData\Dashboard), Edge
 * headless mit Wegwerf-Profil, Netzsperre auf Seite und Browser-Ebene
 * (edge-gemeinsam.mjs), jede CDP-Nachricht mit Frist (cdp-frist.mjs).
 *
 * Geprüft wird:
 *   - CSP: script-src ohne 'unsafe-inline', mit der Prüfsumme des
 *     Thema-Skripts aus index.html; keine CSP-Verletzung im Browser
 *   - Kein Aufblitzen: „Dunkel“ gewählt bei hellem System und „Hell“ bei
 *     dunklem (Emulation.setEmulatedMedia), dazu beide ohne Wahl. Ein
 *     Skript, das vor allen Skripten der Seite läuft
 *     (Page.addScriptToEvaluateOnNewDocument), liest Hintergrund von body
 *     und data-theme im ersten Bild (requestAnimationFrame), bei
 *     DOMContentLoaded und nach load; alle drei müssen das gewählte Thema
 *     zeigen
 *   - Schalter mit der Tastatur: Fokus auf „System“, Pfeil rechts wählt
 *     „Hell“, noch einmal „Dunkel“; gemerkt nach Neuladen
 *   - Startseite „Spalten“ (gemerkt je Browser): bei 375 px untereinander,
 *     kein seitliches Scrollen; bei 1280 px nebeneinander; Zahl in den
 *     Spalten gleich der Zählzeile
 *   - /katalog: Abschnitt „Als Text“ mit offenen Tabellen; „Als Text“ bei
 *     Gantt macht die Tabelle sichtbar; 375 px ohne seitliches Scrollen
 *   - /katalog/entwuerfe/logo: drei Entwürfe, je hell und dunkel; die Kästen
 *     zeigen ihr Thema unabhängig vom System; 375 px ohne seitliches Scrollen
 *   - keine Fehler in der Konsole, keine Anfrage außerhalb 127.0.0.1
 *
 * Aufruf aus dem Repo (vorher `cargo build` und ein Bau der Oberfläche):
 *
 *     node tests\skripte\thema-edge.mjs [--web <Ordner mit index.html>]
 *
 * Optional: --exe <dashboard.exe>, --edge <msedge.exe>, --port 8788,
 * --behalten. Exit 0: alles bestanden, 1: mindestens eine Prüfung nicht.
 */
import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { antwortAbwarten } from "./cdp-frist.mjs";
import {
  beiAbbruch,
  browserSperren,
  mitFrist,
  portePruefen,
  inhaltOrt,
} from "./edge-gemeinsam.mjs";

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

/** Hintergrund von body in den beiden Themen (--bg aus app.css). */
const HELL = "rgb(244, 246, 251)";
const DUNKEL = "rgb(15, 20, 32)";
const SCHLUESSEL = "dashboard-thema";

const ordner = mkdtempSync(join(tmpdir(), "dashboard-thema-e2e-"));
const prozesse = [];
let edgeGestartet = false;
// Ein Aufräumen für alle Wege (finally, Strg+C, unbehandelter Fehler).
const aufraeumen = beiAbbruch(aufraeumenJetzt);
let bestanden = 0;
const fehlgeschlagen = [];

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
    if (!edgeGestartet) throw new Error("kein eigenes Edge");
    const v = await mitFrist(
      fetch(`http://127.0.0.1:${DEBUG_PORT}/json/version`).then((r) =>
        r.json(),
      ),
      4000,
      "/json/version",
    );
    const b = await mitFrist(
      Cdp.verbinden(v.webSocketDebuggerUrl),
      3000,
      "Verbinden",
    );
    await Promise.race([
      b.senden("Browser.close", {}, 3000).catch(() => {}),
      pause(3000),
    ]);
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

/** Nur die eingecheckten Dateien aus data\ (wie pc-edge.mjs). */
function datenKopieren() {
  const liste = execFileSync(
    "git",
    ["-C", INHALT.wurzel, "ls-files", "-z", "--", "data"],
    {
      encoding: "utf8",
      windowsHide: true,
    },
  )
    .split("\0")
    .filter((d) => d !== "");
  for (const datei of liste) {
    const quelle = join(INHALT.wurzel, ...datei.split("/"));
    if (!existsSync(quelle)) continue;
    const ziel = join(ordner, "repo", ...datei.split("/"));
    mkdirSync(dirname(ziel), { recursive: true });
    copyFileSync(quelle, ziel);
  }
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
      `pipe = "dashboard-bote-thema-${process.pid}"`,
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
    await pause(400);
  }

  async taste(key, vk, text) {
    await this.senden("Input.dispatchKeyEvent", {
      type: "keyDown",
      key,
      code: key,
      windowsVirtualKeyCode: vk,
      ...(text === undefined ? {} : { text }),
    });
    await this.senden("Input.dispatchKeyEvent", {
      type: "keyUp",
      key,
      code: key,
      windowsVirtualKeyCode: vk,
    });
    await pause(200);
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

  async system(wert) {
    await this.senden("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: wert }],
    });
    await pause(150);
  }
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
        cdp.browserSperre = await browserSperren(DEBUG_PORT, cdp.gesperrt, {
          ausser: seite.id,
        });
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
        `${m.params.entry.source}: ${m.params.entry.text}`.slice(0, 400),
      );
    }
  };
  cdp.hoerer.push(h);
  return { fehler, stopp: () => cdp.hoerer.splice(cdp.hoerer.indexOf(h), 1) };
}

// ----- Messen im Browser -----

/**
 * Läuft vor jedem Skript der Seite (auch vor dem Thema-Skript im head): hält
 * Hintergrund und data-theme im ersten Bild, bei DOMContentLoaded und nach
 * load fest, dazu jede CSP-Verletzung.
 */
const FRUEH = `(() => {
  const m = { csp: [] };
  window.__thema = m;
  const lesen = () => ({
    hintergrund: document.body ? getComputedStyle(document.body).backgroundColor : null,
    thema: document.documentElement.getAttribute('data-theme'),
  });
  document.addEventListener('securitypolicyviolation', (e) => m.csp.push(e.violatedDirective + ' ' + (e.blockedURI || '')));
  requestAnimationFrame(() => { m.erstesBild = lesen(); });
  document.addEventListener('DOMContentLoaded', () => { m.geladen = lesen(); });
  window.addEventListener('load', () => { m.fertig = lesen(); });
})();`;

const SEITLICH = `(() => ({ breit: document.documentElement.scrollWidth, geraet: screen.width }))()`;

async function speicherSetzen(cdp, eintraege) {
  // Eine kleine Seite derselben Herkunft, damit localStorage dazu passt.
  const geladen = cdp.warteAuf("Page.loadEventFired");
  // Scheitert das Navigieren, darf diese Frist nicht unbehandelt ablaufen.
  geladen.catch(() => {});
  await cdp.senden("Page.navigate", { url: `${BASIS}/healthz` });
  await geladen;
  await cdp.auswerten(`(() => {
    localStorage.clear();
    const e = ${JSON.stringify(eintraege)};
    for (const k of Object.keys(e)) localStorage.setItem(k, e[k]);
    return true;
  })()`);
}

async function themaPruefen(cdp, system, wahl) {
  const soll =
    wahl === "dunkel" || (wahl === null && system === "dark") ? DUNKEL : HELL;
  const sollAttr = wahl;
  const name = `System ${system}, Wahl ${wahl ?? "System"}`;
  await cdp.system(system);
  await speicherSetzen(cdp, wahl === null ? {} : { [SCHLUESSEL]: wahl });
  const k = konsoleMitschreiben(cdp);
  await cdp.oeffnen("/");
  await pause(300);
  const m = await cdp.auswerten("window.__thema");
  k.stopp();
  for (const zeitpunkt of ["erstesBild", "geladen", "fertig"]) {
    const w = m?.[zeitpunkt];
    pruefe(
      w?.hintergrund === soll && w?.thema === sollAttr,
      `${name}: ${zeitpunkt === "erstesBild" ? "erstes Bild" : zeitpunkt === "geladen" ? "DOMContentLoaded" : "nach load"} im richtigen Thema`,
      JSON.stringify(w),
    );
  }
  pruefe(
    m?.csp?.length === 0,
    `${name}: keine CSP-Verletzung`,
    JSON.stringify(m?.csp),
  );
  const gewaehlt = await cdp.auswerten(
    `document.querySelector('footer [role="radio"][aria-checked="true"]')?.textContent.trim()`,
  );
  pruefe(
    gewaehlt ===
      (wahl === "hell" ? "Hell" : wahl === "dunkel" ? "Dunkel" : "System"),
    `${name}: der Schalter zeigt die Wahl`,
    String(gewaehlt),
  );
  pruefe(
    k.fehler.length === 0,
    `${name}: keine Fehler in der Konsole`,
    k.fehler.join(" | "),
  );
}

async function main() {
  if (!existsSync(EXE)) throw new Error(`${EXE} fehlt: vorher cargo build`);
  if (!existsSync(join(WEB, "index.html")))
    throw new Error(`${WEB}\\index.html fehlt: vorher bauen`);
  // Etappe 4b: Testserver und beide Edge-Debug-Ports, wie alle Proben.
  await portePruefen(PORT);
  mkdirSync(join(ordner, "bin"), { recursive: true });
  mkdirSync(join(ordner, "profil"), { recursive: true });
  copyFileSync(EXE, join(ordner, "bin", "dashboard.exe"));
  cpSync(INHALT.projects, join(ordner, "repo", "projects"), {
    recursive: true,
  });
  datenKopieren();
  console.log(`Temp-Ordner: ${ordner}`);

  await serverStarten(eigeneSid());

  console.log("\n== CSP ==");
  const index = readFileSync(join(WEB, "index.html"), "utf8");
  const thema = /<script>([\s\S]*?)<\/script>/.exec(index)?.[1] ?? "";
  const hash = createHash("sha256").update(thema, "utf8").digest("base64");
  const antwort = await fetch(`http://127.0.0.1:${PORT}/`);
  const csp = antwort.headers.get("content-security-policy") ?? "";
  const scriptSrc =
    csp.split(";").find((d) => d.trim().startsWith("script-src")) ?? "";
  pruefe(
    thema.includes(SCHLUESSEL),
    "index.html: das Thema-Skript steht im head",
    thema.slice(0, 80),
  );
  pruefe(
    scriptSrc.includes(`'sha256-${hash}'`),
    "CSP: Prüfsumme des Thema-Skripts in script-src",
    scriptSrc,
  );
  pruefe(
    !scriptSrc.includes("unsafe-inline"),
    "CSP: script-src ohne 'unsafe-inline'",
    scriptSrc,
  );

  const cdp = await edgeStarten();
  await cdp.senden("Page.addScriptToEvaluateOnNewDocument", { source: FRUEH });
  await cdp.breite(1280);

  console.log("\n== Kein Aufblitzen ==");
  await themaPruefen(cdp, "light", "dunkel");
  await themaPruefen(cdp, "dark", "hell");
  await themaPruefen(cdp, "light", null);
  await themaPruefen(cdp, "dark", null);

  console.log("\n== Schalter mit der Tastatur ==");
  await cdp.system("light");
  await speicherSetzen(cdp, {});
  await cdp.oeffnen("/");
  const fokus = await cdp.auswerten(`(() => {
    const e = document.querySelector('footer [role="radio"][aria-checked="true"]');
    e?.scrollIntoView({ block: 'center' });
    e?.focus();
    return document.activeElement?.textContent.trim();
  })()`);
  pruefe(
    fokus === "System",
    "Tab-Ziel ist die gewählte Möglichkeit „System“",
    String(fokus),
  );
  await cdp.taste("ArrowRight", 39);
  let z = await cdp.auswerten(
    `({ thema: document.documentElement.getAttribute('data-theme'), gemerkt: localStorage.getItem('${SCHLUESSEL}'), hintergrund: getComputedStyle(document.body).backgroundColor, fokus: document.activeElement?.textContent.trim() })`,
  );
  pruefe(
    z.thema === "hell" &&
      z.gemerkt === "hell" &&
      z.hintergrund === HELL &&
      z.fokus === "Hell",
    "Pfeil rechts: „Hell“ gewählt, gemerkt, Fokus folgt",
    JSON.stringify(z),
  );
  await cdp.taste("ArrowRight", 39);
  z = await cdp.auswerten(
    `({ thema: document.documentElement.getAttribute('data-theme'), hintergrund: getComputedStyle(document.body).backgroundColor })`,
  );
  pruefe(
    z.thema === "dunkel" && z.hintergrund === DUNKEL,
    "noch einmal: „Dunkel“ sofort",
    JSON.stringify(z),
  );
  await cdp.oeffnen("/pc");
  const m = await cdp.auswerten("window.__thema");
  pruefe(
    m?.erstesBild?.hintergrund === DUNKEL,
    "gemerkt: nächste Seite schon im ersten Bild dunkel",
    JSON.stringify(m?.erstesBild),
  );

  console.log("\n== Startseite als Spalten ==");
  await cdp.system("light");
  for (const breite of [375, 1280]) {
    await cdp.breite(breite);
    await speicherSetzen(cdp, { "dashboard-startseite-ansicht": "spalten" });
    const k = konsoleMitschreiben(cdp);
    await cdp.oeffnen("/");
    k.stopp();
    const s = await cdp.auswerten(`(() => {
      const spalten = [...document.querySelectorAll('.spalten > .spalte')];
      const links = new Set(spalten.map((e) => Math.round(e.getBoundingClientRect().left)));
      const zahlen = spalten.map((e) => Number(e.querySelector('[data-anzahl]')?.getAttribute('data-anzahl')));
      const zeile = document.querySelector('#projekte')?.parentElement?.querySelector('.muted')?.textContent ?? '';
      const gewaehlt = document.querySelector('[role="radiogroup"] [aria-checked="true"]')?.textContent.trim();
      return { spalten: spalten.length, links: links.size, summe: zahlen.reduce((a, b) => a + b, 0), zeile, gewaehlt, karten: document.querySelectorAll('.cards').length };
    })()`);
    pruefe(
      s.spalten > 0 && s.karten === 0 && s.gewaehlt === "Spalten",
      `${breite} px: Ansicht „Spalten“ gemerkt`,
      JSON.stringify(s),
    );
    const gesamt = Number(/^(\d+)/.exec(s.zeile)?.[1] ?? -1);
    pruefe(
      s.summe === gesamt,
      `${breite} px: Spalten zählen so viele wie die Zählzeile`,
      `${s.summe} gegen „${s.zeile}“`,
    );
    if (breite === 375)
      pruefe(s.links === 1, "375 px: Spalten untereinander", JSON.stringify(s));
    else
      pruefe(s.links > 1, "1280 px: Spalten nebeneinander", JSON.stringify(s));
    const w = await cdp.auswerten(SEITLICH);
    pruefe(
      w.breit <= w.geraet,
      `${breite} px: kein seitliches Scrollen`,
      JSON.stringify(w),
    );
    pruefe(
      k.fehler.length === 0,
      `${breite} px: keine Fehler in der Konsole`,
      k.fehler.join(" | "),
    );
  }

  console.log("\n== /katalog: Als Text ==");
  for (const breite of [375, 1280]) {
    await cdp.breite(breite);
    const k = konsoleMitschreiben(cdp);
    await cdp.oeffnen("/katalog");
    k.stopp();
    const t = await cdp.auswerten(`(() => {
      const a = document.getElementById('als-text');
      const tabellen = a ? [...a.querySelectorAll('table')] : [];
      return { tabellen: tabellen.length, sichtbar: tabellen.filter((t) => t.getBoundingClientRect().width > 50).length };
    })()`);
    pruefe(
      t.tabellen >= 8 && t.sichtbar === t.tabellen,
      `${breite} px: Abschnitt „Als Text“ mit offenen Tabellen`,
      JSON.stringify(t),
    );
    const umschalten = await cdp.auswerten(`(async () => {
      const knopf = document.querySelector('#zeit-gantt [data-textfassung] button');
      const tabelle = document.querySelector('#zeit-gantt [data-textfassung] table');
      if (!knopf || !tabelle) return null;
      // Sichtbar ist, was der Rahmen um die Tabelle zeigt (zu: 1 px, nur für Vorlese-Programme).
      const rahmen = tabelle.parentElement;
      const vorher = rahmen.getBoundingClientRect().width;
      knopf.click();
      await new Promise((r) => setTimeout(r, 200));
      return { vorher, nachher: rahmen.getBoundingClientRect().width, gedrueckt: knopf.getAttribute('aria-pressed') };
    })()`);
    pruefe(
      umschalten !== null &&
        umschalten.vorher <= 1 &&
        umschalten.nachher > 50 &&
        umschalten.gedrueckt === "true",
      `${breite} px: „Als Text“ beim Balkenplan zeigt die Tabelle`,
      JSON.stringify(umschalten),
    );
    const w = await cdp.auswerten(SEITLICH);
    pruefe(
      w.breit <= w.geraet,
      `${breite} px: /katalog ohne seitliches Scrollen`,
      JSON.stringify(w),
    );
    pruefe(
      k.fehler.length === 0,
      `${breite} px: /katalog ohne Fehler in der Konsole`,
      k.fehler.join(" | "),
    );
  }

  console.log("\n== Logo-Entwürfe ==");
  for (const system of ["light", "dark"]) {
    await cdp.system(system);
    await speicherSetzen(cdp, {});
    for (const breite of [375, 1280]) {
      await cdp.breite(breite);
      const k = konsoleMitschreiben(cdp);
      await cdp.oeffnen("/katalog/entwuerfe/logo");
      k.stopp();
      const l = await cdp.auswerten(`(() => {
        const tafeln = [...document.querySelectorAll('[data-thema-vorschau]')];
        return {
          entwuerfe: document.querySelectorAll('figure.entwurf').length,
          hell: tafeln.filter((t) => t.dataset.themaVorschau === 'hell').map((t) => getComputedStyle(t).backgroundColor),
          dunkel: tafeln.filter((t) => t.dataset.themaVorschau === 'dunkel').map((t) => getComputedStyle(t).backgroundColor),
          logos: document.querySelectorAll('[data-thema-vorschau] svg[role="img"]').length,
          skripte: document.querySelectorAll('[data-thema-vorschau] svg script, [data-thema-vorschau] svg foreignObject').length
        };
      })()`);
      const w = await cdp.auswerten(SEITLICH);
      const name = `System ${system}, ${breite} px`;
      pruefe(
        l.entwuerfe === 3 && l.logos === 12 && l.skripte === 0,
        `${name}: drei Entwürfe, je hell und dunkel, groß und klein`,
        JSON.stringify(l),
      );
      pruefe(
        l.hell.length === 3 &&
          l.hell.every((h) => h === HELL) &&
          l.dunkel.length === 3 &&
          l.dunkel.every((h) => h === DUNKEL),
        `${name}: die Kästen zeigen ihr Thema unabhängig vom System`,
        JSON.stringify(l),
      );
      pruefe(
        w.breit <= w.geraet,
        `${name}: kein seitliches Scrollen`,
        JSON.stringify(w),
      );
      pruefe(
        k.fehler.length === 0,
        `${name}: keine Fehler in der Konsole`,
        k.fehler.join(" | "),
      );
    }
  }

  pruefe(
    cdp.gesperrt.length === 0,
    "Netzsperre: nur Anfragen an den eigenen Server",
    cdp.gesperrt.slice(0, 5).join(" | "),
  );
  console.log(
    `\nBestanden: ${bestanden} Prüfungen. Nicht bestanden: ${fehlgeschlagen.length}.`,
  );
  for (const f of fehlgeschlagen) console.log(`  - ${f}`);
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
