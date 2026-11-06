//! Die Module der geprüften Kisten aus `kisten.toml` ([zusammenbau]).
//! Zwischen den Marken je Kiste genau eine Zeile `dashboard::eintrag(<kiste>::<typ>)`;
//! der Wächter-Test vergleicht sie mit `kisten.toml`.

/// Alle Module dieser Fassung, in der Reihenfolge von `kisten.toml`.
pub fn module() -> Vec<dashboard::ModulEintrag> {
    vec![
        // KISTEN-ANFANG
        // KISTEN-ENDE
    ]
}
