//! Systemaufrufe unter macOS und Linux, je ein kleiner Block an einer Stelle.
//! Alles andere im Programm bleibt ohne `unsafe` außerhalb von Windows.

/// Die Benutzernummer dieses Prozesses.
pub fn eigene_uid() -> u32 {
    // SAFETY: `getuid` hat keine Vorbedingung, liest nur die Benutzernummer
    // des Prozesses und kann nicht scheitern.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;

    /// Eine Datei, die dieser Prozess anlegt, gehört seiner Benutzernummer.
    #[test]
    fn eigene_uid_ist_die_des_prozesses() {
        let datei = std::env::temp_dir().join(format!("dashboard-uid-{}", std::process::id()));
        std::fs::write(&datei, b"x").unwrap();
        let uid = std::fs::metadata(&datei).unwrap().uid();
        let _ = std::fs::remove_file(&datei);
        assert_eq!(super::eigene_uid(), uid);
    }
}
