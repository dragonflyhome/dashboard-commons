# Mitwirken

Beiträge sind willkommen. Dieses Repo nimmt sie auf einem einzigen Weg an: als Merge-Angebot in einem Issue.

## Kurzfassung

- Es gibt keinen Schreibzugriff auf dieses Repo, auch nicht auf Anfrage.
- Es gibt keinen direkten Push und keine Zweige in diesem Repo.
- Pull Requests sind ausgeschaltet.
- Gearbeitet wird im eigenen Fork.
- Angeboten wird über ein Issue mit dem Formular „Merge-Angebot“, das die Fork-URL und die Ref nennt.
- Die Betreuer prüfen, antworten im Issue und mergen selbst, immer mit einem Merge-Commit.

Ein Pull Request würde auf dem Server zusammengeführt, an der Prüfung vorbei, die jeder Stand vor dem Hochladen durchläuft. Darum holen die Betreuer einen angebotenen Stand selbst aus dem Fork, prüfen ihn und laden erst danach hoch.

## Ablauf

1. **Fork anlegen.** Lege einen eigenen Fork an und arbeite dort in einem Zweig, aufgesetzt auf den aktuellen Hauptzweig `main` dieses Repos (keine eigene Wurzel, keine fremde Geschichte).
2. **Committen und in den Fork pushen.** Die Regeln unten gelten für jeden Commit des Zweigs, nicht nur für den letzten.
3. **Issue eröffnen.** Nutze das Formular „Merge-Angebot“ (`.github/ISSUE_TEMPLATE/merge-angebot.yml`). Beschreibe darin, was sich ändert und warum. Nenne dazu:
   - die Adresse deines Forks (Fork-URL),
   - die Ref, die geprüft werden soll: Zweigname oder, am genauesten, die volle Commit-Kennung,
   - wie du die Änderung geprüft hast.
4. **Rückfragen beantworten.** Die Betreuer antworten im Issue. Besserst du nach, kommen die Änderungen als weitere Commits auf denselben Zweig; nenne dort die neue Commit-Kennung. Schreibe einen schon angebotenen Zweig nicht um, außer die Betreuer bitten darum.
5. **Merge.** Nach der Annahme holen die Betreuer den genannten Stand aus deinem Fork und führen ihn mit `git merge --no-ff` zusammen. Das Issue wird danach geschlossen.

Kleine Hinweise, Fehlerberichte und Fragen sind ebenfalls als Issue willkommen, auch ohne Fork.

## Nur Merge

In diesem Repo gilt „Merge-Only“:

- Beiträge kommen als Merge-Commit auf den Hauptzweig.
- Kein Squash, kein Rebase durch die Betreuer, kein Force-Push.
- Die Geschichte deines Zweigs bleibt erhalten. Halte sie deshalb lesbar: wenige, in sich stimmige Commits.

## Was ein Beitrag einhalten muss

Vor jedem Hochladen prüft ein Werkzeug die ganze Geschichte, die hinausgeht. Es weist den ganzen Stand ab, wenn ein einziger Commit eine dieser Regeln verletzt. Ein Beitrag wird darum erst angenommen, wenn der Zweig im Fork diese Prüfung besteht.

- **Commit-Meldungen** folgen Conventional Commits: `typ(bereich)!: text`, mit `typ` aus feat, fix, docs, style, refactor, perf, test, build, ci, chore oder revert; Bereich und `!` (ein Bruch) nach Bedarf, nach dem Doppelpunkt ein Leerzeichen. Zum Beispiel `fix(plan): leere Etappe zählt nicht`. Das gilt auch für Merge-Commits im Fork.
- **Keine Anhänge** in Commit-Meldungen: kein `Co-Authored-By`, kein `Signed-off-by`, kein Fuß `BREAKING CHANGE:` (ein Bruch steht als `!` in der ersten Zeile), auch keine anderen Trailer und kein Hinweis auf Werkzeuge. Keine Namen von KI-Assistenten, Modellen oder ihren Anbietern, weder in der Meldung noch im Namen von Autor oder Committer.
- **Autor- und Committer-Adresse** ist die noreply-Adresse deines GitHub-Kontos (`<zahl>+<login>@users.noreply.github.com` oder `<login>@users.noreply.github.com`, zu finden in den GitHub-Einstellungen unter „Emails“) oder eine erfundene Adresse auf `.invalid` oder `.example`. Jede andere Adresse weist das Werkzeug ab. Commits mit der Adresse der Betreuer lehnen die Betreuer ab. Setze die Angaben einmal im Klon deines Forks:

  ```
  git config user.email "<zahl>+<login>@users.noreply.github.com"
  git config user.name "<login oder ein Pseudonym>"
  ```

- **Keine signierten Commits.** Ein Commit hat nur die Kopfzeilen tree, parent, author und committer. Commits, die in der Weboberfläche von GitHub entstehen, sind signiert und werden abgewiesen; committe lokal. Wer sonst signiert, committet mit `git commit --no-gpg-sign` oder schreibt den Zweig vor dem Angebot unsigniert neu, aufgesetzt auf `main` dieses Repos (als Remote `upstream`) und mit der Adresse aus `user.email`:

  ```
  git fetch upstream
  git -c commit.gpgsign=false rebase --force-rebase --exec "git commit --amend --no-edit --no-gpg-sign --reset-author" upstream/main
  ```

- **Keine Änderungen an den Prüfpfaden:** alles unter `scripts/`, `.cargo/` und `.github/` sowie `tests/vor_push.rs` und `KONTAKT-PGP.asc`. Ein Commit mit fremdem Autor oder Committer, der dort etwas ändert, wird abgewiesen, auch ein Merge-Commit im Fork. Wer dort etwas braucht, beschreibt es im Issue; die Betreuer übernehmen solche Änderungen als eigenen Commit.
- **Keine persönlichen Angaben.** Namen, Adressen, Pfade des eigenen Rechners, Rechner- und Netzwerknamen und Zugangsdaten gehören weder in Code noch in Texte, Tests oder Commit-Meldungen. Beispiele nur mit erfundenen Werten.
- **Nur Orte, die das Repo schon hat.** Keine weiteren Markdown-Dateien, keine gebauten Dateien (`target`, `web/build`, `node_modules`), keine Archive.
- **Was einmal in einem Commit stand, geht mit.** Ein späterer Commit, der etwas entfernt, hilft nicht. Bereinige deinen Zweig, bevor du ihn anbietest.
- **Prüfwerkzeuge** laufen vor dem Commit: `rustfmt` und `clippy` für Rust, `prettier`, `eslint`, `stylelint` und `svelte-check` für die Oberfläche.
- **Tests** gehören zu jeder neuen Logik.
- **Kleine Änderungen** sind leichter zu prüfen als große. Ein Issue je Anliegen; bei einem größeren Vorhaben vorher ein Issue mit der Idee.

## Vor dem Commit prüfen

Im Ordner des Repos:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Im Ordner `web` (beim ersten Mal `npm ci`):

```
npm run lint
npm run check
npm run test
```

Ein Teil der Tests braucht Windows mit Git und PowerShell; fehlt etwas, überspringen sie sich mit einem Hinweis.

## Hinweise

- Die Beschreibung im Issue sagt, warum. Was sich ändert, zeigt der Diff.
- `main` nachziehen per Merge ist in Ordnung, mit einer Meldung nach Conventional Commits (`git pull` und `git merge` ohne `-m` schreiben eine, die das Werkzeug abweist):

  ```
  git merge --no-ff -m "chore(merge): main nachgezogen" upstream/main
  ```

- Ein Test, der ohne die Änderung scheitert und mit ihr besteht, ist der beste Beleg.
- Alles im Issue ist öffentlich: keine privaten Angaben, keine Pfade des eigenen Rechners, keine Zugangsdaten, auch nicht in angehängten Ausgaben.

## Lizenz

Mit dem Angebot stellst du deinen Beitrag unter dieselbe Lizenz wie die Datei, die er ändert: Code unter AGPL-3.0-or-later (einzelne übernommene Dateien unter GPL-3.0-or-later, siehe LIESMICH.txt), die Dokumentation in `docs/anleitung` und `docs/zielbild` unter CC0-1.0. Biete nur an, was du selbst geschrieben hast oder was unter einer dazu passenden Lizenz steht, und nenne im zweiten Fall die Quelle.

## Sicherheitslücken

Bitte beschreibe eine Sicherheitslücke nicht in einem öffentlichen Issue. Schreibe stattdessen eine E-Mail an ranke2@proton.me, verschlüsselt mit dem öffentlichen Schlüssel in `KONTAKT-PGP.asc` im Wurzelordner (Fingerabdruck `75E3 9C35 3011 E6DD 78AD B7E0 2919 8C6D 0B67 9E0E`). Auf demselben Weg gehen auch andere vertrauliche Anliegen.
