# Mitwirken

Beiträge sind willkommen. Dieses Repo nimmt sie auf einem einzigen Weg an: als Merge-Angebot in einem Issue.

## Kurzfassung

- Es gibt keinen Schreibzugriff auf dieses Repo, auch nicht auf Anfrage.
- Pull Requests sind ausgeschaltet.
- Gearbeitet wird im eigenen Fork.
- Angeboten wird über ein Issue, das Fork und Stand nennt.
- Die Betreuer prüfen, antworten im Issue und mergen selbst.

## Ablauf

1. **Fork anlegen.** Lege einen eigenen Fork an und arbeite dort in einem Zweig.
2. **Issue eröffnen.** Beschreibe im Issue, was sich ändert und warum. Nenne dazu:
   - die Adresse deines Forks,
   - den Stand, der geprüft werden soll: Zweigname und Commit-Kennung,
   - wie du die Änderung geprüft hast.
3. **Rückfragen beantworten.** Die Betreuer antworten im Issue. Besserst du nach, nenne dort die neue Commit-Kennung.
4. **Merge.** Nach der Annahme holen die Betreuer den genannten Stand aus deinem Fork und mergen ihn. Das Issue wird danach geschlossen.

Kleine Hinweise, Fehlerberichte und Fragen sind ebenfalls als Issue willkommen, auch ohne Fork.

## Nur Merge

In diesem Repo gilt „Merge-Only“:

- Beiträge kommen als Merge-Commit auf den Hauptzweig.
- Kein Squash, kein Rebase durch die Betreuer, kein Force-Push.
- Die Geschichte deines Zweigs bleibt erhalten. Halte sie deshalb lesbar: wenige, in sich stimmige Commits.

## Was ein Beitrag einhalten muss

Vor jedem Hochladen prüft ein Werkzeug die ganze Geschichte, die hinausgeht. Ein Beitrag wird nur angenommen, wenn er diese Prüfung besteht.

- **Commit-Meldungen** folgen Conventional Commits, zum Beispiel `fix(plan): leere Etappe zählt nicht`.
- **Keine Anhänge** in Commit-Meldungen: kein `Co-Authored-By`, kein `Signed-off-by`, kein Hinweis auf Werkzeuge.
- **Keine signierten Commits.** Commits, die in der Weboberfläche von GitHub entstehen, sind signiert und werden abgewiesen. Committe lokal.
- **Keine persönlichen Angaben.** Verwende als Autor die noreply-Adresse deines GitHub-Kontos. Namen, Adressen, Pfade des eigenen Rechners und Zugangsdaten gehören weder in Code noch in Texte, Tests oder Commit-Meldungen.
- **Was einmal in einem Commit stand, geht mit.** Ein späterer Commit, der etwas entfernt, hilft nicht. Bereinige deinen Zweig, bevor du ihn anbietest.
- **Prüfwerkzeuge** laufen vor dem Commit: `rustfmt` und `clippy` für Rust, `prettier`, `eslint`, `stylelint` und `svelte-check` für die Oberfläche.
- **Tests** gehören zu jeder neuen Logik.
- **Kleine Änderungen** sind leichter zu prüfen als große. Ein Issue je Anliegen.

## Lizenz

Mit dem Angebot stellst du deinen Beitrag unter dieselbe Lizenz wie die Datei, die er ändert: Code unter AGPL-3.0-or-later (einzelne übernommene Dateien unter GPL-3.0-or-later, siehe LIESMICH.txt), die Dokumentation in `docs/anleitung` und `docs/zielbild` unter CC0-1.0. Biete nur an, was du selbst geschrieben hast oder was unter einer dazu passenden Lizenz steht, und nenne im zweiten Fall die Quelle.

## Sicherheitslücken

Bitte beschreibe eine Sicherheitslücke nicht in einem öffentlichen Issue. Schreibe stattdessen eine E-Mail an ranke2@proton.me, verschlüsselt mit dem öffentlichen Schlüssel in `KONTAKT-PGP.asc` im Wurzelordner (Fingerabdruck `75E3 9C35 3011 E6DD 78AD B7E0 2919 8C6D 0B67 9E0E`). Auf demselben Weg gehen auch andere vertrauliche Anliegen.
