/**
 * Gemeinsamer Zeitablauf für das DevTools-Protokoll (CDP) der Edge-Proben
 * (katalog-edge.mjs, zeitachse-edge.mjs, passkey-edge.mjs; Auflage aus 3a-1):
 * Jede gesendete Nachricht bekommt eine Frist. Antwortet Edge nicht, scheitert
 * die Probe mit einem klaren Satz, statt für immer zu warten.
 *
 *   senden(method, params = {}, ms) {
 *     const id = ++this.nr;
 *     this.ws.send(JSON.stringify({ id, method, params }));
 *     return antwortAbwarten(this.offen, id, method, ms);
 *   }
 *
 * `offen` ist die Map id -> { ok, nein }, die der message-Hörer des Skripts
 * auflöst; nach Ablauf der Frist kommt der Eintrag dort heraus, eine späte
 * Antwort wird also still verworfen.
 */

/** Standard-Frist je Nachricht in Millisekunden. */
export const CDP_FRIST_MS = 30000;

/**
 * Wartet auf die Antwort zu `id` und lehnt nach `ms` Millisekunden ab.
 * @param {Map<number, { ok: (r: unknown) => void, nein: (e: Error) => void }>} offen
 * @param {number} id
 * @param {string} method
 * @param {number} [ms]
 * @returns {Promise<any>}
 */
export function antwortAbwarten(offen, id, method, ms = CDP_FRIST_MS) {
  return new Promise((ok, nein) => {
    const t = setTimeout(() => {
      offen.delete(id);
      nein(
        new Error(
          `CDP ${method}: keine Antwort von Edge nach ${ms} ms (Browser hängt oder ist weg).`,
        ),
      );
    }, ms);
    offen.set(id, {
      ok: (r) => {
        clearTimeout(t);
        ok(r);
      },
      nein: (e) => {
        clearTimeout(t);
        nein(e);
      },
    });
  });
}
