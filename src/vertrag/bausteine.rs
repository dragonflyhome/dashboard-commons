//! Die Anzeige-Bausteine der Oberfläche als Daten (`Block` und seine
//! Zeilen). Die Oberfläche übersetzt `daten/*.toml` in diese Form
//! (`web/src/lib/daten.ts`) und zeichnet sie mit den Komponenten in
//! `web/src/lib/components/blocks`. Hier stehen nur die Typen; `cargo test`
//! schreibt sie nach `web/src/lib/vertrag/`.

use serde::Serialize;
use ts_rs::TS;

use crate::vertrag::bedeutung::Bedeutung;
use crate::vertrag::daten::FortschrittStufe;

// ----- Datenmodell (TypeScript dazu in web/src/lib/vertrag/, erzeugt) -----

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Progress {
        rows: Vec<ProgressRow>,
    },
    Ampel {
        rows: Vec<AmpelRow>,
    },
    Kpi {
        items: Vec<KpiItem>,
    },
    Kanban {
        columns: Vec<KanbanColumn>,
    },
    Gantt(Gantt),
    Flow(Flow),
    /// Fehler im Block; wird sichtbar angezeigt, damit er auffällt.
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
pub struct ProgressRow {
    pub label: String,
    pub percent: f32,
    /// „70 %“ oder „3 von 5“.
    pub text: String,
    pub note: String,
    /// Regel des Servers (Etappe 3a+): erreicht ab 100 %, ab-haelfte ab 50 %,
    /// sonst unter-haelfte; Farbe und Ton macht die Oberfläche daraus.
    pub stufe: FortschrittStufe,
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
pub struct AmpelRow {
    pub bedeutung: Bedeutung,
    pub label: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
pub struct KpiItem {
    pub label: String,
    pub value: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
pub struct KanbanColumn {
    pub title: String,
    pub bedeutung: Bedeutung,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(rename = "GanttData")]
pub struct Gantt {
    /// true: Achse sind Kalendertage (Tage seit 1.1.1); false: Plan-Monate.
    pub dates: bool,
    pub min: f64,
    pub max: f64,
    pub bars: Vec<GanttBar>,
    pub ticks: Vec<GanttTick>,
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
pub struct GanttBar {
    pub name: String,
    pub start: f64,
    pub end: f64,
    /// ISO-Datum, wenn `dates`.
    pub start_iso: Option<String>,
    pub end_iso: Option<String>,
    pub bedeutung: Bedeutung,
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
pub struct GanttTick {
    pub at: f64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(rename = "FlowData")]
pub struct Flow {
    pub nodes: Vec<FlowNode>,
    /// Indizes in `nodes`.
    pub edges: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
pub struct FlowNode {
    pub id: String,
    pub label: String,
    /// Spalte von links (längster Weg von einer Quelle).
    pub layer: usize,
    /// Position innerhalb der Spalte.
    pub slot: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_serialize_with_kind_tag() {
        let b = Block::Gantt(Gantt {
            dates: true,
            min: 0.0,
            max: 9.0,
            bars: vec![GanttBar {
                name: "Phase 0".into(),
                start: 0.0,
                end: 9.0,
                start_iso: Some("2026-11-01".into()),
                end_iso: Some("2026-11-10".into()),
                bedeutung: Bedeutung::Laeuft,
            }],
            ticks: Vec::new(),
        });
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains("\"kind\":\"gantt\""), "{json}");
        assert!(json.contains("\"start_iso\":\"2026-11-01\""), "{json}");
        let b = Block::Kpi { items: vec![KpiItem { label: "A".into(), value: "1".into(), note: String::new() }] };
        assert!(serde_json::to_string(&b).unwrap().contains("\"kind\":\"kpi\""));
        let b = Block::Error { message: "x".into() };
        assert_eq!(serde_json::to_value(&b).unwrap()["kind"], "error");
        let b = Block::Progress {
            rows: vec![ProgressRow { label: "A".into(), percent: 50.0, text: "50 %".into(), note: String::new(), stufe: FortschrittStufe::AbHaelfte }],
        };
        assert_eq!(serde_json::to_value(&b).unwrap()["rows"][0]["stufe"], "ab-haelfte");
    }
}
