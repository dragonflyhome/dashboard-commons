//! Die Anbindung an den Anmelde-Schlüsselbund von macOS: genau drei Aufrufe
//! der Kiste `security-framework` (Dialoge sperren, lesen, hinzufügen). Alles,
//! was entscheidet, steht in `mischform` und ist dort auf jeder Plattform
//! getestet; diese Datei bleibt so dünn, dass sie nur am Gerät geprüft wird.
//!
//! Regeln, die hier gelten:
//! - **Nie ein Dialog.** Der Dialog „Zugriff erlauben“ kommt aus der
//!   Zugriffsliste des Eintrags. Jeder Zugriff läuft darum unter
//!   `SecKeychain::disable_user_interaction()`; statt einer Nachfrage kommt
//!   eine Nummer zurück. Der Wächter ist benannt gebunden (`_sperre`): Mit
//!   `let _ =` endete er noch in derselben Zeile. Die Abschaltung gilt für den
//!   ganzen Prozess; der Aufrufer hält darum eine Sperre, solange einer der
//!   Aufrufe hier läuft (`verschluesselung::schluessel`).
//! - **Nur hinzufügen.** `anlegen` meldet „gibt es schon“ (-25299) und ersetzt
//!   nie. Die Aufrufe der Kiste, die anlegen oder ersetzen, kommen hier nicht vor.
//! - **Nie ändern, nie entfernen.** Kein Code des Servers ändert oder entfernt
//!   den Eintrag.
//! - Die Angaben für den Abgleich über iCloud und für die Zugänglichkeit setzt
//!   der Server nicht: Im Anmelde-Schlüsselbund werden sie nicht gespeichert
//!   (am Gerät gemessen), und der Weg, der nur hinzufügt, bietet sie nicht an.

use security_framework::os::macos::keychain::SecKeychain;
use security_framework::passwords::{PasswordOptions, generic_password};
use zeroize::Zeroizing;

/// Liest den Eintrag (allgemeines Kennwort) für `dienst` und `konto`. Als
/// Fehler die Nummer des Schlüsselbunds.
pub fn lesen(dienst: &str, konto: &str) -> Result<Zeroizing<Vec<u8>>, i32> {
    let _sperre = SecKeychain::disable_user_interaction().map_err(|e| e.code())?;
    generic_password(PasswordOptions::new_generic_password(dienst, konto)).map(Zeroizing::new).map_err(|e| e.code())
}

/// Fügt den Eintrag im Anmelde-Schlüsselbund hinzu. Gibt es ihn schon, kommt
/// -25299 zurück, und der vorhandene bleibt, wie er ist.
pub fn anlegen(dienst: &str, konto: &str, inhalt: &[u8; 32]) -> Result<(), i32> {
    let _sperre = SecKeychain::disable_user_interaction().map_err(|e| e.code())?;
    // Der Standard-Schlüsselbund des Kontos ist der Anmelde-Schlüsselbund.
    let anmeldung = SecKeychain::default().map_err(|e| e.code())?;
    anmeldung.add_generic_password(dienst, konto, inhalt).map_err(|e| e.code())
}
