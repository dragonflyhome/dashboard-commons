#!/usr/bin/env node
/**
 * Messung und Abnahme des Baumeisters (Etappe 4b) mit Edge.
 *
 * Alles in einer Wegwerf-Kopie im Temp-Ordner, nie im Repo und nie gegen den
 * echten Dienst: Kopie von web\ (ohne node_modules, build, builds; die Module
 * als Junction auf web\node_modules), von projects\ und den TOML-Dateien aus
 * data\. Der Server läuft auf Port 8788 im Altweg mit
 * DASHBOARD_BAUMEISTER=direkt (baut ohne Boten im eigenen Prozess, gleicher
 * Ablauf wie im Boten). Edge headless mit Wegwerf-Profil und Netzsperre wie
 * in den anderen Proben (edge-gemeinsam.mjs); alle Kindprozesse ohne Fenster.
 * Vor dem Start prüft die Probe Port 8788 und beide Edge-Debug-Ports.
 *
 * Ablauf:
 *   1. Aufwärmen: eine Änderung, der erste Bau prüft alle Projekte.
 *   2. Messung ESLint je Projektordner mit --cache (Median aus 5, mit Cache
 *      und nach einer Änderung).
 *   3. Messung echter Lauf: Seite.svelte speichern bis die neue Seite in
 *      Edge zu sehen ist (Median aus 5); dazu die Schritte des Baus aus dem
 *      Protokoll des Servers. Gemessen wird am eigenen, erfundenen Projekt
 *      „zz-messung“, nie an einem echten: Die Probe legt es in der Kopie an
 *      (projekt.toml wie jedes Projekt) und dazu eine minimale Seite.svelte
 *      mit ihrem Messfeld. Ohne Seite.svelte stünde der Standardaufbau da,
 *      und der kommt nie aus dem Bau (die Bauliste nimmt nur Projekte mit
 *      Seite.svelte); gemessen werden soll aber der Weg Speichern, Prüfen,
 *      Bauen, neue Seite. Messfeld und Text gehören der Probe, darum hängt
 *      die Messung nicht daran, was eine echte Seite zeigt.
 *   4. Abnahme: ein absichtlich kaputtes Projekt, einmal mit Svelte-Fehler
 *      (bleibt in der letzten grünen Fassung), einmal mit Fehler beim Zeichnen
 *      (Fehlergrenze mit „neu versuchen“), dazu ein Fehler beim Klicken
 *      (Rahmen fängt ihn); alle anderen Seiten laufen weiter.
 *   5. Neue Fassung im offenen Tab (Nachtrag,
 *      `#lib/fassung.svelte.ts`): ein zweites Edge meldet sich bei
 *      localhost bei Windows an (Live-Kanal mit Ereignis `fassung`). a) Nach
 *      dem Bau steht am Live-Punkt der Knopf „Daten live, App wird gleich
 *      aktualisiert“ (mindestens 44 px, einmal angesagt, nicht auf der
 *      Seite, bei 360 px ohne seitliches Scrollen); ohne Klick lädt der Tab
 *      nach etwa 30 s von selbst die neue Fassung. b) Mit Fokus in einem Feld
 *      lädt er auch nach 30 s nicht, der Knopf sagt „App wird aktualisiert,
 *      sobald du fertig bist“ (einmal angesagt), das Feld behält seinen Text;
 *      ein Klick auf den Knopf lädt dann nicht (Fokus zurück ins Feld), Text
 *      ohne Fokus und ein offener Dialog halten ebenso auf; ist nichts mehr
 *      offen, lädt der Tab kurz danach von selbst. c2) Zustand 1 nach etwa
 *      8 s, sonst ein HINWEIS (bekannte Grenze).
 *      c) Ohne Live-Kanal (gesperrt): Am Live-Punkt steht „Keine Verbindung
 *      zum Dienst, Daten können veraltet sein“ (kein Knopf), ein neuer Bau
 *      lädt nichts neu. Nach einem Bau, der die Seite eines anderen Projekts
 *      ändert, holt der Klick dorthin einen Teil des alten Baus (404); die
 *      Seite lädt einmal neu statt des Fehlerbilds.
 *   6. Bau-Stand (Etappe 4c; nur die
 *      Statuszeile, ohne Zeitzeile und ohne Kreispfeil zum
 *      Bauen), mit eigenem Probe-Projekt:
 *      a) Angemeldet (localhost): Kopf grün „Seite in Ordnung“, keine
 *      Zeitzeile, kein Knopf im Kopf, keine Ansage-Region; bei 360 px ohne
 *      seitliches Scrollen. Bau-Tempo, Zweig C: Während eines Baus steht
 *      „prüft Projekt x von y“ im Kopf, keine Live-Region nennt den Schritt.
 *      b) Absichtlich kaputte Seite: rot mit „zeigt die letzte gute Fassung
 *      von …“, Satz ohne Pfade, die alte Seite bleibt, Marke in der Leiste;
 *      keine Zeitzeile, kein Knopf (auch bei 360 px).
 *      c) Ohne Anmeldung (127.0.0.1): derselbe Stand ohne Satz und ohne
 *      Knopf, bei rot ohne Zeile „Seite wurde … generiert“; Karte und Zählzeile
 *      der Startseite; /api/bauen 401. Zweig C: Während des Baus aus a) kein
 *      Fortschritt in /api/events/frei und /api/projects.
 *      d) Abnahme 4c: Eine Seite, die nur die Gesamtprüfung bricht, wird aus
 *      ihrer grünen Kopie gebaut und zeigt ihre alte Fassung.
 *      e) Dasselbe mit dem geänderten Projekt vorn im Alphabet: Es trifft das
 *      geänderte, das unveränderte bleibt grün.
 *
 * Aufruf aus dem Repo (vorher `cargo build` und eine fertige Oberfläche):
 *
 *     node tests\skripte\baumeister-edge.mjs --web <Ordner mit index.html>
 *
 * Optional: --exe <dashboard.exe>, --edge <msedge.exe>, --port 8788,
 * --runden 5, --projekt <slug> (misst an einem echten Projekt der Kopie: hängt
 * einen Absatz an dessen Seite.svelte; ohne die Option das eigene Projekt
 * „zz-messung“), --behalten, --nur fassung (nur
 * Aufwärmen und Schritt 5), --nur knopf oder --nur bau (nur Aufwärmen und
 * Schritt 6; „knopf“ ist der alte Name aus der Zeit des Kreispfeils).
 * Exit 0: alles bestanden.
 */
import { spawn, spawnSync, execFileSync } from "node:child_process";
import {
  copyFileSync,
  cpSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  utimesSync,
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
const EXE = resolve(argument("exe", join(REPO, "target", "debug", "dashboard.exe")));
const WEB = resolve(argument("web", join(REPO, "web", "build")));
const EDGE = argument("edge", "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe");
const RUNDEN = Number(argument("runden", "5"));
// Ohne --projekt: das eigene, erfundene Projekt (in der Kopie angelegt, nie ein echtes).
const EIGENES = "zz-messung";
const MESS_PROJEKT = argument("projekt", null) ?? EIGENES;
const MESS_EIGENES = MESS_PROJEKT === EIGENES;
/** Das zweite eigene Projekt für den Klick in 5c (statt eines echten). */
const EIGENES_B = "zz-messung-b";
const BEHALTEN = process.argv.includes("--behalten");
const NUR = argument("nur", "");
const DEBUG_PORT = PORT + 1000;
const BASIS = `http://127.0.0.1:${PORT}`;
/** Schritt 5: über localhost, dort meldet Edge sich bei Windows an (Live-Kanal). */
const BASIS_ANGEMELDET = `http://localhost:${PORT}`;
const SYSTEM = (name) => join(process.env.SystemRoot ?? "C:\\Windows", "System32", name);
const PROBE = "zz-baumeister-probe";
const KOPF = 'schema = 3\ntitle = "Baumeister-Probe"\nstatus = "aktiv"\nsummary = "Absichtlich kaputt für die Abnahme 4b."\n';
const WORTE = ["eins", "zwei", "drei", "vier", "fuenf", "sechs", "sieben", "acht", "neun", "zehn"];

const ordner = mkdtempSync(join(tmpdir(), "dashboard-baumeister-e2e-"));
const repo = join(ordner, "repo");
const prozesse = [];
let bestanden = 0;
const fehlgeschlagen = [];
const messwerte = [];
// Ein Aufräumen für alle Wege (finally, Strg+C, unbehandelter Fehler).
const aufraeumen = beiAbbruch(aufraeumenJetzt);

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
const messen = (was, wert) => messwerte.push(`${was}: ${wert}`);
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
const median = (v) => {
  const s = [...v].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const sek = (ms) => `${(ms / 1000).toFixed(1)} s`;

async function aufraeumenJetzt() {
  try {
    const v = await mitFrist(fetch(`http://127.0.0.1:${DEBUG_PORT}/json/version`, { signal: AbortSignal.timeout(3000) }).then((r) => r.json()), 4000, "/json/version");
    const b = await mitFrist(Cdp.verbinden(v.webSocketDebuggerUrl), 3000, "Verbinden");
    await Promise.race([b.senden("Browser.close", {}, 3000).catch(() => {}), pause(3000)]);
  } catch {
    // Edge läuft nicht (mehr)
  }
  for (const p of prozesse) {
    try {
      execFileSync(SYSTEM("taskkill.exe"), ["/PID", String(p.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
    } catch {
      // schon beendet
    }
  }
  prozesse.length = 0;
  // Nur Prozesse, deren Befehlszeile den eigenen Temp-Ordner nennt.
  try {
    const muster = ordner.replace(/'/g, "''");
    execFileSync(
      SYSTEM("WindowsPowerShell\\v1.0\\powershell.exe"),
      ["-NoProfile", "-Command", `Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains('${muster}') } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`],
      { stdio: "ignore", windowsHide: true },
    );
  } catch {
    // nichts mehr da
  }
  if (!BEHALTEN) {
    // Erst die Junction lösen (nur sie), dann den Rest.
    try {
      rmSync(join(repo, "web", "node_modules"), { recursive: false, force: true });
    } catch {
      // schon weg
    }
    if (!existsSync(join(repo, "web", "node_modules"))) {
      for (let i = 0; i < 10; i++) {
        try {
          rmSync(ordner, { recursive: true, force: true });
          break;
        } catch {
          Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1000);
        }
      }
    }
  } else {
    console.log(`Temp-Ordner bleibt: ${ordner}`);
  }
}

// ----- Kopie -----

const OHNE = /[\\/](node_modules|target|\.git|build|builds|\.projekte-gruen|\.baumeister)([\\/]|$)/;

function kopieAnlegen() {
  mkdirSync(join(repo, "web"), { recursive: true });
  for (const e of readdirSync(join(REPO, "web"))) {
    if (["node_modules", "build", "builds", ".projekte-gruen", ".baumeister"].includes(e)) continue;
    cpSync(join(REPO, "web", e), join(repo, "web", e), { recursive: true, filter: (q) => !OHNE.test(q.slice(join(REPO, "web").length)) });
  }
  symlinkSync(join(REPO, "web", "node_modules"), join(repo, "web", "node_modules"), "junction");
  cpSync(WEB, join(repo, "web", "build"), { recursive: true });
  cpSync(INHALT.projects, join(repo, "projects"), { recursive: true, filter: (q) => !OHNE.test(q.slice(INHALT.projects.length)) });
  mkdirSync(join(repo, "data"), { recursive: true });
  for (const d of ["pc.toml", "schluessel.toml", "schwerpunkte.toml"]) {
    if (existsSync(join(INHALT.data, d))) copyFileSync(join(INHALT.data, d), join(repo, "data", d));
  }
  mkdirSync(join(ordner, "bin"), { recursive: true });
  mkdirSync(join(ordner, "profil"), { recursive: true });
  mkdirSync(join(ordner, "daten"), { recursive: true });
  copyFileSync(EXE, join(ordner, "bin", "dashboard.exe"));
  if (MESS_EIGENES) eigenesProjektAnlegen(EIGENES, "Baumeister-Messung");
  eigenesProjektAnlegen(EIGENES_B, "Baumeister-Messung B");
}

/**
 * Das Messfeld der Probe: eine minimale Seite.svelte, deren einziger Inhalt
 * der Text der Probe ist. Erfunden, unabhängig von jeder echten Seite.
 */
const messSeite = (text) => `<script lang="ts">\n\tconst text: string = '${text}';\n</script>\n\n<p data-messfeld>{text}</p>\n`;

/** Eigenes Projekt in der Kopie: projekt.toml wie jedes Projekt, dazu die minimale Seite. */
function eigenesProjektAnlegen(slug, titel) {
  const p = join(repo, "projects", slug);
  if (existsSync(p)) throw new Error(`projects\\${slug} gibt es im Inhalt schon; die Probe misst nur an ihrem eigenen Projekt.`);
  mkdirSync(p, { recursive: true });
  writeFileSync(
    join(p, "projekt.toml"),
    `schema = 3\ntitle = "${titel}"\nstatus = "aktiv"\nsummary = "Erfundenes Projekt der Baumeister-Probe: misst Speichern bis zur neuen Seite."\n`,
  );
  writeFileSync(join(p, "Seite.svelte"), messSeite(`${titel} bereit`));
}

/**
 * Mit --projekt: warum das Messfeld auf der echten Seite nicht zu sehen ist
 * (statt nur „nicht erreicht“).
 */
function feldFehlt(slug) {
  const ohne = "Ohne --projekt misst die Probe an ihrem eigenen Projekt.";
  try {
    const p = JSON.parse(readFileSync(join(repo, "web", ".projekte-gruen", "stand.json"), "utf8")).projekte?.[slug];
    if (p?.gruen === false) return `Messfeld auf /projects/${slug} nicht zu sehen: Die Seite ist im Bau rot und wird ausgelassen (${String(p.satz ?? "").slice(0, 160)}). ${ohne}`;
  } catch {
    // kein Stand
  }
  return `Messfeld auf /projects/${slug} nicht zu sehen: Die Seite zeigt den angehängten Absatz nicht an (etwa weil sie nur Bausteine zeigt). ${ohne}`;
}

/** Schreibt das Messfeld mit `text`: eigenes Projekt ganz, echtes mit angehängtem Absatz. */
const feldSchreiben = (original, text) => writeFileSync(seite(MESS_PROJEKT), MESS_EIGENES ? messSeite(text) : `${original}\n<p>${text}</p>\n`);

const protokoll = [];

async function serverStarten() {
  const kind = spawn(join(ordner, "bin", "dashboard.exe"), [], {
    env: {
      ...process.env,
      DASHBOARD_PORT: String(PORT),
      DASHBOARD_PROJECTS_DIR: join(repo, "projects"),
      DASHBOARD_WEB_DIR: join(repo, "web", "build"),
      DASHBOARD_DATEN: join(ordner, "daten", "dashboard.redb"),
      DASHBOARD_PC_DATA: join(repo, "data", "pc.toml"),
      DASHBOARD_KEYS_DATA: join(repo, "data", "schluessel.toml"),
      DASHBOARD_JWT_SCHLUESSEL: join(ordner, "daten", "jwt-schluessel"),
      DASHBOARD_BAUMEISTER: "direkt",
      USERPROFILE: join(ordner, "profil"),
      RUST_LOG: "info",
      NO_COLOR: "1",
    },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  const datei = createWriteStream(join(ordner, "server.log"));
  for (const s of [kind.stdout, kind.stderr]) {
    s.on("data", (d) => {
      datei.write(d);
      for (const z of d.toString().split(/\r?\n/)) if (z.includes("Baumeister") || z.includes("Bau")) protokoll.push(z);
    });
  }
  prozesse.push(kind);
  for (let i = 0; i < 150; i++) {
    try {
      if ((await fetch(`${BASIS}/healthz`)).ok) return;
    } catch {
      // noch nicht da
    }
    if (kind.exitCode !== null) throw new Error("Server beendet (server.log)");
    await pause(200);
  }
  throw new Error("Server antwortet nicht");
}

const zeiger = () => {
  try {
    return readFileSync(join(repo, "web", "builds", "aktuell"), "utf8").trim();
  } catch {
    return null;
  }
};

/** Wartet, bis der Zeiger nicht mehr `alt` ist (ein grüner Bau). */
async function neueFassung(alt, frist = 240000) {
  const start = Date.now();
  while (Date.now() - start < frist) {
    const z = zeiger();
    if (z !== null && z !== alt) return z;
    await pause(200);
  }
  return null;
}

/**
 * Wartet, bis `stand.json` des Baumeisters `soll(projekte)` erfüllt (der
 * Zeiger allein sagt nicht, welcher Bau fertig ist: Ein Bau, den eine
 * frühere Datei angestoßen hat, kann ihn schon versetzen). Gibt die
 * Projekte zurück, oder `null` nach der Frist.
 */
async function standAbwarten(soll, frist = 240000) {
  const start = Date.now();
  while (Date.now() - start < frist) {
    try {
      const p = JSON.parse(readFileSync(join(repo, "web", ".projekte-gruen", "stand.json"), "utf8")).projekte ?? {};
      if (soll(p)) return p;
    } catch {
      // noch keiner oder gerade geschrieben
    }
    await pause(300);
  }
  return null;
}

/** Die letzte Zeile des Baumeisters mit den Schritten. */
const letzteSchritte = () => [...protokoll].reverse().find((z) => z.includes("Baumeister: Bau grün") || (z.includes("Baumeister: ") && z.includes("vite build"))) ?? "";

const seite = (slug) => join(repo, "projects", slug, "Seite.svelte");

// ----- Edge -----

class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.nr = 0;
    this.offen = new Map();
    this.hoerer = [];
    this.gesperrt = [];
    this.basis = BASIS;
    /** Schritt 5c: den Live-Kanal sperren (kein Ereignis `fassung`). */
    this.ohneLive = false;
    ws.addEventListener("message", (e) => {
      const m = JSON.parse(String(e.data));
      if (m.id !== undefined && this.offen.has(m.id)) {
        const { ok, nein } = this.offen.get(m.id);
        this.offen.delete(m.id);
        if (m.error) nein(new Error(`${m.error.message}`));
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

  async netzSperren({ anmelden = false } = {}) {
    // Der Altweg fragt auf geschützten Wegen (Live-Kanal) nach der Windows-
    // Anmeldung; ohne Antwort bliebe Edge an der Rückfrage hängen. Abbrechen,
    // wie beim freien Weg in pruefer-edge.mjs. Mit `anmelden` (Schritt 5)
    // meldet Edge sich über localhost selbst an (--auth-server-allowlist).
    this.hoerer.push((m) => {
      if (m.method === "Fetch.authRequired") {
        this.senden("Fetch.continueWithAuth", { requestId: m.params.requestId, authChallengeResponse: { response: "CancelAuth" } }).catch(() => {});
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
      if (this.ohneLive && request.url.includes("/api/events")) {
        this.senden("Fetch.failRequest", { requestId, errorReason: "ConnectionRefused" }).catch(() => {});
      } else if (host === "127.0.0.1" || host === "localhost" || host === "[::1]") {
        this.senden("Fetch.continueRequest", { requestId }).catch(() => {});
      } else {
        this.gesperrt.push(request.url);
        this.senden("Fetch.failRequest", { requestId, errorReason: "BlockedByClient" }).catch(() => {});
      }
    });
    await this.senden("Fetch.enable", { patterns: [{ urlPattern: "*" }], handleAuthRequests: !anmelden });
  }

  async rufe(fn, arg) {
    const r = await this.senden("Runtime.evaluate", { expression: `(${fn.toString()})(${JSON.stringify(arg ?? null)})`, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(`JS: ${JSON.stringify(r.exceptionDetails).slice(0, 400)}`);
    return r.result.value;
  }

  async oeffnen(pfad) {
    // Kommt das Ladeereignis nicht (etwa während die Fassung wechselt), geht es
    // trotzdem weiter: Danach wird ohnehin auf den Text gewartet.
    const geladen = this.warteAuf("Page.loadEventFired", 15000).catch(() => null);
    const t0 = Date.now();
    const antwort = await this.senden("Page.navigate", { url: `${this.basis}${pfad}` }).catch((e) => ({ errorText: String(e?.message ?? e) }));
    const ms = Date.now() - t0;
    const ereignis = await geladen;
    // Für den Hinweis, wenn eine Seite nicht erscheint (bisSichtbar).
    this.letzteNavigation = { pfad, antwortMs: ms, fehler: antwort?.errorText ?? null, geladen: ereignis !== null, gesamtMs: Date.now() - t0 };
    const ende = Date.now() + 8000;
    await pause(300);
    while (Date.now() < ende) {
      const laedt = await this.rufe(() => !document.querySelector("main") || document.querySelector('[aria-busy="true"]') !== null).catch(() => true);
      if (!laedt) break;
      await pause(150);
    }
  }

  text() {
    return this.rufe(() => document.querySelector("main")?.innerText ?? "");
  }
}

/**
 * Edge headless mit Wegwerf-Profil. `anmelden` (Schritt 5): zweites Edge auf
 * dem zweiten Debug-Port, meldet sich bei localhost bei Windows an.
 */
async function edgeStarten({ anmelden = false } = {}) {
  const debugPort = anmelden ? DEBUG_PORT + 1 : DEBUG_PORT;
  const profil = join(ordner, anmelden ? "edge-profil-angemeldet" : "edge-profil");
  mkdirSync(profil, { recursive: true });
  const edge = spawn(
    EDGE,
    [
      "--headless=new",
      `--remote-debugging-port=${debugPort}`,
      `--user-data-dir=${profil}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-sync",
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-extensions",
      "--disable-component-extensions-with-background-pages",
      ...(anmelden ? ["--auth-server-allowlist=localhost"] : []),
      "--window-size=1280,900",
      "about:blank",
    ],
    { stdio: "ignore", windowsHide: true },
  );
  prozesse.push(edge);
  let ziel = null;
  for (let i = 0; i < 100 && ziel === null; i++) {
    try {
      ziel = (await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json()).find((t) => t.type === "page") ?? null;
    } catch {
      // Edge startet noch
    }
    if (ziel === null) await pause(200);
  }
  if (ziel === null) throw new Error("Edge antwortet nicht auf dem Debug-Port");
  const cdp = await Cdp.verbinden(ziel.webSocketDebuggerUrl);
  if (anmelden) cdp.basis = BASIS_ANGEMELDET;
  await cdp.senden("Page.enable");
  await cdp.netzSperren({ anmelden });
  cdp.browserSperre = await browserSperren(debugPort, cdp.gesperrt, { ausser: ziel.id });
  await cdp.senden("Runtime.enable");
  return cdp;
}

/**
 * Öffnet `pfad`, bis `soll` im Text steht (oder die Frist um ist). Es zählt
 * nur der Text, wenn der Tab auch unter `pfad` steht: Lädt die vorige Seite
 * gerade selbst neu (neue Fassung nach einem Bau), kann das Navigieren
 * verloren gehen, und der Tab zeigt weiter die alte Seite.
 */
async function bisSichtbar(cdp, pfad, soll, frist = 30000) {
  const start = Date.now();
  let t = "";
  let dort = false;
  while (Date.now() - start < frist) {
    await cdp.oeffnen(pfad);
    const da = await cdp.rufe(() => ({ pfad: location.pathname, text: document.querySelector("main")?.innerText ?? "" })).catch(() => ({ pfad: null, text: "" }));
    t = da.text;
    dort = da.pfad === pfad;
    if (dort && soll.test(t)) return t;
    await pause(300);
  }
  if (dort) {
    console.log(`        Hinweis: ${pfad} ist offen, aber ${soll} steht nicht auf der Seite (${t.slice(0, 120).replace(/\s+/g, " ")})`);
    return t;
  }
  // Für die Fehlersuche: wo der Tab steht, wie das letzte Navigieren lief und wie schnell der Server die Seite liefert.
  const jetzt = await cdp.rufe(() => location.pathname).catch((e) => `? (${String(e?.message ?? e).slice(0, 80)})`);
  const t0 = Date.now();
  const status = await fetch(`${BASIS}${pfad}`, { signal: AbortSignal.timeout(20000) }).then((r) => r.status).catch((e) => String(e?.message ?? e));
  console.log(`        Hinweis: ${pfad} nicht erreicht; Tab auf ${jetzt}; letztes Navigieren ${JSON.stringify(cdp.letzteNavigation ?? null)}; GET ${pfad}: ${status} in ${Date.now() - t0} ms`);
  return t;
}

// ----- Messungen -----

/** Kopie des Projekts außerhalb der Quellen: Die Messung löst keinen Bau aus. */
const eslintOrt = () => join(ordner, "eslint-messung");

function eslintEinmal(slug) {
  const web = join(repo, "web");
  const start = process.hrtime.bigint();
  const r = spawnSync(
    process.execPath,
    [join(web, "node_modules", "eslint", "bin", "eslint.js"), "--config", join(web, "eslint.projekte.config.js"), "--cache", "--cache-location", join(web, "node_modules", ".cache", "eslint-baumeister-probe") + "\\", "--format", "json", "--no-warn-ignored", slug],
    { cwd: eslintOrt(), windowsHide: true, encoding: "utf8" },
  );
  const ms = Number(process.hrtime.bigint() - start) / 1e6;
  return { ms, ok: r.status === 0 };
}

async function eslintMessen() {
  console.log("\n== ESLint je Projektordner mit --cache ==");
  cpSync(join(repo, "projects", MESS_PROJEKT), join(eslintOrt(), MESS_PROJEKT), { recursive: true, filter: (q) => !OHNE.test(q) && !/[\\/]server([\\/]|$)/.test(q.slice(join(repo, "projects").length)) });
  eslintEinmal(MESS_PROJEKT); // Cache füllen
  const warm = [];
  for (let i = 0; i < 5; i++) warm.push(eslintEinmal(MESS_PROJEKT).ms);
  const geaendert = [];
  for (let i = 0; i < 5; i++) {
    const jetzt = new Date();
    utimesSync(join(eslintOrt(), MESS_PROJEKT, "Seite.svelte"), jetzt, jetzt);
    geaendert.push(eslintEinmal(MESS_PROJEKT).ms);
  }
  messen(`ESLint ${MESS_PROJEKT} mit Cache, Median aus 5`, `${sek(median(warm))} (${warm.map(sek).join(", ")})`);
  messen(`ESLint ${MESS_PROJEKT} nach Änderung einer Datei, Median aus 5`, `${sek(median(geaendert))} (${geaendert.map(sek).join(", ")})`);
  pruefe(eslintEinmal(MESS_PROJEKT).ok, `ESLint: ${MESS_PROJEKT} ist sauber`);
}

async function echtMessen(cdp) {
  console.log(`\n== Echter Lauf: Seite.svelte speichern bis zur neuen Seite (${RUNDEN}×) ==`);
  const original = readFileSync(seite(MESS_PROJEKT), "utf8");
  const zeiten = [];
  for (let i = 0; i < RUNDEN; i++) {
    const marke = `Baumeister-Messung ${WORTE[i % WORTE.length]}`;
    const alt = zeiger();
    const start = Date.now();
    feldSchreiben(original, marke);
    const neu = await neueFassung(alt);
    const fassung = Date.now() - start;
    const text = neu === null ? "" : await bisSichtbar(cdp, `/projects/${MESS_PROJEKT}`, new RegExp(marke));
    const ms = Date.now() - start;
    const ok = text.includes(marke);
    const grund = ok || neu === null || MESS_EIGENES ? "" : `; ${feldFehlt(MESS_PROJEKT)}`;
    pruefe(ok, `Messung ${i + 1}: „${marke}“ ist in Edge zu sehen`, `${sek(ms)}, Zeiger nach ${sek(fassung)}${grund}`);
    if (ok) zeiten.push(ms);
    messen(`Lauf ${i + 1}`, `${sek(ms)} (neue Fassung nach ${sek(fassung)}); ${letzteSchritte().replace(/^.*Baumeister: /, "").slice(0, 400)}`);
    await pause(1500);
  }
  writeFileSync(seite(MESS_PROJEKT), original);
  await neueFassung(zeiger());
  const m = zeiten.length ? median(zeiten) : NaN;
  messen("Speichern bis zur Seite, Median", `${sek(m)} aus ${zeiten.length} Läufen (${zeiten.map(sek).join(", ")})`);
  pruefe(zeiten.length === RUNDEN && m < 60000, "Messung: unter der Grenze von 60 s", sek(m));
  if (m >= 30000) console.log(`  HINWEIS: über dem Ziel von 30 s (${sek(m)}); die Schritte stehen bei den Läufen.`);
}

// ----- Abnahme -----

async function abnahme(cdp) {
  console.log("\n== Abnahme: kaputtes Projekt, alle anderen laufen ==");
  const p = join(repo, "projects", PROBE);
  mkdirSync(p, { recursive: true });
  writeFileSync(join(p, "projekt.toml"), KOPF);
  const gut = '<script lang="ts">\n\tconst satz: string = \'Die Probe ist gut.\';\n</script>\n\n<p>{satz}</p>\n';
  let alt = zeiger();
  writeFileSync(join(p, "Seite.svelte"), gut);
  pruefe((await neueFassung(alt)) !== null, "Probe-Projekt neu: neue Fassung");
  let t = await bisSichtbar(cdp, `/projects/${PROBE}`, /Die Probe ist gut/);
  pruefe(t.includes("Die Probe ist gut."), "Probe-Projekt erscheint grün", t.slice(0, 120));

  // a) Svelte-Fehler: bleibt in der letzten grünen Fassung.
  alt = zeiger();
  writeFileSync(join(p, "Seite.svelte"), "<script lang=\"ts\">\n\tconst satz: string = 'Kaputt';\n</script>\n\n<p>{satz</p>\n");
  const nachKaputt = await neueFassung(alt);
  pruefe(nachKaputt !== null, "Svelte-Fehler: der Bau läuft trotzdem durch (die anderen Seiten sind neu gebaut)");
  const stand = JSON.parse(readFileSync(join(repo, "web", ".projekte-gruen", "stand.json"), "utf8"));
  const eintrag = stand.projekte?.[PROBE];
  pruefe(eintrag && eintrag.gruen === false && /letzten guten Fassung/.test(eintrag.satz), "Svelte-Fehler: das Projekt ist rot, mit Satz in einfachem Deutsch", eintrag?.satz?.slice(0, 200) ?? "");
  const liste = readFileSync(join(repo, "web", "src", "lib", "projekte.generated.ts"), "utf8");
  pruefe(liste.includes(`.projekte-gruen/${PROBE}/Seite.svelte`) && !liste.includes("import.meta"), "Svelte-Fehler: gebaut aus der letzten grünen Kopie (Bauliste)");
  t = await bisSichtbar(cdp, `/projects/${PROBE}`, /Die Probe ist gut/);
  pruefe(t.includes("Die Probe ist gut.") && !t.includes("hat gerade einen Fehler"), "Svelte-Fehler: du siehst die letzte grüne Fassung", t.slice(0, 120));
  await andereLaufen(cdp, "Svelte-Fehler");
  const rot = protokoll.find((z) => z.includes(`${PROBE} ist rot`));
  pruefe(rot !== undefined, "Svelte-Fehler: Ereignis bau/rot gemeldet (Protokoll)", (rot ?? "").replace(/^.*Bau: /, "").slice(0, 200));

  // b) Fehler beim Zeichnen: grün geprüft, die Fehlergrenze fängt ihn.
  alt = zeiger();
  writeFileSync(join(p, "Seite.svelte"), "<script lang=\"ts\">\n\tfunction kaputt(): string {\n\t\tthrow new Error('Probe: Fehler beim Zeichnen');\n\t}\n</script>\n\n<p>{kaputt()}</p>\n");
  pruefe((await neueFassung(alt)) !== null, "Fehler beim Zeichnen: neue Fassung");
  t = await bisSichtbar(cdp, `/projects/${PROBE}`, /hat gerade einen Fehler/);
  const knopf = await cdp.rufe(() => [...document.querySelectorAll("button")].some((b) => b.textContent.trim() === "neu versuchen"));
  const kopf = await cdp.rufe(() => document.querySelector("h1")?.textContent ?? "");
  pruefe(t.includes("Diese Seite hat gerade einen Fehler") && knopf, "Fehler beim Zeichnen: Fehlergrenze mit „neu versuchen“", t.slice(0, 160));
  pruefe(/Baumeister-Probe/.test(kopf), "Fehler beim Zeichnen: Kopf des Projekts bleibt", kopf);
  await andereLaufen(cdp, "Fehler beim Zeichnen");

  // c) Fehler beim Klicken: der Rahmen fängt ihn am Fenster.
  alt = zeiger();
  writeFileSync(join(p, "Seite.svelte"), "<script lang=\"ts\">\n\tfunction klick(): void {\n\t\tthrow new Error('Probe: Fehler beim Klicken');\n\t}\n</script>\n\n<button type=\"button\" onclick={klick}>Probe klicken</button>\n");
  pruefe((await neueFassung(alt)) !== null, "Fehler beim Klicken: neue Fassung");
  // Erst muss die Seite mit dem Knopf da sein (sonst klickte die Probe ins Leere
  // und läse die Seite, auf der sie gerade steht); dann nach dem Klick bis zu 5 s warten.
  const mitKnopf = await bisSichtbar(cdp, `/projects/${PROBE}`, /Probe klicken/, 60000);
  pruefe(mitKnopf.includes("Probe klicken"), "Fehler beim Klicken: die Seite mit dem Knopf erscheint", mitKnopf.slice(0, 120));
  await cdp.senden("Runtime.evaluate", { expression: "setTimeout(() => [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Probe klicken')?.click(), 0)" });
  for (let i = 0; i < 25; i++) {
    await pause(200);
    t = await cdp.text().catch(() => "");
    if (t.includes("Bei einer Bedienung")) break;
  }
  pruefe(t.includes("Diese Seite hat gerade einen Fehler") && t.includes("Bei einer Bedienung"), "Fehler beim Klicken: dieselbe Meldung an Stelle der Seite", t.slice(0, 160));
  await andereLaufen(cdp, "Fehler beim Klicken");
  pruefe(cdp.gesperrt.length === 0, "Netzsperre: keine Anfrage nach außen", cdp.gesperrt.slice(0, 3).join(", "));
}

/** Startseite und eine andere Projektseite laufen weiter. */
async function andereLaufen(cdp, wann) {
  const t = await bisSichtbar(cdp, `/projects/${MESS_PROJEKT}`, /\S/);
  pruefe(t.length > 40 && !t.includes("hat gerade einen Fehler"), `${wann}: /projects/${MESS_PROJEKT} läuft`, t.slice(0, 80).replace(/\s+/g, " "));
  await cdp.oeffnen("/");
  const start = await cdp.text();
  pruefe(start.length > 40 && !start.includes("hat gerade einen Fehler"), `${wann}: die Startseite läuft`);
}

// ----- Neue Fassung im offenen Tab (Schritt 5) -----

/** Fragt `fn` im Tab, bis es wahr ist (oder die Frist um ist); während eines Neuladens weiter. */
async function bisWahr(cdp, fn, arg, frist = 30000) {
  const ende = Date.now() + frist;
  while (Date.now() < ende) {
    if (await cdp.rufe(fn, arg).catch(() => false)) return true;
    await pause(200);
  }
  return false;
}

/** Was der Tab gerade zeigt: Adresse, Marke, Hinweis, Fehlerbild, Merker. */
function tabStand(cdp) {
  return cdp.rufe(() => {
    const main = document.querySelector("main")?.innerText ?? "";
    let merker = null;
    try {
      merker = sessionStorage.getItem("dashboard:fassung-neu-geladen");
    } catch {
      // ohne Speicher
    }
    return {
      pfad: location.pathname,
      marke: window.__fassungProbe ?? null,
      hinweis: [...document.querySelectorAll('[role="status"]')].map((e) => e.textContent.trim()).filter((t) => t !== "").join(" | "),
      fehler: /hat gerade einen Fehler|ließ sich nicht laden|Das hat nicht geklappt|Diese Seite gibt es nicht/.test(main),
      merker,
      main: main.slice(0, 160).replace(/\s+/g, " "),
    };
  });
}

/** Die Zustände am Live-Punkt (`LiveBadge.svelte`). */
const HINWEIS = "Daten live, App wird gleich aktualisiert";
const HINWEIS_AUFGEHALTEN = "App wird aktualisiert, sobald du fertig bist";
const KEINE_VERBINDUNG = "Keine Verbindung zum Dienst, Daten können veraltet sein";
/** Der Knopf in Zustand 2 (App veraltet). */
const KNOPF = 'header button.live[data-lage="veraltet"]';

/** Was der Knopf am Live-Punkt gerade zeigt (oder `da: false`). */
function hinweisStand(cdp) {
  return cdp.rufe(() => {
    const k = document.querySelector('header button.live[data-lage="veraltet"]');
    const r = k?.getBoundingClientRect();
    return {
      da: k !== null,
      text: k?.textContent?.replace(/\s+/g, " ").trim() ?? "",
      name: k === null ? "" : (k.getAttribute("aria-label") ?? k.textContent?.replace(/\s+/g, " ").trim() ?? ""),
      ansage: k?.closest(".ort")?.querySelector('[role="status"]')?.textContent?.trim() ?? "",
      imMain: [...document.querySelectorAll("main")].some((m) => m.innerText.includes("App wird")),
      hoehe: r === undefined ? 0 : Math.round(r.height),
      breite: r === undefined ? 0 : Math.round(r.width),
      rechts: r === undefined ? 0 : Math.round(r.right),
      breit: document.documentElement.scrollWidth,
      marke: window.__fassungProbe ?? null,
    };
  });
}

/** Der Hinweis bei 360 px Breite (wie ein Telefon), dann wieder die volle Breite. */
async function hinweisSchmal(cdp) {
  await cdp.senden("Emulation.setDeviceMetricsOverride", { width: 360, height: 800, deviceScaleFactor: 1, mobile: true, screenWidth: 360, screenHeight: 800 });
  await pause(400);
  const h = await hinweisStand(cdp);
  await cdp.senden("Emulation.clearDeviceMetricsOverride");
  await pause(300);
  return h;
}

/** Marke im Tab setzen: Ist sie nach dem Bau weg, hat der Tab ganz neu geladen. */
const markieren = (cdp) => cdp.rufe(() => (window.__fassungProbe = "alt"));

/** Das zweite Edge (angemeldet über localhost), einmal für Schritt 5 und 6. */
let angemeldet = null;
async function angemeldetesEdge() {
  angemeldet ??= await edgeStarten({ anmelden: true });
  return angemeldet;
}

async function fassungPruefen() {
  console.log("\n== Neue Fassung im offenen Tab (angemeldet über localhost) ==");
  const cdp = await angemeldetesEdge();
  const original = readFileSync(seite(MESS_PROJEKT), "utf8");
  const pfad = `/projects/${MESS_PROJEKT}`;
  // Seit Kopfleiste 2 trägt der Live-Punkt seinen Zustand als data-zustand.
  const live = () =>
    bisWahr(cdp, () => document.querySelector('.live[data-zustand="angemeldet"]') !== null, null, 15000);
  const leben = (satz) => `<p>Fassungsprobe ${satz}</p>`;
  const fassungSchreiben = (satz) => feldSchreiben(original, `Fassungsprobe ${satz}`);
  // Konsolenfehler und unbehandelte Ausnahmen im Tab während des ganzen Schritts.
  const konsole = [];
  const konsoleHoeren = (m) => {
    if (m.method === "Runtime.exceptionThrown") konsole.push(m.params.exceptionDetails?.exception?.description ?? m.params.exceptionDetails?.text ?? "Ausnahme");
    if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") konsole.push(m.params.args?.map((a) => a.value ?? a.description ?? "").join(" ") ?? "Fehler");
  };
  cdp.hoerer.push(konsoleHoeren);

  // a) Keine Eingabe offen: erst der Hinweis am
  // Live-Punkt, ohne Klick lädt der Tab nach etwa 30 s von selbst.
  await cdp.oeffnen(pfad);
  pruefe(await live(), "5a: Live-Kanal angemeldet verbunden");
  await markieren(cdp);
  let alt = zeiger();
  let start = Date.now();
  fassungSchreiben("eins");
  let neu = await neueFassung(alt);
  const nachZeiger = Date.now() - start;
  let ok = neu !== null && (await bisWahr(cdp, (sel) => document.querySelector(sel) !== null, KNOPF, 20000));
  const erschienen = Date.now();
  let h = await hinweisStand(cdp);
  pruefe(ok && h.text === HINWEIS && h.name === HINWEIS && h.marke === "alt", "5a: am Live-Punkt der Knopf „Daten live, App wird gleich aktualisiert“, der Tab lädt noch nicht", JSON.stringify(h));
  pruefe(h.hoehe >= 44 && h.breite >= 44, "5a: der Knopf ist mindestens 44 px groß", JSON.stringify({ hoehe: h.hoehe, breite: h.breite }));
  pruefe(h.ansage === HINWEIS && !h.imMain, "5a: einmal höflich angesagt (role=status am Live-Punkt), nicht auf der Seite selbst", JSON.stringify({ ansage: h.ansage, imMain: h.imMain }));
  const eng = await hinweisSchmal(cdp);
  pruefe(eng.da && eng.breit <= 360 && eng.rechts <= 360 && eng.hoehe >= 44 && eng.breite >= 44, "5a: bei 360 px kein seitliches Scrollen, Knopf ganz zu sehen und mindestens 44 px", JSON.stringify(eng));
  ok = await bisWahr(cdp, () => window.__fassungProbe === undefined && (document.querySelector("main")?.innerText ?? "").includes("Fassungsprobe eins"), null, 50000);
  const nachHinweis = Date.now() - erschienen;
  let s = await tabStand(cdp);
  const grund5a = ok || neu === null || MESS_EIGENES ? "" : `; ${feldFehlt(MESS_PROJEKT)}`;
  pruefe(ok && s.pfad === pfad && !s.fehler && s.hinweis === "", "5a: ohne Klick lädt der Tab die neue Fassung von selbst, ohne Fehlerbild, danach kein Hinweis", `${sek(Date.now() - start)} ab Speichern, Zeiger nach ${sek(nachZeiger)}; ${JSON.stringify(s)}${grund5a}`);
  pruefe(ok && nachHinweis >= 25000 && nachHinweis <= 45000, "5a: von selbst erst nach etwa 30 s (nicht sofort)", `${sek(nachHinweis)} nach dem Hinweis`);
  messen("5a Hinweis bis zum Neuladen von selbst", sek(nachHinweis));

  // b) Sperrgründe (Grundsatz der Abnahme: NIE neu laden, solange Eingabe, Dialog oder
  // Freigabe offen ist): Text mit Fokus, Klick im Zustand „aufgehalten“, Text ohne Fokus,
  // offener Dialog; erst wenn nichts mehr offen ist, lädt der Tab von selbst.
  await cdp.oeffnen(pfad);
  await live();
  await markieren(cdp);
  await cdp.rufe(() => {
    const feld = document.createElement("textarea");
    feld.id = "fassung-probe-feld";
    feld.value = "halb getippt";
    document.querySelector("main")?.prepend(feld);
    feld.focus();
    return document.activeElement === feld;
  });
  alt = zeiger();
  fassungSchreiben("zwei");
  neu = await neueFassung(alt);
  ok = neu !== null && (await bisWahr(cdp, (sel) => document.querySelector(sel) !== null, KNOPF, 20000));
  await pause(35000);
  s = await tabStand(cdp);
  const feldDa = await cdp.rufe(() => document.getElementById("fassung-probe-feld")?.value ?? null);
  const fokusDa = await cdp.rufe(() => document.activeElement?.id === "fassung-probe-feld");
  pruefe(ok && s.marke === "alt" && feldDa === "halb getippt" && fokusDa && !s.fehler, "5b: Eingabe offen: auch nach 35 s kein Neuladen, das Feld behält Text und Fokus", JSON.stringify({ ...s, fokusDa }));
  h = await hinweisStand(cdp);
  pruefe(h.da && h.text === HINWEIS_AUFGEHALTEN, "5b: der Knopf sagt „App wird aktualisiert, sobald du fertig bist“", JSON.stringify({ text: h.text, ansage: h.ansage }));
  pruefe(h.ansage === HINWEIS_AUFGEHALTEN, "5b: das Umschalten auf „sobald du fertig bist“ ist einmal angesagt", JSON.stringify({ ansage: h.ansage }));
  // Klick im Zustand „aufgehalten“ (Muss 1): Der Fokus wandert zum Knopf, der Text im Feld zählt trotzdem.
  await cdp.rufe((sel) => {
    const k = document.querySelector(sel);
    k?.focus();
    k?.click();
  }, KNOPF);
  await pause(3000);
  s = await tabStand(cdp);
  const nachKlick = await cdp.rufe(() => ({ wert: document.getElementById("fassung-probe-feld")?.value ?? null, fokus: document.activeElement?.id ?? "" }));
  pruefe(s.marke === "alt" && nachKlick.wert === "halb getippt" && !s.fehler, "5b: Klick im Zustand „aufgehalten“ lädt nicht, das Feld behält seinen Text", JSON.stringify({ ...s, ...nachKlick }));
  pruefe(nachKlick.fokus === "fassung-probe-feld", "5b: nach dem Klick steht der Fokus wieder im Feld", JSON.stringify(nachKlick));
  // Text ohne Fokus: weiter kein Neuladen (länger als die kurze Frist nach einem Sperrgrund).
  await cdp.rufe(() => document.getElementById("fassung-probe-feld")?.blur());
  await pause(10000);
  s = await tabStand(cdp);
  pruefe(s.marke === "alt" && !s.fehler, "5b: Text im Feld ohne Fokus: auch nach 10 s kein Neuladen", JSON.stringify(s));
  // Offener Dialog (ohne Felder), das Feld zugleich leer: weiter kein Neuladen.
  await cdp.rufe(() => {
    const d = document.createElement("dialog");
    d.id = "fassung-probe-dialog";
    d.textContent = "Probe-Dialog";
    document.body.append(d);
    d.show();
    const f = document.getElementById("fassung-probe-feld");
    if (f) f.value = "";
  });
  await pause(10000);
  s = await tabStand(cdp);
  pruefe(s.marke === "alt" && !s.fehler, "5b: offener Dialog: auch nach 10 s kein Neuladen", JSON.stringify(s));
  // Nichts mehr offen: kurz danach (NACH_SPERRE_MS) lädt der Tab von selbst.
  await cdp.rufe(() => {
    const d = document.getElementById("fassung-probe-dialog");
    d?.close();
    d?.remove();
    document.getElementById("fassung-probe-feld")?.remove();
  });
  ok = await bisWahr(cdp, () => window.__fassungProbe === undefined && (document.querySelector("main")?.innerText ?? "").includes("Fassungsprobe zwei"), null, 20000);
  await pause(800);
  s = await tabStand(cdp);
  pruefe(ok && s.pfad === pfad && !s.fehler && s.hinweis === "", "5b: ist nichts mehr offen, lädt der Tab kurz danach von selbst die neue Fassung, ohne Fehlerbild", JSON.stringify(s));

  // c) Zustand 1 „Keine Verbindung“: nie neu laden, auch nicht von selbst.
  // c1) Live-Kanal gesperrt, der Dienst läuft, ein Teil des alten Baus fehlt (404): Unser
  // Neuladen bleibt aus (kein Merker). Der Klick auf einen Link darf zum Ziel gehen (das
  // verlangt der Klick; SvelteKit sieht die neue Fassung selbst, solange der Dienst antwortet).
  // Das zweite eigene Projekt (nur Seite.svelte), nie ein echtes.
  const anderes = EIGENES_B;
  const anderesOriginal = readFileSync(seite(anderes), "utf8");
  cdp.ohneLive = true;
  await cdp.oeffnen(pfad);
  await cdp.rufe(() => sessionStorage.clear());
  await markieren(cdp);
  // Neuladen zählen und Teile des Baus, die 404 bekommen (der fehlende Teil).
  const neuGeladen = [];
  const fehlend = [];
  const zaehler = (m) => {
    if (m.method === "Page.frameNavigated" && !m.params.frame.parentId) neuGeladen.push(m.params.frame.url);
    if (m.method === "Network.responseReceived" && m.params.response.status === 404 && m.params.response.url.includes("/_app/")) fehlend.push(m.params.response.url);
  };
  cdp.hoerer.push(zaehler);
  await cdp.senden("Network.enable");
  alt = zeiger();
  writeFileSync(seite(anderes), `${anderesOriginal}\n${leben("drei")}\n`);
  neu = await neueFassung(alt);
  await pause(1500);
  s = await tabStand(cdp);
  const punkt = await cdp.rufe(() => {
    const a = document.querySelector("header a.live");
    return { text: a?.textContent?.replace(/\s+/g, " ").trim() ?? "", lage: a?.getAttribute("data-lage") ?? null, knopf: document.querySelector("header button.live") !== null, breit: document.documentElement.scrollWidth };
  });
  pruefe(neu !== null && s.marke === "alt" && s.hinweis === KEINE_VERBINDUNG, "5c: neue Fassung ohne Live-Kanal; der Tab lädt nicht neu, angesagt nur „Keine Verbindung …“", JSON.stringify(s));
  pruefe(punkt.text === KEINE_VERBINDUNG && punkt.lage === "keine_verbindung" && !punkt.knopf, "5c: am Live-Punkt „Keine Verbindung zum Dienst, Daten können veraltet sein“, als Link, kein Knopf", JSON.stringify(punkt));
  await cdp.senden("Emulation.setDeviceMetricsOverride", { width: 360, height: 800, deviceScaleFactor: 1, mobile: true, screenWidth: 360, screenHeight: 800 });
  await pause(400);
  const engPunkt = await cdp.rufe(() => ({ breit: document.documentElement.scrollWidth, rechts: Math.round(document.querySelector("header a.live")?.getBoundingClientRect().right ?? 999) }));
  await cdp.senden("Emulation.clearDeviceMetricsOverride");
  await pause(300);
  pruefe(engPunkt.breit <= 360 && engPunkt.rechts <= 360, "5c: Zustand 1 bei 360 px ohne seitliches Scrollen", JSON.stringify(engPunkt));
  neuGeladen.length = 0;
  // Klick auf einen Link zum anderen Projekt: der Router bleibt in der alten Fassung.
  await cdp.rufe((ziel) => {
    const a = document.createElement("a");
    a.href = ziel;
    a.id = "fassung-probe-link";
    a.textContent = "zum anderen Projekt";
    document.querySelector("main")?.prepend(a);
    a.click();
  }, `/projects/${anderes}`);
  await pause(5000);
  s = await tabStand(cdp);
  pruefe(fehlend.length > 0, "5c: der Klick holt einen Teil des alten Baus, der Dienst sagt 404", fehlend.slice(0, 2).join(", "));
  pruefe(s.merker === null, "5c: in Zustand 1 lädt der fehlende Teil nicht von selbst neu (kein Merker)", `${neuGeladen.length}× geladen; Merker ${s.merker}; ${JSON.stringify(s)}`);
  const zumZiel = neuGeladen.length === 1 && s.pfad === `/projects/${anderes}` && !s.fehler;
  pruefe(zumZiel || (neuGeladen.length === 0 && s.marke === "alt"), "5c: höchstens die Navigation, die der Klick verlangt (keine Schleife)", `${neuGeladen.length}× geladen; ${JSON.stringify(s)}`);

  // c2) Zustand 2, dann reißt die Verbindung ab (Edge offline, „Dienst weg“): Zustand 1,
  // die laufende 30-s-Uhr stoppt, auch nach 35 s kein Neuladen; wieder online wieder 2.
  cdp.ohneLive = false;
  await cdp.oeffnen(pfad);
  await live();
  await cdp.rufe(() => sessionStorage.clear());
  await markieren(cdp);
  alt = zeiger();
  writeFileSync(seite(anderes), `${anderesOriginal}
${leben("vier")}
`);
  neu = await neueFassung(alt);
  ok = neu !== null && (await bisWahr(cdp, (sel) => document.querySelector(sel) !== null, KNOPF, 20000));
  pruefe(ok, "5c: vor der Unterbrechung Zustand 2 (Knopf am Live-Punkt)");
  neuGeladen.length = 0;
  const netz = (offline) => cdp.senden("Network.emulateNetworkConditions", { offline, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
  const offline = Date.now();
  await netz(true);
  // Der offene Live-Strom bleibt bei der Emulation oft stehen; dann merkt die Seite
  // spätestens bei der Frage vor dem Neuladen (/api/stand), dass der Dienst nicht antwortet.
  // shortcut: Zustand 1 zeigt sich dann erst nach bis zu 30 s (bei der Frage vor dem Neuladen), nur als HINWEIS gemeldet, aufrüsten, wenn der Inhaber „Keine Verbindung“ früher sehen soll
  const frueh = await bisWahr(cdp, () => document.querySelector('header a.live[data-lage="keine_verbindung"]') !== null, null, 8000);
  const erkannt = Date.now() - offline;
  if (frueh) console.log(`  ok    5c: Zustand 1 nach ${sek(erkannt)} erkannt (Ziel etwa 8 s)`);
  else console.log("  HINWEIS (bekannte Grenze): Zustand 1 nach 8 s noch nicht erkannt; der Strom steht scheinbar, erst die Frage vor dem Neuladen (bis 30 s) merkt es.");
  await pause(Math.max(0, 35000 - (Date.now() - offline)));
  const getrennt = await cdp.rufe(() => document.querySelector('header a.live[data-lage="keine_verbindung"]') !== null);
  s = await tabStand(cdp);
  pruefe(getrennt && s.marke === "alt" && neuGeladen.length === 0 && s.hinweis.includes(KEINE_VERBINDUNG), "5c: Verbindung weg während 2: „Keine Verbindung …“, auch nach 35 s kein Neuladen", `${neuGeladen.length}× geladen; ${JSON.stringify(s)}`);
  await netz(false);
  ok = await bisWahr(cdp, (sel) => document.querySelector(sel) !== null, KNOPF, 20000);
  s = await tabStand(cdp);
  pruefe(ok && s.marke === "alt", "5c: wieder online: wieder Zustand 2, ohne Neuladen dazwischen", JSON.stringify(s));
  await cdp.rufe((sel) => document.querySelector(sel)?.click(), KNOPF);
  ok = await bisWahr(cdp, () => window.__fassungProbe === undefined && (document.querySelector("main")?.innerText ?? "").includes("Fassungsprobe zwei"), null, 15000);
  pruefe(ok, "5c: danach lädt der Klick die neue Fassung");
  cdp.hoerer.splice(cdp.hoerer.indexOf(zaehler), 1);
  await cdp.senden("Network.disable").catch(() => {});
  writeFileSync(seite(anderes), anderesOriginal);

  writeFileSync(seite(MESS_PROJEKT), original);
  await neueFassung(zeiger());
  cdp.hoerer.splice(cdp.hoerer.indexOf(konsoleHoeren), 1);
  pruefe(konsole.length === 0, "5: keine Konsolenfehler und keine unbehandelten Ausnahmen im Tab", konsole.slice(0, 3).join(" | ").slice(0, 400));
  pruefe(cdp.gesperrt.length === 0, "5: Netzsperre: keine Anfrage nach außen", cdp.gesperrt.slice(0, 3).join(", "));
}

// ----- Bau-Stand im Kopf (Schritt 6, Etappe 4c; ohne Zeitzeile und Kreispfeil) -----

const KNOPF_PROBE = "zz-knopf-probe";
const KNOPF_KOPF = 'schema = 3\ntitle = "Knopf-Probe"\nstatus = "aktiv"\nsummary = "Probe für den Bau-Stand im Kopf (4c)."\n';
const KNOPF_GUT = "<script lang=\"ts\">\n\tconst satz: string = 'Die Knopf-Probe ist gut.';\n</script>\n\n<p>{satz}</p>\n";
const KNOPF_KAPUTT = "<script lang=\"ts\">\n\tconst satz: string = 'Kaputt';\n</script>\n\n<p>{satz</p>\n";
/** „Sa 31.10., 14:05 Uhr“ (Tag und Uhrzeit, `fmtTagUhrzeit`). */
const TAG_UHRZEIT = /(Mo|Di|Mi|Do|Fr|Sa|So) \d{2}\.\d{2}\.(\d{4})?, \d{2}:\d{2} Uhr/;
/** Was Kopf, Knopf und Seitenleiste gerade zum Probe-Projekt zeigen. */
function bauStand(cdp) {
  return cdp.rufe((slug) => {
    const bau = document.querySelector("main .bau");
    const knopf = bau?.querySelector("button") ?? null;
    const r = knopf?.getBoundingClientRect();
    // Sichtbarer Text der Zeile, ohne die nur vorgelesene genaue Zeit.
    const g = bau?.querySelector(".generiert")?.cloneNode(true) ?? null;
    for (const n of g?.querySelectorAll(".nur-vorlesen") ?? []) n.remove();
    return {
      zustand: bau?.getAttribute("data-bau") ?? null,
      zeile: bau?.querySelector(".zeile")?.textContent?.trim() ?? "",
      satz: bau?.querySelector(".satz")?.textContent?.trim() ?? "",
      meldung: bau?.querySelector(".meldung")?.textContent?.trim() ?? "",
      generiert: (g?.textContent ?? "").replace(/\s+/g, " ").trim(),
      ansage: bau?.querySelector('[role="status"]')?.textContent?.trim() ?? "",
      knopf: knopf === null ? null : { name: knopf.getAttribute("aria-label"), gesperrt: knopf.getAttribute("aria-disabled"), busy: knopf.getAttribute("aria-busy"), disabled: knopf.disabled, hoehe: Math.round(r.height), breite: Math.round(r.width) },
      breit: document.documentElement.scrollWidth,
      leiste: document.querySelector(`.leiste a[href$="/projects/${slug}"] .bau-marke`)?.getAttribute("data-bau") ?? null,
      inhalt: document.querySelector("main .inhalt")?.innerText ?? "",
    };
  }, KNOPF_PROBE);
}

/**
 * Kopf und Inhalt der Projektseite im Tab, mit Adresse, Titel und einer
 * Kennung des Dokuments (bleibt bis zum nächsten Laden dieselbe).
 */
function projektStand(cdp) {
  return cdp.rufe(() => {
    window.__probeDokument ??= `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const bau = document.querySelector("main .bau");
    return {
      pfad: location.pathname,
      dokument: window.__probeDokument,
      titel: document.querySelector("main h1")?.textContent?.trim() ?? "",
      zustand: bau?.getAttribute("data-bau") ?? null,
      zeile: bau?.querySelector(".zeile")?.textContent?.trim() ?? "",
      satz: bau?.querySelector(".satz")?.textContent?.trim() ?? "",
      inhalt: document.querySelector("main .inhalt")?.innerText ?? "",
    };
  });
}

/**
 * Liest die Projektseite `slug` (Titel `titel`), sobald der Tab wirklich dort
 * steht und `bereit(stand)` gilt, zweimal im Abstand von einer Sekunde im
 * selben Dokument. Ein Neuladen dazwischen (neue Fassung nach dem Bau) oder
 * eine andere Adresse zählt nicht; dann noch einmal. Ohne das las 6d einmal
 * die Seite des Knopf-Projekts aus 6b (auch rot, auch mit Satz) statt
 * zz-gesamt-b. Gibt den letzten Stand zurück, auch wenn die Frist um ist.
 */
async function projektLesen(cdp, slug, titel, bereit, frist = 90000) {
  const pfad = `/projects/${slug}`;
  const ende = Date.now() + frist;
  let letzter = null;
  while (Date.now() < ende) {
    if ((await cdp.rufe(() => location.pathname).catch(() => null)) !== pfad) await cdp.oeffnen(pfad);
    const erst = await projektStand(cdp).catch(() => null);
    const passt = (st) => st !== null && st.pfad === pfad && st.titel.endsWith(titel) && bereit(st);
    if (erst !== null) letzter = erst;
    if (passt(erst)) {
      await pause(1000);
      const dann = await projektStand(cdp).catch(() => null);
      if (dann !== null) letzter = dann;
      if (passt(dann) && dann.dokument === erst.dokument) return dann;
    }
    await pause(300);
  }
  return letzter ?? { pfad: null, dokument: null, titel: "", zustand: null, zeile: "", satz: "", inhalt: "" };
}

/** Wartet, bis der Kopf `zustand` zeigt (auch über ein Neuladen hinweg). */
function bisZustand(cdp, zustand, frist = 240000) {
  return bisWahr(cdp, (z) => document.querySelector("main .bau")?.getAttribute("data-bau") === z, zustand, frist);
}

/** Bei 360 px Breite (wie ein Telefon): der Kopf, dann wieder die volle Breite. */
async function schmal(cdp) {
  await cdp.senden("Emulation.setDeviceMetricsOverride", { width: 360, height: 800, deviceScaleFactor: 1, mobile: true, screenWidth: 360, screenHeight: 800 });
  await pause(400);
  const s = await bauStand(cdp);
  await cdp.senden("Emulation.clearDeviceMetricsOverride");
  await pause(300);
  return s;
}

/** Schritt 6; `frei` ist das Edge ohne Anmeldung (127.0.0.1). */
async function knopfPruefen(frei) {
  console.log("\n== Bau-Stand im Kopf (Etappe 4c; ohne Zeitzeile und Kreispfeil) ==");
  const cdp = await angemeldetesEdge();
  const pfad = `/projects/${KNOPF_PROBE}`;
  const p = join(repo, "projects", KNOPF_PROBE);
  mkdirSync(p, { recursive: true });
  writeFileSync(join(p, "projekt.toml"), KNOPF_KOPF);
  let alt = zeiger();
  writeFileSync(join(p, "Seite.svelte"), KNOPF_GUT);
  pruefe((await neueFassung(alt)) !== null, "6: Probe-Projekt gebaut (Beobachter)");

  // a) Angemeldet: grün, nur die Statuszeile (kein Kreispfeil, keine Zeitzeile).
  await bisSichtbar(cdp, pfad, /Die Knopf-Probe ist gut/);
  await bisWahr(cdp, () => document.querySelector('.live[data-zustand="angemeldet"]') !== null, null, 15000);
  await bisWahr(cdp, () => document.querySelector("main .bau") !== null, null, 15000);
  // Die Sätze (angemeldet, /api/bauen) sind da: Früher kam dann der Kreispfeil.
  await pause(1500);
  let s = await bauStand(cdp);
  pruefe(s.zustand === "gruen" && s.zeile === "Seite in Ordnung", "6a: Kopf zeigt grün „Seite in Ordnung“", s.zeile);
  pruefe(s.knopf === null && s.generiert === "" && s.ansage === "", "6a: angemeldet kein Kreispfeil, keine Zeile „Seite wurde vor … generiert“, keine Ansage-Region im Kopf", JSON.stringify({ knopf: s.knopf, generiert: s.generiert, ansage: s.ansage }));
  const ohneGeneriert = await cdp.rufe(() => !/generiert|Neugenerierung/.test(document.querySelector("main .bau")?.textContent ?? "") && document.querySelector("[data-neu-generieren]") === null);
  pruefe(ohneGeneriert, "6a: nirgends „generiert“ oder „Neugenerierung“ im Kopf, kein Attribut des alten Knopfs");
  let eng = await schmal(cdp);
  pruefe(eng.breit <= 360 && eng.zustand === "gruen", "6a: bei 360 px kein seitliches Scrollen", JSON.stringify({ breit: eng.breit, zeile: eng.zeile }));

  // Bau-Tempo, Zweig C: Während eines Baus steht der Schritt im Kopf („prüft
  // Projekt x von y“), angesagt wird nur das Ergebnis (6a). Ohne Anmeldung
  // kommt kein Fortschritt, weder im freien Kanal noch in /api/projects (6c).
  await frei.oeffnen("/");
  await frei.rufe(() => {
    window.__frei = [];
    window.__listen = [];
    window.__kanal = new EventSource("/api/events/frei");
    for (const n of ["anmeldung", "bau-fortschritt", "fassung", "projekt", "inhalt"]) window.__kanal.addEventListener(n, (e) => window.__frei.push(`${n} ${e.data}`));
    window.__listenTakt = setInterval(async () => window.__listen.push(await (await fetch("/api/projects")).text()), 500);
  });
  await cdp.rufe(() => {
    window.__schritte = new Set();
    window.__ansagen = new Set();
    window.__zaehlTakt = setInterval(() => {
      const s = document.querySelector("main .bau [data-bau-schritt]");
      if (s) window.__schritte.add(s.textContent.trim());
      for (const r of document.querySelectorAll('[aria-live], [role="status"], [role="alert"], [role="progressbar"], progress')) {
        const t = r.textContent.trim();
        if (t) window.__ansagen.add(`${r.getAttribute("role") ?? r.tagName}: ${t}`);
      }
    }, 100);
  });
  alt = zeiger();
  writeFileSync(join(p, "Seite.svelte"), KNOPF_GUT.replace("<p>{satz}</p>\n", "<p>{satz}</p>\n<p>Zweite Fassung.</p>\n"));
  pruefe((await neueFassung(alt)) !== null, "6a (Zweig C): zweite Fassung gebaut");
  const zaehler = await cdp.rufe(() => {
    clearInterval(window.__zaehlTakt);
    return { schritte: [...(window.__schritte ?? [])], ansagen: [...(window.__ansagen ?? [])] };
  });
  pruefe(zaehler.schritte.some((t) => /^prüft (Projekt \d+ von \d+|\d+ Seiten)$/.test(t)), "6a (Zweig C): während des Baus „prüft Projekt x von y“ im Kopf", JSON.stringify(zaehler.schritte));
  pruefe(!zaehler.ansagen.some((t) => /prüft|von \d+/.test(t)) && !zaehler.ansagen.some((t) => /^progress/i.test(t)), "6a (Zweig C): angesagt wird nur das Ergebnis, nie der Schritt, kein Fortschrittsbalken", JSON.stringify(zaehler.ansagen));
  const ohne = await frei.rufe(() => {
    clearInterval(window.__listenTakt);
    window.__kanal.close();
    return { ereignisse: window.__frei, listen: window.__listen.length, mitZaehler: window.__listen.filter((l) => /"(schritt|fertig|gesamt)":/.test(l)).length };
  });
  pruefe(
    ohne.ereignisse.some((e) => e.startsWith("anmeldung")) && ohne.listen > 0 && ohne.mitZaehler === 0 && !ohne.ereignisse.some((e) => e.startsWith("bau-fortschritt") || /"(fertig|gesamt)"/.test(e)),
    "6c (Zweig C): ohne Anmeldung kein Fortschritt in /api/events/frei und /api/projects",
    JSON.stringify({ listen: ohne.listen, mitZaehler: ohne.mitZaehler, ereignisse: ohne.ereignisse.slice(0, 6) }),
  );

  // b) Absichtlich kaputt: rot mit „zeigt die letzte gute Fassung von …“.
  alt = zeiger();
  writeFileSync(join(p, "Seite.svelte"), KNOPF_KAPUTT);
  pruefe((await neueFassung(alt)) !== null, "6b: kaputte Seite: der Bau läuft trotzdem durch");
  await cdp.oeffnen(pfad);
  pruefe(await bisZustand(cdp, "rot", 30000), "6b: Kopf zeigt rot");
  await bisWahr(cdp, () => (document.querySelector("main .bau .satz")?.textContent ?? "") !== "", null, 15000);
  s = await bauStand(cdp);
  pruefe(/^Seite mit Fehler: zeigt die letzte gute Fassung von /.test(s.zeile) && TAG_UHRZEIT.test(s.zeile), "6b: „zeigt die letzte gute Fassung von …“ mit Tag und Uhrzeit", s.zeile);
  pruefe(s.generiert === "" && s.knopf === null, "6b: bei rot keine Zeitzeile und kein Kreispfeil", JSON.stringify({ generiert: s.generiert, knopf: s.knopf }));
  pruefe(s.satz !== "" && !/[\\/]|Seite\.svelte|svelte-check|eslint/i.test(s.satz), "6b: angemeldet der Satz dazu, ohne Pfade und Fachwörter", s.satz);
  pruefe(s.inhalt.includes("Die Knopf-Probe ist gut."), "6b: die letzte gute Fassung ist zu sehen", s.inhalt.slice(0, 80));
  pruefe(s.leiste === "rot", "6b: Marke in der Seitenleiste", String(s.leiste));
  eng = await schmal(cdp);
  pruefe(eng.breit <= 360 && eng.knopf === null && /zeigt die letzte gute Fassung von /.test(eng.zeile), "6b: rot bei 360 px kein seitliches Scrollen, kein Knopf", JSON.stringify({ breit: eng.breit, knopf: eng.knopf }));

  // d) Abnahme 4c: Eine Seite, die nur die Gesamtprüfung bricht, bleibt in ihrer
  // alten Fassung. Zweig B: Projekte erklären nichts Globales mehr (Scanner);
  // darum erklärt die Oberfläche der Kopie die Eigenschaft, und das Projekt
  // erwartet sie anders (für sich ohne die Oberfläche grün).
  writeFileSync(join(repo, "web", "src", "gesamt-probe.d.ts"), "export {};\n\ndeclare global {\n\tinterface HTMLElement {\n\t\tgesamtProbe: string;\n\t}\n}\n");
  const global = () => "export const probe: { gesamtProbe?: number; tagName: string } = document.body;\n";
  const gesamtSeite = (t) => `<script lang="ts">\n\tconst satz: string = '${t}';\n</script>\n\n<p>{satz}</p>\n`;
  for (const [slug, text] of [["zz-gesamt-a", "Gesamt-Probe A"], ["zz-gesamt-b", "Gesamt-Probe B"]]) {
    mkdirSync(join(repo, "projects", slug), { recursive: true });
    writeFileSync(join(repo, "projects", slug, "projekt.toml"), KNOPF_KOPF.replace("Knopf-Probe", text));
  }
  alt = zeiger();
  writeFileSync(join(repo, "projects", "zz-gesamt-a", "Seite.svelte"), gesamtSeite("Gesamt-Probe A ist da"));
  writeFileSync(join(repo, "projects", "zz-gesamt-b", "Seite.svelte"), gesamtSeite("Gesamt-Probe B alt"));
  pruefe((await neueFassung(alt)) !== null && (await standAbwarten((p) => p["zz-gesamt-a"]?.gruen === true && p["zz-gesamt-b"]?.gruen === true)) !== null, "6d: zwei Probe-Projekte grün gebaut");
  alt = zeiger();
  writeFileSync(join(repo, "projects", "zz-gesamt-b", "typen.ts"), global());
  writeFileSync(join(repo, "projects", "zz-gesamt-b", "Seite.svelte"), gesamtSeite("Gesamt-Probe B neu"));
  pruefe((await neueFassung(alt)) !== null && (await standAbwarten((p) => p["zz-gesamt-b"]?.rot_art === "zusammenbau")) !== null, "6d: nur die Gesamtprüfung rot: der Bau läuft trotzdem durch");
  // Erst lesen, wenn der Tab auf zz-gesamt-b steht (nicht mehr auf der Knopf-Probe aus 6b).
  const g = await projektLesen(cdp, "zz-gesamt-b", "Gesamt-Probe B", (st) => st.zustand === "rot" && st.satz !== "" && st.inhalt.includes("Gesamt-Probe B"));
  pruefe(g.pfad === "/projects/zz-gesamt-b", "6d: der Tab steht auf zz-gesamt-b", `${g.pfad} „${g.titel}“`);
  pruefe(g.inhalt.includes("Gesamt-Probe B alt") && !g.inhalt.includes("Gesamt-Probe B neu"), "6d: die Seite zeigt ihre alte Fassung (aus der grünen Kopie gebaut)", g.inhalt.slice(0, 80));
  pruefe(/zeigt die letzte gute Fassung von /.test(g.zeile) && /passt nicht zum Rest/.test(g.satz), "6d: Kopf und Satz: passt nicht zum Rest, letzte gute Fassung", `${g.zeile} | ${g.satz}`);
  const andere = await bisSichtbar(cdp, "/projects/zz-gesamt-a", /Gesamt-Probe A ist da/);
  pruefe(andere.includes("Gesamt-Probe A ist da"), "6d: das andere Projekt läuft weiter");

  // e) Nachbesserung 2: dieselbe Lage, das geänderte Projekt sortiert vorn
  // (zz-gesamt-0 vor zz-gesamt-a). Es trifft trotzdem das geänderte.
  const vorn = "zz-gesamt-0";
  mkdirSync(join(repo, "projects", vorn), { recursive: true });
  writeFileSync(join(repo, "projects", vorn, "projekt.toml"), KNOPF_KOPF.replace("Knopf-Probe", "Gesamt-Probe 0"));
  alt = zeiger();
  writeFileSync(join(repo, "projects", vorn, "Seite.svelte"), gesamtSeite("Gesamt-Probe 0 alt"));
  pruefe((await neueFassung(alt)) !== null && (await standAbwarten((p) => p[vorn]?.gruen === true)) !== null, "6e: Probe-Projekt vorn grün gebaut");
  alt = zeiger();
  writeFileSync(join(repo, "projects", vorn, "typen.ts"), global());
  writeFileSync(join(repo, "projects", vorn, "Seite.svelte"), gesamtSeite("Gesamt-Probe 0 neu"));
  pruefe((await neueFassung(alt)) !== null && (await standAbwarten((p) => p[vorn]?.rot_art === "zusammenbau")) !== null, "6e: nur die Gesamtprüfung rot: der Bau läuft trotzdem durch");
  const v = await projektLesen(cdp, vorn, "Gesamt-Probe 0", (st) => st.zustand === "rot" && st.inhalt.includes("Gesamt-Probe 0"));
  pruefe(v.inhalt.includes("Gesamt-Probe 0 alt") && !v.inhalt.includes("Gesamt-Probe 0 neu") && /zeigt die letzte gute Fassung von /.test(v.zeile), "6e: das geänderte (vorn) fällt auf seine alte Fassung zurück", `${v.zeile} | ${v.inhalt.slice(0, 60)}`);
  const a = await projektLesen(cdp, "zz-gesamt-a", "Gesamt-Probe A", (st) => st.zustand === "gruen" && st.inhalt.includes("Gesamt-Probe A ist da"), 30000);
  pruefe(a.pfad === "/projects/zz-gesamt-a" && a.zustand === "gruen", "6e: das unveränderte (hinten) bleibt grün", `${a.pfad}: ${a.zustand}`);
  const stand = JSON.parse(readFileSync(join(repo, "web", ".projekte-gruen", "stand.json"), "utf8"));
  pruefe(stand.projekte?.[vorn]?.rot_art === "zusammenbau" && stand.projekte?.["zz-gesamt-a"]?.gruen === true, "6e: stand.json: das geänderte zurückgesetzt, das unveränderte grün", JSON.stringify({ vorn: stand.projekte?.[vorn], a: stand.projekte?.["zz-gesamt-a"] }));
  // Die Probe-Typen gelten sonst für alle späteren Gesamtprüfungen der Kopie.
  rmSync(join(repo, "web", "src", "gesamt-probe.d.ts"), { force: true });

  // c) Ohne Anmeldung: derselbe Stand, ohne Satz und ohne Knopf.
  await bisSichtbar(frei, pfad, /Die Knopf-Probe ist gut/);
  await bisWahr(frei, () => document.querySelector("main .bau") !== null, null, 15000);
  s = await bauStand(frei);
  pruefe(s.zustand === "rot" && /zeigt die letzte gute Fassung von /.test(s.zeile), "6c: ohne Anmeldung: rot mit „letzte gute Fassung von …“", s.zeile);
  pruefe(s.knopf === null && s.satz === "", "6c: ohne Anmeldung kein Knopf und kein Satz", JSON.stringify({ knopf: s.knopf, satz: s.satz }));
  pruefe(s.generiert === "", "6c: ohne Anmeldung bei rot keine Zeile „Seite wurde vor … generiert“", s.generiert);
  await frei.oeffnen("/");
  await bisWahr(frei, () => document.querySelector(".pill.bau") !== null, null, 15000);
  const start = await frei.rufe(() => ({
    zeile: document.querySelector(".section-head .muted")?.textContent ?? "",
    karte: [...document.querySelectorAll(".pill.bau")].map((e) => e.textContent.trim()).join(" | "),
  }));
  pruefe(/\d+ Seiten? mit Fehler/.test(start.zeile) && /zeigt die letzte gute Fassung von /.test(start.karte), "6c: Startseite: Zählzeile und Karte zeigen die rote Seite", JSON.stringify(start));
  const lesen = await frei.rufe(async () => (await fetch("/api/bauen")).status);
  pruefe(lesen === 401, "6c: /api/bauen ohne Anmeldung 401", String(lesen));
  const bestellen = await frei.rufe(async (slug) => (await fetch(`/api/bauen/${slug}`, { method: "POST" })).status, KNOPF_PROBE);
  pruefe(bestellen < 200 || bestellen > 299, "6d: POST /api/bauen/<slug> gibt es nicht mehr (kein 2xx)", String(bestellen));
  pruefe(cdp.gesperrt.length === 0 && frei.gesperrt.length === 0, "6: Netzsperre: keine Anfrage nach außen", [...cdp.gesperrt, ...frei.gesperrt].slice(0, 3).join(", "));
}

// ----- Ablauf -----

async function main() {
  await portePruefen(PORT);
  if (!existsSync(EXE)) throw new Error(`Programm fehlt: ${EXE}`);
  if (!existsSync(join(WEB, "index.html"))) throw new Error(`Oberfläche fehlt: ${WEB}`);
  kopieAnlegen();
  await serverStarten();
  const cdp = await edgeStarten();
  console.log(`Kopie in ${ordner}; Server auf ${BASIS}`);

  console.log("\n== Aufwärmen: erster Bau prüft alle Projekte ==");
  const start = Date.now();
  const jetzt = new Date();
  utimesSync(seite(MESS_PROJEKT), jetzt, jetzt);
  const erste = await neueFassung(null, 600000);
  pruefe(erste !== null, "Erster Bau grün", `${sek(Date.now() - start)}`);
  messen("Aufwärmen (alle Projekte, erster Bau)", `${sek(Date.now() - start)}; ${letzteSchritte().replace(/^.*Baumeister: /, "").slice(0, 400)}`);

  if (NUR === "") {
    await eslintMessen();
    await echtMessen(cdp);
    await abnahme(cdp);
  }
  if (NUR === "" || NUR === "fassung") await fassungPruefen();
  if (NUR === "" || NUR === "knopf" || NUR === "bau") await knopfPruefen(cdp);
  angemeldet?.browserSperre?.schliessen();
  cdp.browserSperre?.schliessen();

  console.log("\n== Messwerte ==");
  for (const m of messwerte) console.log(`  ${m}`);
  console.log(`\nBestanden: ${bestanden} Prüfungen. Nicht bestanden: ${fehlgeschlagen.length}.`);
  for (const f of fehlgeschlagen) console.log(`  - ${f}`);
  return fehlgeschlagen.length === 0 ? 0 : 1;
}

let code = 1;
try {
  code = await main();
} catch (e) {
  console.error(`Abbruch: ${e.message}`);
} finally {
  await aufraeumen();
}
process.exit(code);
