//! Windows-Ereignisse für den Verlauf (Etappe 2e): Hotfixes, Update-Ereignisse
//! (WindowsUpdateClient, Ereignis 19), Starts (Kernel-General, Ereignis 12)
//! und die Installation von Windows, in der Form `vertrag::verlauf::Ereignis`.
//!
//! Kennungen (`[plan_2e_bestaetigt]`, mit der Änderung der Architektur):
//!
//! - Updates mit KB: `update:<KB>:<JJJJ-MM-TT>`. Gibt es dieselbe KB mehrmals
//!   am selben Tag (Defender-Signaturen KB2267602 mit neuer Version), kommt
//!   die Version aus dem Titel dazu (`update:KB2267602:2026-10-30:1.439.141.0`),
//!   ohne Version die Minute (`…:08-12`). Hotfix und Update-Ereignis mit
//!   derselben KB am selben Tag sind ein Ereignis.
//! - Updates ohne KB: `update:<JJJJ-MM-TTTHH-MM>:<8 Zeichen hex aus dem Titel>`.
//! - Starts: `start:<Zeit auf die Sekunde mit Zone>`.
//! - Windows installiert: `windows-installiert:<JJJJ-MM-TT>`.
//!
//! Der Tag ist der Tag in Ortszeit. Dass eine Kennung über mehrere Läufe
//! gleich bleibt, sichert der Datenspeicher (`speicher::pc_ereignisse`): Ein
//! Ereignis, das er schon kennt (gleiche KB, gleicher Tag, gleiche Zeit),
//! behält seine Kennung, auch wenn später weitere am selben Tag dazukommen.
//!
//! Routine (`art = routine`): Starts, Defender-Signaturen (Titel mit „Security
//! Intelligence“ oder KB2267602) und Store-Updates; sonst `erledigt`.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, Local, NaiveDate};
use sha2::{Digest, Sha256};

use super::sysapps::Hotfix;
use crate::vertrag::verlauf::{Art, Ereignis, Genauigkeit, Quelle, tagesanfang, zeit_text};

/// Die KB der Defender-Signaturen.
pub const KB_SIGNATUREN: &str = "KB2267602";

/// Ein Update-Ereignis aus der Ereignisanzeige.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateRoh {
    pub zeit: DateTime<FixedOffset>,
    pub titel: String,
}

/// Die Installation von Windows (Registry `InstallDate`) mit Produkt und Version.
#[derive(Debug, Clone, PartialEq)]
pub struct Installiert {
    pub zeit: DateTime<Local>,
    pub produkt: String,
}

/// Die KB-Nummer in einem Titel oder einer Hotfix-Kennung (`KB5066835`).
pub fn kb_aus(text: &str) -> Option<String> {
    let gross = text.to_ascii_uppercase();
    let mut rest = gross.as_str();
    while let Some(i) = rest.find("KB") {
        let ziffern: String = rest[i + 2..].chars().take_while(char::is_ascii_digit).collect();
        if ziffern.len() >= 4 {
            return Some(format!("KB{ziffern}"));
        }
        rest = &rest[i + 2..];
    }
    None
}

/// Die Version aus einem Titel wie „… (Version 1.439.141.0) …“.
pub fn version_aus(titel: &str) -> Option<String> {
    let i = titel.find("Version ")?;
    let v: String = titel[i + 8..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let v = v.trim_end_matches('.');
    (!v.is_empty() && v.chars().any(|c| c.is_ascii_digit())).then(|| v.to_string())
}

/// Store-Updates heißen in der Ereignisanzeige wie die Store-Kennung des
/// Pakets: zwölf Zeichen, Bindestrich, Paketname (`9NBLGGH4NNS1-Microsoft.DesktopAppInstaller`).
pub fn ist_store_update(titel: &str) -> bool {
    let t = titel.trim();
    let kopf: Vec<char> = t.chars().take(13).collect();
    let store_kennung = kopf.len() == 13
        && kopf[12] == '-'
        && kopf[..12].iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && kopf[..12].iter().any(char::is_ascii_digit);
    store_kennung || t.contains("Microsoft Store")
}

/// Ist ein Update Routine (Defender-Signaturen, Store-Updates)?
pub fn ist_routine_update(titel: &str, kb: Option<&str>) -> bool {
    titel.contains("Security Intelligence") || kb == Some(KB_SIGNATUREN) || ist_store_update(titel)
}

/// Acht Zeichen hex aus dem Titel (für Updates ohne KB).
fn kurz_hash(titel: &str) -> String {
    Sha256::digest(titel.as_bytes())[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// Minute in Ortszeit als Teil einer Kennung: `08-12`.
fn minute(t: &DateTime<FixedOffset>) -> String {
    t.format("%H-%M").to_string()
}

fn art_update(titel: &str, kb: Option<&str>) -> Art {
    if ist_routine_update(titel, kb) { Art::Routine } else { Art::Erledigt }
}

/// Ereignis eines Updates aus der Ereignisanzeige (auf die Minute).
fn update_ereignis(kennung: String, u: &UpdateRoh, kb: Option<&str>) -> Ereignis {
    Ereignis {
        kennung,
        zeit: zeit_text(&u.zeit),
        genauigkeit: Genauigkeit::Minute,
        bis: None,
        titel: u.titel.trim().to_string(),
        text: None,
        quelle: Quelle::Update,
        art: art_update(&u.titel, kb),
        verweis: None,
    }
}

/// Hotfixes und Update-Ereignisse, nach KB und Tag zusammengeführt.
pub fn updates(hotfixes: &[Hotfix], updates: &[UpdateRoh]) -> Vec<Ereignis> {
    // Je (KB, Tag): Update-Ereignisse in Zeitfolge und der Hotfix.
    type Gruppe<'a> = (Vec<&'a UpdateRoh>, Option<&'a Hotfix>);
    let mut gruppen: BTreeMap<(String, NaiveDate), Gruppe<'_>> = BTreeMap::new();
    let mut aus = Vec::new();
    for u in updates {
        match kb_aus(&u.titel) {
            Some(kb) => gruppen.entry((kb, u.zeit.date_naive())).or_default().0.push(u),
            None => {
                let kennung = format!("update:{}:{}", u.zeit.format("%Y-%m-%dT%H-%M"), kurz_hash(u.titel.trim()));
                aus.push(update_ereignis(kennung, u, None));
            }
        }
    }
    for h in hotfixes {
        let (Some(kb), Ok(tag)) = (kb_aus(&h.id), NaiveDate::parse_from_str(h.installed_on.trim(), "%Y-%m-%d")) else {
            continue;
        };
        gruppen.entry((kb, tag)).or_default().1.get_or_insert(h);
    }
    for ((kb, tag), (mut ereignisse, hotfix)) in gruppen {
        let basis = format!("update:{kb}:{}", tag.format("%Y-%m-%d"));
        ereignisse.sort_by_key(|u| u.zeit);
        match ereignisse.as_slice() {
            [] => {
                let Some(h) = hotfix else { continue };
                let titel = format!("{} {}", h.id.trim(), h.description.trim()).trim().to_string();
                aus.push(Ereignis {
                    kennung: basis,
                    zeit: zeit_text(&tagesanfang(tag)),
                    genauigkeit: Genauigkeit::Tag,
                    bis: None,
                    art: art_update(&titel, Some(&kb)),
                    titel,
                    text: None,
                    quelle: Quelle::Update,
                    verweis: None,
                });
            }
            [u] => aus.push(update_ereignis(basis, u, Some(&kb))),
            mehrere => {
                let versionen: Vec<Option<String>> = mehrere.iter().map(|u| version_aus(&u.titel)).collect();
                let alle_verschieden =
                    versionen.iter().all(Option::is_some) && versionen.iter().collect::<std::collections::BTreeSet<_>>().len() == versionen.len();
                let mut gesehen = std::collections::BTreeSet::new();
                for (u, v) in mehrere.iter().zip(&versionen) {
                    let zusatz = match v {
                        Some(v) if alle_verschieden => v.clone(),
                        _ => minute(&u.zeit),
                    };
                    let mut kennung = format!("{basis}:{zusatz}");
                    let mut n = 2;
                    while !gesehen.insert(kennung.clone()) {
                        kennung = format!("{basis}:{zusatz}-{n}");
                        n += 1;
                    }
                    aus.push(update_ereignis(kennung, u, Some(&kb)));
                }
            }
        }
    }
    aus
}

/// Starts als Ereignisse (Routine, auf die Sekunde in der Kennung).
pub fn starts(zeiten: &[DateTime<FixedOffset>]) -> Vec<Ereignis> {
    let mut aus: Vec<Ereignis> = zeiten
        .iter()
        .map(|t| Ereignis {
            kennung: format!("start:{}", zeit_text(t)),
            zeit: zeit_text(t),
            genauigkeit: Genauigkeit::Minute,
            bis: None,
            titel: "Windows gestartet".to_string(),
            text: None,
            quelle: Quelle::Start,
            art: Art::Routine,
            verweis: None,
        })
        .collect();
    aus.sort_by(|a, b| a.kennung.cmp(&b.kennung));
    aus.dedup_by(|a, b| a.kennung == b.kennung);
    aus
}

/// Die Installation von Windows als Ereignis (auf den Tag).
pub fn installiert(i: &Installiert) -> Ereignis {
    let tag = i.zeit.date_naive();
    let produkt = i.produkt.trim();
    Ereignis {
        kennung: format!("windows-installiert:{}", tag.format("%Y-%m-%d")),
        zeit: zeit_text(&i.zeit),
        genauigkeit: Genauigkeit::Minute,
        bis: None,
        titel: if produkt.is_empty() { "Windows installiert".to_string() } else { format!("{produkt} installiert") },
        text: None,
        quelle: Quelle::WindowsInstalliert,
        art: Art::Erledigt,
        verweis: None,
    }
}

/// Alles aus einem Lauf des Sammlers.
pub fn alle(hotfixes: &[Hotfix], ereignisse: &[UpdateRoh], start_zeiten: &[DateTime<FixedOffset>], inst: Option<&Installiert>) -> Vec<Ereignis> {
    let mut aus = updates(hotfixes, ereignisse);
    aus.extend(starts(start_zeiten));
    aus.extend(inst.map(installiert));
    aus
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn zeit(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    pub(crate) fn signatur(z: &str, version: &str) -> UpdateRoh {
        UpdateRoh {
            zeit: zeit(z),
            titel: format!("Security Intelligence-Update für Microsoft Defender Antivirus - KB2267602 (Version {version}) - Current Channel (Broad)"),
        }
    }

    fn hotfix(id: &str, tag: &str) -> Hotfix {
        Hotfix { id: id.to_string(), description: "Security Update".to_string(), installed_on: tag.to_string() }
    }

    #[test]
    fn kb_version_und_routine() {
        assert_eq!(kb_aus("2026-10 Kumulatives Update für Windows 11 (KB5066835)").as_deref(), Some("KB5066835"));
        assert_eq!(kb_aus("kb5066835").as_deref(), Some("KB5066835"));
        assert_eq!(kb_aus("KB12 und KB5066835"), Some("KB5066835".into()));
        assert_eq!(kb_aus("9NBLGGH4NNS1-Microsoft.DesktopAppInstaller"), None);
        assert_eq!(version_aus(&signatur("2026-10-30T08:00:00+01:00", "1.439.141.0").titel).as_deref(), Some("1.439.141.0"));
        assert_eq!(version_aus("Kein Versionstext"), None);
        assert!(ist_routine_update(&signatur("2026-10-30T08:00:00+01:00", "1").titel, Some(KB_SIGNATUREN)));
        assert!(ist_routine_update("Update", Some("KB2267602")));
        assert!(ist_store_update("9NBLGGH4NNS1-Microsoft.DesktopAppInstaller"));
        assert!(ist_store_update("9WZDNCRFJ3PS-Microsoft.Office"));
        assert!(!ist_store_update("2026-10 Kumulatives Update (KB5066835)"));
        assert!(!ist_routine_update("2026-10 Kumulatives Update (KB5066835)", Some("KB5066835")));
    }

    /// Abnahme 2e: drei Signatur-Updates (KB2267602) an einem Tag sind drei
    /// Ereignisse, mit der Version in der Kennung, alle Routine.
    #[test]
    fn drei_signatur_updates_an_einem_tag() {
        let roh = [
            signatur("2026-10-30T14:05:00+01:00", "1.439.150.0"),
            signatur("2026-10-30T08:12:00+01:00", "1.439.141.0"),
            signatur("2026-10-30T20:40:00+01:00", "1.439.160.0"),
        ];
        let e = updates(&[], &roh);
        let k: Vec<&str> = e.iter().map(|e| e.kennung.as_str()).collect();
        assert_eq!(k, ["update:KB2267602:2026-10-30:1.439.141.0", "update:KB2267602:2026-10-30:1.439.150.0", "update:KB2267602:2026-10-30:1.439.160.0"]);
        assert!(e.iter().all(|e| e.art == Art::Routine && e.quelle == Quelle::Update && e.genauigkeit == Genauigkeit::Minute));
        // Ohne Version (oder mit gleicher) die Minute.
        let mut gleich = roh.clone();
        gleich[2].titel = gleich[1].titel.clone();
        let k: Vec<String> = updates(&[], &gleich).into_iter().map(|e| e.kennung).collect();
        assert_eq!(k, ["update:KB2267602:2026-10-30:08-12", "update:KB2267602:2026-10-30:14-05", "update:KB2267602:2026-10-30:20-40"]);
        // Nur eines am Tag: ohne Zusatz.
        assert_eq!(updates(&[], &roh[..1])[0].kennung, "update:KB2267602:2026-10-30");
    }

    /// Hotfix und Update-Ereignis sind eins, wenn KB und Tag gleich sind;
    /// sonst zwei.
    #[test]
    fn hotfix_und_update_nach_kb_und_tag() {
        let u = UpdateRoh { zeit: zeit("2026-10-15T09:30:00+02:00"), titel: "2026-10 Kumulatives Update für Windows 11 (KB5066835)".into() };
        let e = updates(&[hotfix("KB5066835", "2026-10-15")], std::slice::from_ref(&u));
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].kennung, "update:KB5066835:2026-10-15");
        assert_eq!((e[0].genauigkeit, e[0].art), (Genauigkeit::Minute, Art::Erledigt));
        assert!(e[0].titel.contains("Kumulatives Update"));
        // Anderer Tag: zwei Ereignisse.
        let e = updates(&[hotfix("KB5066835", "2026-10-14")], std::slice::from_ref(&u));
        let k: Vec<&str> = e.iter().map(|e| e.kennung.as_str()).collect();
        assert_eq!(k, ["update:KB5066835:2026-10-14", "update:KB5066835:2026-10-15"]);
        assert_eq!(e[0].genauigkeit, Genauigkeit::Tag);
        assert_eq!(e[0].titel, "KB5066835 Security Update");
        // Andere KB am selben Tag: auch zwei.
        assert_eq!(updates(&[hotfix("KB5000001", "2026-10-15")], &[u]).len(), 2);
        // Hotfix ohne Datum fehlt.
        assert!(updates(&[hotfix("KB5000001", "")], &[]).is_empty());
    }

    #[test]
    fn update_ohne_kb_starts_und_installation() {
        let u = UpdateRoh { zeit: zeit("2026-10-30T08:12:59+01:00"), titel: "9NBLGGH4NNS1-Microsoft.DesktopAppInstaller".into() };
        let e = updates(&[], std::slice::from_ref(&u));
        assert!(e[0].kennung.starts_with("update:2026-10-30T08-12:"), "{}", e[0].kennung);
        assert_eq!(e[0].kennung.len(), "update:2026-10-30T08-12:".len() + 8);
        assert_eq!(e[0].art, Art::Routine);
        assert_eq!(updates(&[], &[u])[0].kennung, e[0].kennung, "gleicher Titel, gleiche Kennung");
        let s = starts(&[zeit("2026-10-30T07:59:01+01:00"), zeit("2026-10-30T07:59:01+01:00")]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kennung, "start:2026-10-30T07:59:01+01:00");
        assert_eq!((s[0].art, s[0].quelle), (Art::Routine, Quelle::Start));
        let i = installiert(&Installiert {
            zeit: crate::vertrag::verlauf::tagesanfang(NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()),
            produkt: "Windows 11 Pro".into(),
        });
        assert_eq!(i.kennung, "windows-installiert:2026-10-01");
        assert_eq!((i.titel.as_str(), i.quelle), ("Windows 11 Pro installiert", Quelle::WindowsInstalliert));
    }
}
