//! Alles, was den PC selbst misst oder beschreibt: Messwerte (`system`),
//! Windows-Anwendungen und Dienste (`sysapps`), Ordnergrößen (`inventory`)
//! die handgepflegten Eckdaten aus `data/pc.toml` (`pcdata`) und ob der Inhaber
//! angemeldet ist (`sitzung`, über WTS) und die Windows-Ereignisse für den Verlauf
//! (`ereignisse`, Etappe 2e).

pub mod ereignisse;
pub mod inventory;
pub mod pcdata;
pub mod sitzung;
pub mod sysapps;
pub mod system;
