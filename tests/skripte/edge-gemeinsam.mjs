/**
 * Gemeinsames für die Edge-Proben (katalog-edge.mjs, zeitachse-edge.mjs,
 * passkey-edge.mjs, pc-edge.mjs; Auflagen aus 3a-1 und 3a-4, Plan 3a+ B).
 *
 * 1. `browserSperren`: Netzsperre auf Ebene des Browsers. Die Proben sperren
 *    die Seite selbst mit Fetch.enable auf ihrer Verbindung. Dazu hängt sich
 *    hier eine zweite Verbindung an den Browser (`/json/version`) und mit
 *    `Target.setAutoAttach` (flatten, `waitForDebuggerOnStart`) an jedes
 *    andere Ziel: Service-Worker, Shared Worker, weitere Seiten und Fenster,
 *    und über die Seiten an deren Kinder (Worker, Frames aus anderem
 *    Prozess). Jedes dieser Ziele bekommt Fetch.enable, bevor es läuft
 *    (`Runtime.runIfWaitingForDebugger` erst danach). Was nicht an
 *    127.0.0.1 oder localhost geht, bricht ab und landet in `gesperrt`.
 *    Nicht erfasst: Anfragen, die der Browser-Prozess selbst stellt (etwa
 *    Updates); die schalten die Startschalter der Proben ab
 *    (--disable-background-networking, --disable-component-update).
 *    Eingebaute Erweiterungen mit Hintergrundseite (chrome-extension://…)
 *    starten gar nicht (--disable-extensions,
 *    --disable-component-extensions-with-background-pages); die Netzsperre
 *    prüft so nur, was die Dashboard-Seiten anfragen.
 * 2. `beiAbbruch`: macht das Aufräumen (Server, Edge, Temp-Ordner) einmalig
 *    und ruft es auch bei Strg+C (SIGINT, SIGBREAK, SIGTERM) und bei einem
 *    unbehandelten Fehler oder abgelehnten Versprechen. Die Probe ruft das
 *    zurückgegebene Aufräumen im `finally` um `main()`.
 * 3. `portBelegt`: Ist ein Port schon in Gebrauch (etwa ein anderer
 *    Testserver)? Dann bricht die Probe mit klarem Satz ab, statt gegen den
 *    falschen Server zu prüfen.
 * 4. `mitFrist`: Ein Versprechen mit Zeitablauf (auch beim Aufräumen).
 * 5. `inhaltOrt`: Woher die Proben den Inhalt kopieren (Umzug des Inhalts):
 *    `DASHBOARD_INHALT` (Ordner mit projects, data, docs\entscheidungen),
 *    sonst der Nachbarordner `dashboard-inhalt` des Repos, wenn es ihn
 *    gibt, sonst das Repo selbst (vor dem Umzug). Die Proben kopieren nur,
 *    nie ändern sie den Inhalt.
 * 6. `ohneAnmeldungPruefen` (Schritt E2): Ohne Anmeldung führt jede nicht
 *    öffentliche Seite zur Anmeldung, zeigt keinen Projekttitel und
 *    `/api/projects` antwortet 401. Die reinen Teile prüft
 *    `node tests/skripte/edge-gemeinsam.mjs` selbst.
 */
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { createConnection, createServer } from "node:net";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { antwortAbwarten } from "./cdp-frist.mjs";

/**
 * Der Ort des Inhalts für die Kopien der Proben: `{ wurzel, projects, data,
 * entscheidungen }`. `wurzel` ist das Git-Repo des Inhalts (für
 * `git ls-files data`).
 */
export function inhaltOrt(repo) {
  const kandidaten = [process.env.DASHBOARD_INHALT, join(dirname(resolve(repo)), "dashboard-inhalt")].filter(Boolean);
  const da = kandidaten.find((k) => existsSync(join(k, "projects")));
  const wurzel = da ? resolve(da) : resolve(repo);
  return {
    wurzel,
    projects: join(wurzel, "projects"),
    data: join(wurzel, "data"),
    entscheidungen: join(wurzel, "docs", "entscheidungen"),
  };
}

/** Adressen, die eine Probe erreichen darf. */
const ERLAUBT = new Set(["127.0.0.1", "localhost", "[::1]"]);

/** Ist `url` eine erlaubte Adresse (nur der eigene Testserver)? */
export function erlaubteAdresse(url) {
  try {
    return ERLAUBT.has(new URL(url).hostname);
  } catch {
    return false;
  }
}

/**
 * Schritt E2: Ohne Anmeldung zeigt das Dashboard nur die Anmeldeseite. Was ein
 * Browser ohne Anmeldung auf einer nicht öffentlichen Seite sieht, liest dieser
 * Ausdruck (für Runtime.evaluate): wo der Tab steht, das Ziel der Anmeldeseite,
 * die Ziele der Links zur Anmeldung (die 401-Seite des Servers unter Windows),
 * Links auf Projekte, das ganze HTML und den Status von `/api/projects`.
 */
export const OHNE_ANMELDUNG = `(async () => {
  const links = [...document.querySelectorAll("a[href]")].map((a) => new URL(a.href, location.href));
  return {
    pfad: location.pathname,
    ziel: location.pathname === "/anmelden" ? new URLSearchParams(location.search).get("ziel") : null,
    anmeldeZiele: links.filter((u) => u.pathname === "/anmelden").map((u) => u.searchParams.get("ziel")),
    projektLinks: links.filter((u) => /^\\/(projects|gruppen)\\//.test(u.pathname)).map((u) => u.pathname),
    html: document.documentElement.outerHTML,
    projekte: await fetch("/api/projects", { headers: { accept: "application/json" } }).then((r) => r.status, () => 0),
  };
})()`;

/**
 * Die Mängel einer Seite ohne Anmeldung (leer: bestanden). Gut ist die
 * Anmeldeseite der Oberfläche mit `ziel` gleich `pfad` (Weiterleitung im
 * Browser, außerhalb von Windows 303 vom Server) oder die 401-Seite des
 * Servers mit dem Link „Zur Anmeldung“ auf dasselbe Ziel (Windows). Dazu kein
 * Link auf ein anderes Projekt als `pfad` selbst (die 401-Seite verlinkt die
 * angefragte Adresse), kein Projekttitel im DOM und `/api/projects` 401.
 * Ein Titel, der ganz in einem festen Text der Oberfläche steht
 * (`OBERFLAECHE`, etwa „Dashboard“ im Tab-Titel), lässt sich davon nicht
 * unterscheiden und zählt nicht.
 * @param {{ pfad: string, ziel: string | null, anmeldeZiele: (string | null)[], projektLinks: string[], html: string, projekte: number }} stand
 * @param {string} pfad
 * @param {string[]} titel
 * @returns {string[]}
 */
export function ohneAnmeldungMaengel(stand, pfad, titel) {
  const maengel = [];
  if (!((stand.pfad === "/anmelden" && stand.ziel === pfad) || stand.anmeldeZiele.includes(pfad)))
    maengel.push(`führt nicht zur Anmeldung (Tab auf ${stand.pfad}, Ziel ${stand.ziel ?? (stand.anmeldeZiele.join(", ") || "keins")})`);
  // Die 401-Seite verlinkt die angefragte Adresse selbst (zum Öffnen in Edge); die nennt nichts Neues.
  const links = stand.projektLinks.filter((l) => l !== pfad);
  if (links.length > 0) maengel.push(`Links auf Projekte: ${links.slice(0, 3).join(", ")}`);
  const gesehen = titel.filter((t) => t !== "" && stand.html.includes(t) && !OBERFLAECHE.some((o) => o.includes(t)));
  if (gesehen.length > 0) maengel.push(`Projekttitel im DOM: ${gesehen.slice(0, 3).join(", ")}`);
  if (stand.projekte !== 401) maengel.push(`/api/projects ${stand.projekte} statt 401`);
  return maengel;
}

/**
 * Feste Texte der Seiten ohne Anmeldung (Tab-Titel der Oberfläche, Anmeldeseite,
 * 401-Seite des Servers). Nur sie nimmt die Titelprüfung aus, nie das HTML einer
 * Antwort: Ein Leck, das auf jeder Seite gleich aussieht, bliebe sonst unbemerkt.
 */
export const OBERFLAECHE = ["Dashboard", "Anmelden · Dashboard", "Dashboard: Anmeldung nötig"];

/** Ein Weg, den es nie gibt: Auch seine Antwort ohne Anmeldung darf keinen Projekttitel nennen. */
export const ERFUNDEN = "/projects/zz-gibt-es-nicht-e2";

/**
 * Öffnet jeden Weg aus `pfade` ohne Anmeldung und wartet höchstens `frist` ms
 * auf die Anmeldeseite (die Weiterleitung im Browser braucht einen Moment).
 * `cdp`: `oeffnen(pfad)` und `werte(ausdruck)` der Probe. Gibt je Weg die
 * Mängel zurück; der erfundene Weg (`ERFUNDEN`) läuft zuerst mit.
 * @param {{ oeffnen: (pfad: string) => Promise<unknown>, werte: (ausdruck: string) => Promise<any> }} cdp
 * @param {string[]} pfade
 * @param {string[]} titel
 * @returns {Promise<{ pfad: string, maengel: string[] }[]>}
 */
export async function ohneAnmeldungPruefen(cdp, pfade, titel, frist = 10000) {
  const lesen = async (pfad) => {
    await cdp.oeffnen(pfad);
    const ende = Date.now() + frist;
    let stand = null;
    while (Date.now() < ende) {
      stand = await cdp.werte(OHNE_ANMELDUNG).catch(() => null);
      if (stand !== null && ohneAnmeldungMaengel(stand, pfad, []).length === 0) break;
      await new Promise((ok) => setTimeout(ok, 250));
    }
    return stand ?? { pfad: "?", ziel: null, anmeldeZiele: [], projektLinks: [], html: "", projekte: 0 };
  };
  const ergebnis = [];
  for (const pfad of [ERFUNDEN, ...pfade]) ergebnis.push({ pfad, maengel: ohneAnmeldungMaengel(await lesen(pfad), pfad, titel) });
  return ergebnis;
}

/**
 * Die Titel der Projekte in `ordner` (je `projekt.toml` die Zeile `title = "…"`).
 * shortcut: liest nur die einfache Zeile ohne TOML-Parser; ein Titel mit
 * Anführungszeichen darin fehlt dann in der Liste, aufrüsten, wenn es so einen gibt.
 * @param {string} ordner
 * @returns {string[]}
 */
export function projektTitel(ordner) {
  if (!existsSync(ordner)) return [];
  return readdirSync(ordner, { withFileTypes: true })
    .filter((d) => d.isDirectory() && existsSync(join(ordner, d.name, "projekt.toml")))
    .map((d) => /^title\s*=\s*"([^"\\]*)"/m.exec(readFileSync(join(ordner, d.name, "projekt.toml"), "utf8"))?.[1] ?? "")
    .filter((t) => t !== "");
}

/**
 * Wartet höchstens `ms` auf `versprechen`; danach Fehler mit `satz`.
 * @template T
 * @param {Promise<T>} versprechen
 * @param {number} ms
 * @param {string} satz
 * @returns {Promise<T>}
 */
export function mitFrist(versprechen, ms, satz) {
  let t;
  const ablauf = new Promise((_, nein) => {
    t = setTimeout(() => nein(new Error(`${satz} (nach ${ms} ms)`)), ms);
  });
  return Promise.race([versprechen, ablauf]).finally(() => clearTimeout(t));
}

/**
 * Netzsperre auf Browser-Ebene (siehe oben). `gesperrt`: das Feld, in das
 * gesperrte Adressen kommen (dasselbe wie die Sperre der Seite, damit die
 * Probe nur eine Liste prüft). `ausser`: die Ziel-Kennung der Seite, die die
 * Probe selbst sperrt (keine doppelte Sperre; ihre Kinder sperrt diese hier).
 * @param {number} debugPort
 * @param {string[]} gesperrt
 * @param {{ ausser?: string }} [optionen]
 */
export async function browserSperren(debugPort, gesperrt, { ausser } = {}) {
  const version = await mitFrist(
    fetch(`http://127.0.0.1:${debugPort}/json/version`).then((r) => r.json()),
    10000,
    "Netzsperre: /json/version antwortet nicht",
  );
  const ws = new WebSocket(version.webSocketDebuggerUrl);
  await mitFrist(
    new Promise((ok, nein) => {
      ws.addEventListener("open", ok, { once: true });
      ws.addEventListener("error", nein, { once: true });
    }),
    10000,
    "Netzsperre: Verbindung zum Browser geht nicht auf",
  );
  const offen = new Map();
  let nr = 0;
  /** Ziele, an denen die Sperre hängt: sessionId -> { typ, url, gesperrt }. */
  const ziele = new Map();
  /** Ziele, die Fetch nicht annehmen (dort greift die Sperre nicht). */
  const ohneSperre = [];

  function senden(method, params = {}, sessionId) {
    const id = ++nr;
    ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    return antwortAbwarten(offen, id, method);
  }

  async function anhaengen(sessionId, info, wartet) {
    const eigeneSeite = info.targetId === ausser;
    ziele.set(sessionId, { typ: info.type, url: info.url, gesperrt: !eigeneSeite });
    try {
      if (!eigeneSeite)
        await senden("Fetch.enable", { patterns: [{ urlPattern: "*" }] }, sessionId);
    } catch (e) {
      ziele.get(sessionId).gesperrt = false;
      ohneSperre.push(`${info.type} ${info.url}: ${String(e?.message ?? e).slice(0, 120)}`);
    }
    // Kinder dieses Ziels (Worker, Frames) ebenso anhängen; nicht jedes Ziel kann das.
    await senden(
      "Target.setAutoAttach",
      { autoAttach: true, waitForDebuggerOnStart: true, flatten: true },
      sessionId,
    ).catch(() => {});
    if (wartet) await senden("Runtime.runIfWaitingForDebugger", {}, sessionId).catch(() => {});
  }

  ws.addEventListener("message", (e) => {
    const m = JSON.parse(String(e.data));
    if (m.id !== undefined) {
      const o = offen.get(m.id);
      if (!o) return;
      offen.delete(m.id);
      if (m.error) o.nein(new Error(`${m.error.message} (${JSON.stringify(m.error)})`));
      else o.ok(m.result);
      return;
    }
    if (m.method === "Target.attachedToTarget") {
      void anhaengen(m.params.sessionId, m.params.targetInfo, m.params.waitingForDebugger);
    } else if (m.method === "Target.detachedFromTarget") {
      ziele.delete(m.params.sessionId);
    } else if (m.method === "Fetch.requestPaused") {
      const { requestId, request } = m.params;
      if (erlaubteAdresse(request.url)) {
        senden("Fetch.continueRequest", { requestId }, m.sessionId).catch(() => {});
      } else {
        gesperrt.push(`${request.url} (${ziele.get(m.sessionId)?.typ ?? "Ziel"})`);
        senden(
          "Fetch.failRequest",
          { requestId, errorReason: "BlockedByClient" },
          m.sessionId,
        ).catch(() => {});
      }
    }
  });

  await senden("Target.setAutoAttach", {
    autoAttach: true,
    waitForDebuggerOnStart: true,
    flatten: true,
  });

  return {
    ziele,
    ohneSperre,
    /** Kurz für das Protokoll der Probe. */
    bericht() {
      const typen = [...ziele.values()].filter((z) => z.gesperrt).map((z) => z.typ);
      return `Netzsperre auf Browser-Ebene: ${typen.length} weitere Ziele (${[...new Set(typen)].join(", ") || "keine"})${ohneSperre.length > 0 ? `; ohne Sperre: ${ohneSperre.join(" | ")}` : ""}`;
    },
    schliessen() {
      try {
        ws.close();
      } catch {
        // schon zu
      }
    },
  };
}

/**
 * Das eine Aufräumen jeder Probe (Server, Edge, Temp-Ordner), auf jedem Weg:
 *
 * - Gibt `aufraeumen` einmalig zurück: Der erste Ruf räumt auf (höchstens
 *   60 s), jeder weitere wartet auf denselben Lauf. Die Probe ruft es im
 *   `finally` um `main()`, also auch nach Fehlern und Fristen.
 * - Strg+C (SIGINT), SIGBREAK und SIGTERM: aufräumen, dann Ende mit 130.
 *   Ein zweites Strg+C beendet sofort.
 * - Ein Fehler außerhalb von `main()` (unbehandelt abgelehntes Versprechen,
 *   etwa eine abgelaufene Frist, auf die niemand mehr wartet, oder ein Wurf
 *   in einem Hörer): Node beendete sonst sofort, am `finally` vorbei, und
 *   Edge hielte den Debug-Port belegt („Port … ist belegt“ in der nächsten
 *   Probe). Hier: melden, aufräumen, Ende mit 1.
 * @param {() => Promise<void>} aufraeumen
 * @returns {() => Promise<void>}
 */
export function beiAbbruch(aufraeumen) {
  let lauf = null;
  const einmal = () => {
    lauf ??= mitFrist(Promise.resolve().then(aufraeumen), 60000, "Aufräumen hängt").catch((e) => {
      console.error(String(e?.message ?? e));
    });
    return lauf;
  };
  let halt = false;
  const ende = async (code, satz) => {
    if (halt) {
      // Schon beim Aufräumen: ein zweites Signal beendet sofort; ein weiterer Fehler wartet.
      if (code === 130) process.exit(code);
      return;
    }
    halt = true;
    console.error(`\n${satz}: räume Server, Edge und Temp-Ordner auf …`);
    await einmal();
    process.exit(code);
  };
  for (const s of ["SIGINT", "SIGBREAK", "SIGTERM"]) {
    try {
      process.on(s, () => void ende(130, `Abbruch (${s})`));
    } catch {
      // Signal gibt es auf diesem System nicht
    }
  }
  process.on("uncaughtException", (e) => {
    console.error(String(e?.stack ?? e));
    void ende(1, "Abbruch (unbehandelter Fehler)");
  });
  process.on("unhandledRejection", (e) => {
    console.error(String(e?.stack ?? e));
    void ende(1, "Abbruch (unbehandelt abgelehnt)");
  });
  return einmal;
}

/**
 * Ist `port` belegt? Ja, wenn dort etwas eine Verbindung annimmt (IPv4 oder
 * IPv6) oder sich der Port auf 127.0.0.1 nicht binden lässt.
 * @param {number} port
 * @returns {Promise<boolean>}
 */
export async function portBelegt(port) {
  const verbindet = (host) =>
    new Promise((ok) => {
      const s = createConnection({ host, port });
      const fertig = (ja) => {
        s.destroy();
        ok(ja);
      };
      s.setTimeout(1000, () => fertig(false));
      s.once("connect", () => fertig(true));
      s.once("error", () => fertig(false));
    });
  if ((await verbindet("127.0.0.1")) || (await verbindet("::1"))) return true;
  return new Promise((ok) => {
    const srv = createServer();
    srv.once("error", () => ok(true));
    srv.listen({ host: "127.0.0.1", port, exclusive: true }, () => srv.close(() => ok(false)));
  });
}

/**
 * Etappe 4b: Alle Edge-Proben prüfen vor dem Start denselben Satz Ports:
 * den Testserver (`port`, Standard 8788) und beide Edge-Debug-Ports
 * (`port + 1000` und `port + 1001`, also 9788 und 9789). Ist einer belegt,
 * bricht die Probe mit einem klaren Satz ab; sie startet nie etwas gegen
 * einen fremden Server. Nie zwei Proben zugleich; sonst `--port`.
 * @param {number} port
 */
export async function portePruefen(port) {
  for (const [p, wofuer] of [
    [port, "Testserver"],
    [port + 1000, "Edge-Debug-Port"],
    [port + 1001, "Edge-Debug-Port (zweites Edge)"],
  ]) {
    if (await portBelegt(p))
      throw new Error(
        `Port ${p} (${wofuer}) ist belegt: Läuft dort schon ein Server oder eine andere Probe? ` +
          `Erst beenden oder später noch einmal, oder eine andere Probe mit --port starten; diese Probe startet nichts gegen einen fremden Server.`,
      );
  }
}

/**
 * Selbstprüfung der reinen Teile (ohne Edge): `node tests/skripte/edge-gemeinsam.mjs`.
 * Exit 0 heißt bestanden; jede Zusicherung, die nicht hält, bricht mit Fehler ab.
 */
async function selbstpruefung() {
  const { deepStrictEqual: gleich, ok } = await import("node:assert/strict");
  const { mkdtempSync, mkdirSync, writeFileSync, rmSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const leer = { pfad: "/anmelden", ziel: "/pc", anmeldeZiele: [], projektLinks: [], html: "<title>Anmelden · Dashboard</title>", projekte: 401 };
  gleich(ohneAnmeldungMaengel(leer, "/pc", ["Beispiel"]), []);
  // Die 401-Seite des Servers (Windows): Link „Zur Anmeldung“ mit demselben Ziel.
  gleich(ohneAnmeldungMaengel({ ...leer, pfad: "/pc", ziel: null, anmeldeZiele: ["/pc"] }, "/pc", []), []);
  // Falsches Ziel, eine andere Seite, ein Link auf ein Projekt, ein Titel, /api/projects 200: je ein Mangel.
  gleich(ohneAnmeldungMaengel({ ...leer, ziel: "/" }, "/pc", []).length, 1);
  gleich(ohneAnmeldungMaengel({ ...leer, pfad: "/pc", ziel: null }, "/pc", []).length, 1);
  gleich(ohneAnmeldungMaengel({ ...leer, projektLinks: ["/projects/beispiel"] }, "/pc", []).length, 1);
  // Die 401-Seite verlinkt die angefragte Projektseite selbst: kein Mangel; ein anderes Projekt schon.
  const seite401 = { ...leer, pfad: "/projects/beispiel", ziel: null, anmeldeZiele: ["/projects/beispiel"] };
  gleich(ohneAnmeldungMaengel({ ...seite401, projektLinks: ["/projects/beispiel"] }, "/projects/beispiel", []), []);
  gleich(ohneAnmeldungMaengel({ ...seite401, projektLinks: ["/projects/beispiel", "/projects/probekonto"] }, "/projects/beispiel", []).length, 1);
  gleich(ohneAnmeldungMaengel({ ...leer, html: "<h1>Beispiel zwei</h1>" }, "/pc", ["Beispiel zwei"]).length, 1);
  gleich(ohneAnmeldungMaengel({ ...leer, projekte: 200 }, "/pc", []).length, 1);
  // Ein Titel, der ganz in einem festen Text der Oberfläche steht, zählt nicht; einer, der nur darin anfängt, schon.
  gleich(ohneAnmeldungMaengel(leer, "/pc", ["Dashboard"]), []);
  gleich(ohneAnmeldungMaengel({ ...leer, html: "<title>Dashboard Beispiel</title>" }, "/pc", ["Dashboard Beispiel"]).length, 1);
  // Gegenprüfung E2 Runde 2: Steht ein Titel auf jeder Seite, auch auf dem erfundenen Weg, ist das ein Mangel überall.
  const ueberall = { ...leer, html: "<nav>Beispiel Geheim</nav>", ziel: null, pfad: "/anmelden" };
  const cdp = { oeffnen: async (p) => (ueberall.ziel = p), werte: async () => ({ ...ueberall }) };
  const alle = await ohneAnmeldungPruefen(cdp, ["/", "/projects/beispiel"], ["Beispiel Geheim", "Probekonto Plan"], 50);
  gleich(alle.map((e) => e.pfad), [ERFUNDEN, "/", "/projects/beispiel"]);
  ok(alle.every((e) => e.maengel.length === 1 && e.maengel[0].startsWith("Projekttitel im DOM")), JSON.stringify(alle));
  // projektTitel: nur Ordner mit projekt.toml, nur die Zeile title.
  const ordner = mkdtempSync(join(tmpdir(), "edge-gemeinsam-"));
  try {
    for (const [slug, kopf] of [["beispiel", 'schema = 3\ntitle = "Beispiel"\n'], ["beispiel2", 'subtitle = "nein"\ntitle = "Beispiel zwei"\n']]) {
      mkdirSync(join(ordner, slug));
      writeFileSync(join(ordner, slug, "projekt.toml"), kopf);
    }
    mkdirSync(join(ordner, "ohne-kopf"));
    gleich(projektTitel(ordner).sort(), ["Beispiel", "Beispiel zwei"]);
    gleich(projektTitel(join(ordner, "fehlt")), []);
  } finally {
    rmSync(ordner, { recursive: true, force: true });
  }
  ok(ERFUNDEN.startsWith("/projects/"));
  // Der Ausdruck für die Seite ist gültiges JavaScript, und sein Muster erkennt Projektwege.
  ok(typeof new Function(`return ${OHNE_ANMELDUNG}`) === "function");
  ok(OHNE_ANMELDUNG.includes(String.raw`/^\/(projects|gruppen)\//`));
  console.log("edge-gemeinsam: Selbstprüfung bestanden");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await selbstpruefung();
