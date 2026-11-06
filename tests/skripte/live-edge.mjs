#!/usr/bin/env node
/**
 * Ende-zu-Ende-Probe für die Live-Lücken (Entscheidung
 * immer-aktuell, Punkt 2) mit Edge.
 *
 * Gleicher Weg wie pruefer-edge.mjs: Wegwerf-Daten in einem Temp-Ordner
 * (Kopien von projects\, docs\entscheidungen, docs\zielbild und den
 * eingecheckten Dateien aus data\), eigener Testport (Standard 8868, nie
 * 8787, nie C:\ProgramData\Dashboard), Edge headless mit Wegwerf-Profil,
 * ohne Hintergrund-Netz, DevTools-Protokoll über den WebSocket von Node 24.
 * Netzsperre auf Seite und Browser (edge-gemeinsam.mjs); was nicht
 * 127.0.0.1 oder localhost ist, lässt die Probe scheitern. Sind Testport
 * oder Debug-Ports belegt, bricht sie vor dem Start ab.
 *
 * Je Seite: öffnen, eine Marke ins Fenster legen, dann am Server etwas
 * ändern und warten, bis die Seite es zeigt; die Marke muss danach noch da
 * sein (kein Neuladen).
 *   daten          /daten: eine Aufgabe über die API anlegen (aus der Seite
 *                  heraus, angemeldet); die Zahl der Tabelle aufgaben steigt
 *                  (Ereignis speicher, Bereich aufgaben)
 *   entscheidungen /entscheidungen: eine neue TOML in docs\entscheidungen
 *                  der Kopie; ihr Thema erscheint (Ereignis docs)
 *   pc             /pc: ein Vorgang des Dashboards über die API; er erscheint
 *                  im Verlauf (Ereignis speicher, Bereich vorgaenge, Schlüssel
 *                  api:verlauf), und der Verlauf wurde neu geholt
 *   bild           /projects/<projekt>/zeitachse: das erste Projekt der Kopie mit
 *                  assets/zeitachse-fassung1.svg (oder --bild-projekt): erst eine andere Datei in
 *                  assets/ anlegen, die Adresse des Bildes bleibt (Version je
 *                  Datei); dann das Bild in assets/ der Kopie ersetzen; die
 *                  Adresse bekommt ?v=, und unter ihr liegt der neue Inhalt
 *                  (projekt mit assets und dateien)
 *
 * Aufruf aus dem Repo (vorher `cargo build` und `npm run build` in web\):
 *
 *     node tests\skripte\live-edge.mjs [--port 8868]
 *
 * Optional: --exe <dashboard.exe>, --web <Ordner mit index.html>, --edge
 * <msedge.exe>, --behalten (Temp-Ordner nicht löschen). Exit 0: alles
 * bestanden, 1: mindestens eine Prüfung nicht. Am Ende laufen weder Server
 * noch Edge weiter.
 */
import { spawn, execFileSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
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

const PORT = Number(argument("port", "8868"));
if (PORT === 8787) {
  console.error("Port 8787 ist der echte Dienst. Bitte einen Testport nehmen.");
  process.exit(2);
}
const EXE = resolve(argument("exe", join(REPO, "target", "debug", "dashboard.exe")));
const WEB = resolve(argument("web", join(REPO, "web", "build")));
/** Projekt mit dem Bild für die Probe „bild“ (Unterseite zeitachse); ohne Angabe gesucht (kein Projektname im Code). */
const BILD_PROJEKT = argument("bild-projekt", null);

/** Das erste Projekt (nach Slug) der Kopie, für das `passt(ordner)` stimmt; sonst null. */
function projektMit(passt) {
  const wurzel = join(ordner, "repo", "projects");
  for (const slug of readdirSync(wurzel).sort()) {
    if (!slug.startsWith("_") && passt(join(wurzel, slug))) return slug;
  }
  return null;
}

const EDGE = argument("edge", "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe");
const BEHALTEN = process.argv.includes("--behalten");
const DEBUG_PORT = PORT + 1000;
const BASIS = `http://localhost:${PORT}`;
const SYSTEM = (name) => join(process.env.SystemRoot ?? "C:\\Windows", "System32", name);
/** So lange wartet eine Prüfung höchstens auf die Änderung (Beobachter 2 s, Bündeln 250 ms). */
const FRIST_MS = 15000;

const ordner = mkdtempSync(join(tmpdir(), "dashboard-live-e2e-"));
const prozesse = [];
// Ein Aufräumen für alle Wege (finally, Strg+C, unbehandelter Fehler).
const aufraeumen = beiAbbruch(aufraeumenJetzt);
let bestanden = 0;
const fehlgeschlagen = [];
const messwerte = [];
const pause = (ms) => new Promise((r) => setTimeout(r, ms));

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

async function aufraeumenJetzt() {
  try {
    const v = await mitFrist(
      fetch(`http://127.0.0.1:${DEBUG_PORT}/json/version`, { signal: AbortSignal.timeout(3000) }).then((r) => r.json()),
      4000,
      "/json/version",
    );
    const b = await mitFrist(Cdp.verbinden(v.webSocketDebuggerUrl), 3000, "Verbinden");
    await Promise.race([b.senden("Browser.close", {}, 3000).catch(() => {}), pause(3000)]);
    await pause(1000);
  } catch {
    // Edge läuft nicht (mehr); taskkill räumt auf
  }
  for (const p of prozesse) {
    try {
      execFileSync(SYSTEM("taskkill.exe"), ["/PID", String(p.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
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
  const zeile = execFileSync(SYSTEM("whoami.exe"), ["/user", "/fo", "csv", "/nh"], { encoding: "utf8", windowsHide: true });
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
      `pipe = "dashboard-bote-live-${process.pid}"`,
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
    /** Angefragte Adressen (Network.requestWillBeSent), für „neu geholt“. */
    this.anfragen = [];
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

  async netzSperren() {
    this.hoerer.push((m) => {
      if (m.method === "Network.requestWillBeSent") this.anfragen.push(m.params.request.url);
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
    await this.senden("Fetch.enable", { patterns: [{ urlPattern: "*" }] });
  }

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
    await this.senden("Page.navigate", { url: `${BASIS}${pfad}` });
    await geladen;
    const ende = Date.now() + 8000;
    await pause(600);
    while (Date.now() < ende) {
      const laedt = await this.rufe(() => !document.querySelector("main") || document.querySelector('[aria-busy="true"]') !== null).catch(
        () => true,
      );
      if (!laedt) break;
      await pause(250);
    }
    // Der Live-Kanal muss offen sein, bevor sich am Server etwas ändert.
    const kanal = Date.now() + 8000;
    while (Date.now() < kanal) {
      const offen = await this.rufe(() => (window.__eventQuellen?.() ?? []).includes("/api/events")).catch(() => false);
      if (offen) break;
      await pause(200);
    }
    await pause(500);
  }

  /** Legt eine Marke ins Fenster; fehlt sie später, hat die Seite neu geladen. */
  async markeSetzen() {
    const marke = `m${Math.random().toString(36).slice(2)}`;
    await this.rufe((m) => {
      window.__liveMarke = m;
    }, marke);
    return marke;
  }

  async markeDa(marke) {
    return (await this.rufe(() => window.__liveMarke ?? null)) === marke;
  }

  /** Wartet, bis `fn` im Browser einen wahren Wert liefert; gibt ihn und die Zeit zurück. */
  async warteBis(fn, arg, ms = FRIST_MS) {
    const start = Date.now();
    let letzter = null;
    while (Date.now() - start < ms) {
      letzter = await this.rufe(fn, arg).catch(() => null);
      if (letzter) return { wert: letzter, ms: Date.now() - start };
      await pause(150);
    }
    return { wert: letzter, ms: null };
  }
}

async function edgeStarten() {
  const profil = join(ordner, "edge-profil");
  mkdirSync(profil, { recursive: true });
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
  let seite = null;
  for (let i = 0; i < 100 && seite === null; i++) {
    try {
      const liste = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/list`)).json();
      seite = liste.find((t) => t.type === "page") ?? null;
    } catch {
      // Edge startet noch
    }
    if (seite === null) await pause(200);
  }
  if (seite === null) throw new Error("Edge antwortet nicht auf dem Debug-Port");
  const cdp = await Cdp.verbinden(seite.webSocketDebuggerUrl);
  await cdp.senden("Page.enable");
  // Offene EventSource je Seite (wie pruefer-edge.mjs): erst prüfen, wenn der Kanal steht.
  await cdp.senden("Page.addScriptToEvaluateOnNewDocument", {
    source: `(() => {
      const O = window.EventSource;
      if (!O) return;
      const alle = new Set();
      window.__eventQuellen = () => [...alle].filter((q) => q.readyState === 1).map((q) => new URL(q.url, location.href).pathname);
      window.EventSource = class extends O {
        constructor(u, o) { super(u, o); alle.add(this); }
        close() { alle.delete(this); super.close(); }
      };
    })()`,
  });
  await cdp.netzSperren();
  cdp.browserSperre = await browserSperren(DEBUG_PORT, cdp.gesperrt, { ausser: seite.id });
  await cdp.senden("Runtime.enable");
  await cdp.senden("Network.enable");
  return cdp;
}

/** Eine Anfrage aus der Seite heraus (angemeldet, gleiche Herkunft). */
function POST_AUS_DER_SEITE({ pfad, koerper }) {
  return fetch(pfad, {
    method: "POST",
    credentials: "same-origin",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify(koerper),
  }).then(async (r) => ({ status: r.status, text: (await r.text()).slice(0, 300) }));
}

// ----- Prüfungen -----

async function datenPruefen(cdp) {
  console.log("\ndaten: /daten zeigt eine neue Aufgabe ohne Neuladen");
  await cdp.oeffnen("/daten");
  const zahl = () => {
    for (const tr of document.querySelectorAll("#tabellen tr")) {
      if (tr.querySelector("code")?.textContent?.trim() === "aufgaben") return tr.querySelector("td.num")?.textContent?.trim() ?? null;
    }
    return null;
  };
  const vorher = await cdp.rufe(zahl);
  pruefe(vorher !== null, "daten: Tabelle aufgaben steht da", `Zahl ${vorher}`);
  const marke = await cdp.markeSetzen();
  const r = await cdp.rufe(POST_AUS_DER_SEITE, { pfad: "/api/aufgaben", koerper: { projekt: "dashboard", titel: `Live-Probe ${Date.now()}` } });
  pruefe(r.status === 201, "daten: Aufgabe angelegt (POST /api/aufgaben)", `${r.status} ${r.text}`);
  const { wert, ms } = await cdp.warteBis(
    (alt) => {
      for (const tr of document.querySelectorAll("#tabellen tr")) {
        if (tr.querySelector("code")?.textContent?.trim() === "aufgaben") {
          const z = tr.querySelector("td.num")?.textContent?.trim() ?? null;
          return z !== alt ? z : null;
        }
      }
      return null;
    },
    vorher,
  );
  pruefe(ms !== null, "daten: die Zahl der Aufgaben steigt ohne Neuladen", `${vorher} -> ${wert}${ms !== null ? `, nach ${ms} ms` : ""}`);
  pruefe(await cdp.markeDa(marke), "daten: die Seite hat nicht neu geladen");
  if (ms !== null) messwerte.push(`daten: ${ms} ms`);
}

async function entscheidungenPruefen(cdp) {
  console.log("\nentscheidungen: eine neue Entscheidung erscheint ohne Neuladen");
  await cdp.oeffnen("/entscheidungen");
  const thema = `Live-Probe Entscheidung ${Date.now()}`;
  const da = await cdp.rufe((t) => document.body.innerText.includes(t), thema);
  pruefe(!da, "entscheidungen: das Thema steht noch nicht da");
  const marke = await cdp.markeSetzen();
  writeFileSync(
    join(ordner, "repo", "docs", "entscheidungen", "2026-11-01-live-probe.toml"),
    `datum = "2026-11-01"\nthema = "${thema}"\nstatus = "Probe"\n`,
  );
  const { ms } = await cdp.warteBis((t) => document.body.innerText.includes(t), thema);
  pruefe(ms !== null, "entscheidungen: die neue Entscheidung erscheint (Ereignis docs)", ms !== null ? `nach ${ms} ms` : "");
  pruefe(await cdp.markeDa(marke), "entscheidungen: die Seite hat nicht neu geladen");
  if (ms !== null) messwerte.push(`entscheidungen: ${ms} ms`);
}

async function pcPruefen(cdp) {
  console.log("\npc: ein neuer Eintrag im Verlauf erscheint ohne Neuladen");
  await cdp.oeffnen("/pc");
  const titel = `Live-Probe Verlauf ${Date.now()}`;
  const marke = await cdp.markeSetzen();
  const vorher = cdp.anfragen.filter((u) => u.includes("/api/pc/verlauf")).length;
  const r = await cdp.rufe(POST_AUS_DER_SEITE, { pfad: "/api/vorgaenge", koerper: { art: "wunsch", slug: "dashboard", titel } });
  pruefe(r.status === 201, "pc: Vorgang des Dashboards angelegt (POST /api/vorgaenge)", `${r.status} ${r.text}`);
  const { ms } = await cdp.warteBis((t) => document.body.innerText.includes(t), titel);
  const nachher = cdp.anfragen.filter((u) => u.includes("/api/pc/verlauf")).length;
  pruefe(nachher > vorher, "pc: der Verlauf wurde neu geholt (Schlüssel api:verlauf)", `${vorher} -> ${nachher} Anfragen`);
  pruefe(ms !== null, "pc: der neue Eintrag steht im Verlauf", ms !== null ? `nach ${ms} ms` : "nicht im Text der Seite");
  pruefe(await cdp.markeDa(marke), "pc: die Seite hat nicht neu geladen");
  if (ms !== null) messwerte.push(`pc: ${ms} ms`);
}

async function bildPruefen(cdp) {
  console.log("\nbild: ein ersetztes Bild zeigt sich ohne Neuladen");
  const projekt = BILD_PROJEKT ?? projektMit((p) => existsSync(join(p, "assets", "zeitachse-fassung1.svg")));
  if (!pruefe(projekt !== null, "bild: ein Projekt mit assets/zeitachse-fassung1.svg in der Kopie")) return;
  const datei = join(ordner, "repo", "projects", projekt, "assets", "zeitachse-fassung1.svg");
  if (!pruefe(existsSync(datei), "bild: Bild in der Kopie vorhanden", datei)) return;
  await cdp.oeffnen(`/projects/${projekt}/zeitachse`);
  const src = () => document.querySelector('img[alt^="Skizze: Verlauf"]')?.getAttribute("src") ?? null;
  const vorher = await cdp.rufe(src);
  pruefe(vorher !== null && !vorher.includes("?v="), "bild: Bild ohne Version in der Adresse", String(vorher));
  const marke = await cdp.markeSetzen();
  // Asset-Version je Datei: eine andere Datei in assets/ lässt das Bild in Ruhe
  // (der Beobachter schaut alle 2 s, der Melder bündelt 250 ms).
  writeFileSync(
    join(dirname(datei), "live-probe-andere.svg"),
    `<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><title>andere</title></svg>\n`,
  );
  await new Promise((r) => setTimeout(r, 3500));
  const unberuehrt = await cdp.rufe(src);
  pruefe(unberuehrt === vorher, "bild: eine andere Datei in assets/ ändert die Adresse nicht", String(unberuehrt));
  const merkmal = `live-probe-${Date.now()}`;
  writeFileSync(
    datei,
    `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="80" viewBox="0 0 320 80"><title>${merkmal}</title><rect width="320" height="80" fill="#2a7"/></svg>\n`,
  );
  const { wert, ms } = await cdp.warteBis((alt) => {
    const s = document.querySelector('img[alt^="Skizze: Verlauf"]')?.getAttribute("src") ?? null;
    return s !== null && s !== alt && s.includes("?v=") ? s : null;
  }, vorher);
  pruefe(ms !== null, "bild: die Adresse bekommt eine neue Version", `${wert}${ms !== null ? `, nach ${ms} ms` : ""}`);
  if (ms !== null) {
    const inhalt = await cdp.rufe((s) => fetch(s).then((r) => r.text()), wert);
    pruefe(inhalt.includes(merkmal), "bild: unter der neuen Adresse liegt das neue Bild");
    const breite = await cdp.warteBis(() => {
      const img = document.querySelector('img[alt^="Skizze: Verlauf"]');
      return img && img.complete && img.naturalWidth === 320 ? img.naturalWidth : null;
    });
    pruefe(breite.ms !== null, "bild: der Browser zeigt das neue Bild (320 px breit)", String(breite.wert));
    messwerte.push(`bild: ${ms} ms`);
  }
  pruefe(await cdp.markeDa(marke), "bild: die Seite hat nicht neu geladen");
}

// ----- Ablauf -----

function datenKopieren() {
  const liste = execFileSync("git", ["-C", INHALT.wurzel, "ls-files", "-z", "--", "data"], { encoding: "utf8", windowsHide: true })
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

async function main() {
  if (!existsSync(EXE)) throw new Error(`Programm fehlt: ${EXE}`);
  if (!existsSync(join(WEB, "index.html"))) throw new Error(`Oberfläche fehlt: ${WEB}`);
  await portePruefen(PORT);
  mkdirSync(join(ordner, "bin"), { recursive: true });
  mkdirSync(join(ordner, "profil"), { recursive: true });
  copyFileSync(EXE, join(ordner, "bin", "dashboard.exe"));
  cpSync(INHALT.projects, join(ordner, "repo", "projects"), { recursive: true });
  for (const d of ["entscheidungen", "zielbild"]) {
    const q = d === "entscheidungen" ? INHALT.entscheidungen : join(REPO, "docs", d);
    if (existsSync(q)) cpSync(q, join(ordner, "repo", "docs", d), { recursive: true });
  }
  mkdirSync(join(ordner, "repo", "docs", "entscheidungen"), { recursive: true });
  datenKopieren();
  console.log(`Temp-Ordner: ${ordner}`);

  await serverStarten(eigeneSid());
  const cdp = await edgeStarten();

  await datenPruefen(cdp);
  await entscheidungenPruefen(cdp);
  await pcPruefen(cdp);
  await bildPruefen(cdp);

  console.log(`\n${cdp.browserSperre.bericht()}`);
  pruefe(cdp.gesperrt.length === 0, "Netzsperre: die Seiten fragen nur beim eigenen Server an", cdp.gesperrt.slice(0, 5).join(" | "));
  console.log("\nMesswerte (Änderung am Server bis sichtbar):");
  for (const m of messwerte) console.log(`  ${m}`);
  console.log(`\nBestanden: ${bestanden} Prüfungen. Nicht bestanden: ${fehlgeschlagen.length}.`);
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
