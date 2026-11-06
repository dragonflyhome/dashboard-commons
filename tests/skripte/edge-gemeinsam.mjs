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
 */
import { existsSync } from "node:fs";
import { createConnection, createServer } from "node:net";
import { dirname, join, resolve } from "node:path";
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
