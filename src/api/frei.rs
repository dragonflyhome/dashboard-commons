//! Etappe B2 (Architektur, Nacharbeit): was ohne Windows-Anmeldung hinausgeht.
//! Persönlicher Text (Fragen, Titel und Notizen von Aufgaben und Arbeiten,
//! Meldungstexte, offene Punkte `open`, Befund-Sätze der Prüfung) nur mit
//! Anmeldung; frei bleiben Zahlen. Die Regel steht je Weg als Zugangsregel
//! (geschützt in `sicherheit::geschuetzt` oder Felder weglassen); hier nur
//! die neutralen Sätze an Stelle eines Fehlersatzes. Prüft `frei_test.rs`.

/// An Stelle eines Fehlersatzes zu einer Seite (er kann Inhalt zitieren).
pub const SATZ_SEITE_KAPUTT: &str = "Diese Seite lässt sich nicht lesen. Was genau falsch ist, steht in der Prüfung, mit Windows-Anmeldung.";

/// An Stelle des Satzes eines Vertragsfehlers in einer Datei aus `daten/`.
pub const SATZ_DATEI_KAPUTT: &str = "Diese Datei passt nicht zum Vertrag. Was genau falsch ist, steht in der Prüfung, mit Windows-Anmeldung.";
