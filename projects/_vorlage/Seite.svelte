<!--
LIESMICH: Vorlage für eine Projektseite in neuer Form

So fängst du an
- Den Ordner projects/_vorlage nach projects/<slug> kopieren (Kleinbuchstaben,
  Ziffern, Bindestriche). Ordner mit _ am Anfang lädt das Dashboard nie.
- projekt.toml anpassen: Titel, Status, Satz für die Kachel, Chat, offene Punkte.
- In daten/ nur die Dateien behalten, die die Seite braucht.
- Prüfen: dashboard.exe pruefe <slug>. Erst wenn das grün ist, kommt die
  Seite in den Bau; bis dahin zeigt das Dashboard einen einfachen Aufbau.
- Ansehen: /projects/<slug>.

Drei Sprachen, drei Aufgaben
- daten/*.toml: alles, was du von Hand einträgst (Schritte, Termine, Kosten,
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
- Daten: Kennzahlen, Ampel, Fortschritt, Zeitstrahl, Gantt, Kanban, Fluss, Fragen,
  Orte (Dateien und Ordner auf diesem PC mit dem Knopf „Öffnen“, daten/orte.toml).
  Jeder nimmt daten = das Objekt aus ladeDaten (zeigt Laden und Fehler selbst),
  dazu teil = "<name>", wenn die Datei Teile hat ([teil.<name>]).
- Text: Abschnitt (titel, unterzeile), Absatz, Text (langer Text aus TOML),
  Liste (punkte, nummeriert), Hinweis (bedeutung), Reiter (titel, children).

Unterseiten: in projekt.toml [[seiten]] mit name, title, order; dazu
seiten/<name>.svelte nach demselben Muster wie diese Datei.
-->
<script lang="ts">
	import { Abschnitt, Absatz, Fortschritt, Gantt, Hinweis, Kennzahlen, Liste } from '$katalog';
	import { ladeDaten, ladeKopf } from '$projekt';

	// Der Typ folgt aus dem Namen der Datei in daten/ (ohne .toml).
	// Eigene Daten: ladeDaten<MeinTyp>('eigen-notizen'), MeinTyp in einer .ts hier im Ordner.
	const kopf = ladeKopf();
	const kennzahlen = ladeDaten('kennzahlen');
	const fortschritt = ladeDaten('fortschritt');
	const plan = ladeDaten('gantt');

	const offen = $derived(kopf.inhalt?.kopf.open ?? []);
</script>

<Abschnitt titel="Auf einen Blick">
	<Kennzahlen daten={kennzahlen} />
</Abschnitt>

<Abschnitt titel="Wo wir stehen" unterzeile="Einschätzung von Hand aus daten/fortschritt.toml.">
	<Fortschritt daten={fortschritt} />
</Abschnitt>

<Abschnitt titel="Plan">
	<Absatz>
		Die Balken kommen aus daten/gantt.toml. Wer eine Phase abschließt, setzt dort die Farbe auf
		fertig; die Kennzahl „Phasen fertig“ stimmt dann von selbst.
	</Absatz>
	<Gantt daten={plan} />
</Abschnitt>

{#if offen.length > 0}
	<Abschnitt titel="Offene Punkte">
		<Liste punkte={offen} />
	</Abschnitt>
{/if}

<Hinweis bedeutung="info">
	Dies ist die Vorlage. Ein echtes Projekt ersetzt diesen Hinweis durch seine eigenen Abschnitte.
</Hinweis>
