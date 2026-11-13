<!--
LIESMICH: Vorlage für eine Projektseite in neuer Form

So fängst du an
- Den Ordner projects/_vorlage nach projects/<slug> kopieren (Kleinbuchstaben,
  Ziffern, Bindestriche). Ordner mit _ am Anfang lädt das Dashboard nie.
- projekt.toml anpassen: Titel, Status, Satz für die Kachel, Chat, offene Punkte.
- In daten/ nur die Dateien behalten, die die Seite braucht. daten/plan.toml
  nennt die Etappen (nur Absicht); jede Arbeit im Tracker nennt ihre Etappe.
- Prüfen: dashboard.exe pruefe <slug>. Erst wenn das grün ist, kommt die
  Seite in den Bau; bis dahin zeigt das Dashboard einen einfachen Aufbau.
- Ansehen: /projects/<slug>.

Stand pflegt niemand von Hand
- Über dieser Datei zeigt das Dashboard auf jeder Projektseite selbst vier
  Teile: Stand, Aufgabenliste, Plan (aus daten/plan.toml) und Zeitleiste. Was
  läuft, was fertig ist, wie weit und seit wann, rechnet der Server aus dem
  Tracker. Diese Seite zeigt nur Inhalt: Fluss, Tabellen, Texte, inhaltliche
  Zeiträume. Kein Fortschritt in Prozent, kein Kanban, keine Gantt-Balken mit
  Stand-Farben; pruefe meldet so etwas als Hinweis.

Drei Sprachen, drei Aufgaben
- daten/*.toml: alles, was du von Hand einträgst (Etappen, Termine, Kosten,
  Texte für Bausteine). Ändert sich sofort, ohne Bau.
- Seite.svelte: wie die Seite aussieht. Texte und Überschriften dürfen hier
  stehen. Keine Zahlen, keine Daten, keine Reihen, keine Farben: die kommen aus
  daten/*.toml über ladeDaten. Was sich zählen oder rechnen lässt (wie viel
  erledigt, Tage bis), rechnet der Server: berechnet in kennzahlen.toml.
- Eigene Typen und Hilfen in <name>.ts in diesem Ordner.

Regeln für diese Datei
- Importe nur aus $katalog (Bausteine), $projekt (ladeDaten, ladeKopf),
  $vertrag (Typen), alle drei ohne Unterpfad, und aus .svelte- und
  .ts-Dateien dieses Ordners. Keine .js- oder .css-Dateien im Ordner.
- Svelte mit Runes ($state, $derived, $props), lang="ts", TypeScript strict, kein any.
- Kein fetch, kein {@html}, kein localStorage, keine Links auf fremde Seiten.
- Kein :global im Stil, kein svelte:head, svelte:window, svelte:body,
  svelte:element, kein import.meta (außer import.meta.env).
- Farben, Abstände, Schriften nur als Token: var(...) im style-Block.
- Keine persönlichen Daten, keine Passwörter.

Bausteine in $katalog
- Daten: Kennzahlen, Ampel (inhaltliche Risiken mit stand), Zeitstrahl und Gantt
  (inhaltliche Zeiträume), Fluss, Fragen,
  Orte (Dateien und Ordner auf diesem PC mit dem Knopf „Öffnen“, daten/orte.toml).
  Jeder nimmt daten = das Objekt aus ladeDaten (zeigt Laden und Fehler selbst),
  dazu teil = "<name>", wenn die Datei Teile hat ([teil.<name>]).
- Text: Abschnitt (titel, unterzeile), Absatz, Text (langer Text aus TOML),
  Liste (punkte, nummeriert), Hinweis (bedeutung), Reiter (titel, children).

Unterseiten: in projekt.toml [[seiten]] mit name, title, order; dazu
seiten/<name>.svelte nach demselben Muster wie diese Datei.
-->
<script lang="ts">
	import { Abschnitt, Absatz, Hinweis, Kennzahlen, Liste } from '$katalog';
	import { ladeDaten, ladeKopf } from '$projekt';

	// Der Typ folgt aus dem Namen der Datei in daten/ (ohne .toml).
	// Eigene Daten: ladeDaten<MeinTyp>('eigen-notizen'), MeinTyp in einer .ts hier im Ordner.
	const kopf = ladeKopf();
	const kennzahlen = ladeDaten('kennzahlen');

	const offen = $derived(kopf.inhalt?.kopf.open ?? []);
</script>

<Abschnitt titel="Worum es geht">
	<Absatz>
		Hier steht der Inhalt des Projekts: was entstehen soll und wie es zusammenhängt. Wie weit es
		ist, zeigen Stand und Plan oben auf der Seite.
	</Absatz>
</Abschnitt>

<Abschnitt titel="Auf einen Blick">
	<Kennzahlen daten={kennzahlen} />
</Abschnitt>

{#if offen.length > 0}
	<Abschnitt titel="Offene Punkte">
		<Liste punkte={offen} />
	</Abschnitt>
{/if}

<Hinweis bedeutung="info">
	Dies ist die Vorlage. Ein echtes Projekt ersetzt diesen Hinweis durch seine eigenen Abschnitte.
</Hinweis>
