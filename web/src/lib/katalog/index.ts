/**
 * `$katalog`: die Bausteine für Projektseiten, unter den Namen des
 * Zielbild-Katalogs. Bis Etappe 3a zeigen sie auf die heutigen Bausteine
 * (web/src/lib/components/blocks) mit dünnen Hüllen; die zweite Portierung
 * ändert nur die Dateien hinter diesem Namen, nie die Importe der Projekte.
 *
 * Daten-Bausteine nehmen `daten`: den Inhalt einer Datei aus `daten/`
 * (Typ aus `$vertrag`) oder gleich das Objekt aus `ladeDaten` (dann zeigen
 * sie Laden und Fehler selbst). Dazu optional `teil` (nur `[teil.<name>]`)
 * und `abgeleitet` (sonst das aus `ladeDaten`).
 *
 * Etappe 3a-3: dazu die Grundteile (Knopf, Schalter, Auswahl, Dialog,
 * Kurzinfo, Aufklapper; Reiter seit 3a-3 auf Bits UI) und die Tabelle, die
 * seitlich in ihrem eigenen Rahmen rollt. Das Toolkit (Bits UI in
 * web/vendor) bindet nur dieser Ordner ein (ESLint-Grenze); alle anderen
 * nehmen die Bausteine von hier. Alles mit Beispiel im /katalog.
 */

// Daten-Bausteine
export { default as Kennzahlen } from './Kennzahlen.svelte';
export { default as Ampel } from './Ampel.svelte';
export { default as Fortschritt } from './Fortschritt.svelte';
export { default as Zeitstrahl } from './Zeitstrahl.svelte';
export { default as Gantt } from './Gantt.svelte';
export { default as Kanban } from './Kanban.svelte';
export { default as Fluss } from './Fluss.svelte';
export { default as Fragen } from './Fragen.svelte';
// Etappe B2: Backlog (Spalten nach Stand und Status aus dem Datenspeicher, Wer ist dran)
export { default as Backlog } from './Backlog.svelte';
// Etappe B8: Plan der Projektseite (Etappen mit Stand und Zählung vom Server, Zeitstreifen nur aus Ist-Daten)
export { default as Plan } from './Plan.svelte';
// Etappe B3: Fristenliste (feste und bewegliche Fristen nach Datum, „hängt an …“)
export { default as Fristenliste } from './Fristenliste.svelte';
// Orte auf diesem PC mit dem Knopf „Öffnen“ (Entscheidung lokal-oeffnen)
export { default as Orte } from './Orte.svelte';

// Text-Bausteine
export { default as Abschnitt } from './Abschnitt.svelte';
export { default as Absatz } from './Absatz.svelte';
export { default as Text } from './Text.svelte';
export { default as Liste } from './Liste.svelte';
export { default as Hinweis } from './Hinweis.svelte';
// Etappe B1: Marke (Stand als Wort mit Bedeutung) und „Stand vom …“
export { default as Marke } from './Marke.svelte';
export { default as StandHinweis } from './StandHinweis.svelte';
export { default as Reiter } from './Reiter.svelte';

// Grundteile (Bits UI, Etappe 3a-3)
export { default as Knopf } from './grund/Knopf.svelte';
export { default as Schalter } from './grund/Schalter.svelte';
export { default as Auswahl } from './grund/Auswahl.svelte';
export { default as Dialog } from './grund/Dialog.svelte';
export { default as Kurzinfo } from './grund/Kurzinfo.svelte';
export { default as Aufklapper } from './grund/Aufklapper.svelte';
export { default as Wahl } from './grund/Wahl.svelte';
export { default as Tabelle } from './Tabelle.svelte';
// Etappe 3b: das Menü „Mehr“ der Kopfzeile
export { default as Menue } from './grund/Menue.svelte';
// Kopfleiste, Fassung 3: Schublade (Seitenleiste am Handy), Klappfeld (Glocke), Minilinie (PC)
export { default as Schublade } from './grund/Schublade.svelte';
export { default as Klappfeld } from './grund/Klappfeld.svelte';
export { default as Minilinie } from './Minilinie.svelte';
export type { MenuePunkt, Option, Spalte, Zeile } from './grund/typen.ts';

// Bilder aus assets/ des eigenen Projekts
export { default as Bild } from './Bild.svelte';

export type { Quelle } from './abgeleitet.ts';
