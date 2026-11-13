<script lang="ts">
	/**
	 * Der Katalog (Etappe 3a-3, Zielbild „Formsprache: vier Schichten“): jeder
	 * Baustein aus $katalog mit lebendem Beispiel, einem Satz und wie man ihn
	 * benutzt; dazu die Entwürfe der Widgets (A4) mit der Wahl des
	 * Designteams. Alle Beispieldaten sind erfunden (beispiele.ts); nichts
	 * auf dieser Seite schreibt etwas oder fragt den Server (der Rahmen
	 * routes/+layout.svelte startet wie überall den Live-Kanal und lädt die
	 * Zähler der Kopfleiste). Hell und dunkel folgen der
	 * Einstellung des Systems; der Katalog läuft ab 360 px in einer Spalte.
	 */
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import {
		Abschnitt,
		Absatz,
		Ampel,
		Aufklapper,
		Auswahl,
		Backlog,
		Dialog,
		Fluss,
		Fortschritt,
		Fragen,
		Fristenliste,
		Gantt,
		Hinweis,
		Kanban,
		Kennzahlen,
		Knopf,
		Kurzinfo,
		Liste,
		Marke,
		Orte,
		Klappfeld,
		Menue,
		Minilinie,
		Reiter,
		Schublade,
		Schalter,
		StandHinweis,
		Tabelle,
		Text,
		Wahl
	} from '$katalog';
	import { resolve } from '$app/paths';
	// Etappe 3b: Textfassungen der Bild-Bausteine, mit denselben Beispielen wie im Test.
	import Textfassung from '#lib/components/Textfassung.svelte';
	import EtwasAnderes from '#lib/components/EtwasAnderes.svelte';
	import Ring from '#lib/components/pc/Ring.svelte';
	import Sparkline from '#lib/components/pc/Sparkline.svelte';
	import Kpi from '#lib/components/blocks/Kpi.svelte';
	import Progress from '#lib/components/blocks/Progress.svelte';
	import Flow from '#lib/components/blocks/Flow.svelte';
	import PcKurve from '#lib/components/daten/Verlauf.svelte';
	import { BILD_BAUSTEINE, dichteText, ganttText } from '#lib/textfassung.ts';
	import * as TB from '#lib/textfassung.beispiele.ts';
	import { tagText } from '#lib/zeit/tage.ts';
	import Verlauf from '$katalog/zeit/Verlauf.svelte';
	import Dichtestreifen from '$katalog/zeit/Dichtestreifen.svelte';
	import Zeitachse from '$katalog/zeit/Zeitachse.svelte';
	import ZeitGantt from '$katalog/zeit/Gantt.svelte';
	import Stand from '$katalog/widgets/Stand.svelte';
	import Frage from '$katalog/widgets/Frage.svelte';
	import Arbeiten from '$katalog/widgets/Arbeiten.svelte';
	import Fristen from '$katalog/widgets/Fristen.svelte';
	import Zuletzt from '$katalog/widgets/Zuletzt.svelte';
	import type { ArbeitsGruppeAnsicht } from '$katalog/widgets/typen.ts';
	import { BEDEUTUNGEN, wort } from '#lib/bedeutung.ts';
	import { meldungsZahlen } from '#lib/anstehend.ts';
	import { tagzahlAus } from '#lib/zeit/achse.ts';
	import type { AnstehendEintrag } from '$vertrag';
	import Beispiel from './Beispiel.svelte';
	import Entwurf from './entwuerfe/Entwurf.svelte';
	import StandEntwurf from './entwuerfe/StandEntwurf.svelte';
	import FrageEntwurf from './entwuerfe/FrageEntwurf.svelte';
	import ArbeitenEntwurf from './entwuerfe/ArbeitenEntwurf.svelte';
	import FristenEntwurf from './entwuerfe/FristenEntwurf.svelte';
	import ZuletztEntwurf from './entwuerfe/ZuletztEntwurf.svelte';
	import {
		ABLAGE_SPALTEN,
		ABLAGE_ZEILEN,
		AMPEL,
		FLUSS,
		FORTSCHRITT,
		KANBAN,
		KENNZAHLEN,
		KOSTEN,
		KOSTEN_ABGELEITET,
		KOSTEN_SPALTEN,
		SCHNIPSEL,
		ZEITRAUM,
		backlog,
		fragen,
		fragenAbgeleitet,
		fristenListe,
		gantt,
		ganttDaten,
		tagIso,
		orte,
		verlauf,
		widgetBeispiel
	} from './beispiele.ts';

	/** Orte im Katalog: der Knopf fragt nie den Server, er sagt nur, dass hier nichts aufgeht. */
	const katalogFetch: typeof fetch = () =>
		Promise.resolve(
			new Response(
				JSON.stringify({ fehler: 'Im Katalog wird nichts geöffnet.', art: 'ort-abgelehnt' }),
				{ status: 422, headers: { 'content-type': 'application/json' } }
			)
		);

	// Grundteile: Zustand der Beispiele (nur hier, nichts wird gespeichert).
	let nurOffene = $state(true);
	let zeitraum = $state('30');
	let klicks = $state(0);
	let laeuft = $state(false);
	let geloescht = $state(false);

	const ereignisse = verlauf();
	const plan = ganttDaten();
	const heute = tagzahlAus(new Date());
	let achsenBreite = $state(0);
	let gewaehlterTag = $state<string | null>(null);

	const w = widgetBeispiel();
	const arbeitenFuerDich = $derived(
		w.arbeiten.filter((e) => e.zustand === 'wartet' && e.wartet_auf === 'inhaber')
	);
	const gruppen: ArbeitsGruppeAnsicht[] = [
		{ wort: 'läuft', eintraege: w.arbeiten.filter((e) => e.zustand === 'laeuft'), weitere: 0 },
		{ wort: 'wartet', eintraege: w.arbeiten.filter((e) => e.zustand === 'wartet'), weitere: 0 },
		{
			wort: 'als Nächstes',
			eintraege: w.arbeiten.filter((e) => e.zustand === 'als-naechstes'),
			weitere: 0
		}
	];
	let antwortSatz = $state<string | null>(null);
	let gelesen = $state<string[]>([]);
	const aktualisierungen = $derived(
		w.aktualisierungen.map((a) => (gelesen.includes(a.id) ? { ...a, gelesen: true } : a))
	);
	let zuletztTitel = $state<HTMLElement | null>(null);

	/** Für „Text“: Leerzeilen trennen Absätze. */
	const ZWEI_ABSAETZE = 'Erster Absatz aus einer Datei.\n\nZweiter Absatz.';

	// Etappe 3b: Wahl (Optionsfelder) und die Textfassungen.
	let ansicht = $state('kacheln');
	const heuteTag = tagText(new Date());
	const dichteBeispiel = TB.dichteEreignisse(new Date());
	const eingebaut = Object.values(BILD_BAUSTEINE).filter((b) => b.ort === 'eingebaut');

	const INHALT = [
		{ id: 'grundteile', titel: 'Grundteile' },
		{ id: 'text', titel: 'Text' },
		{ id: 'daten', titel: 'Daten' },
		{ id: 'zeit', titel: 'Zeit' },
		{ id: 'als-text', titel: 'Als Text' },
		{ id: 'widgets', titel: 'Widgets' }
	];
</script>

<svelte:head>
	<title>{seitenTitel('Katalog')}</title>
</svelte:head>

<div class="katalog">
	<header class="kopf">
		<h1>Katalog</h1>
		<p>
			Alle Bausteine der Oberfläche an einem Ort: wie sie aussehen, was sie tun und wie man sie
			benutzt. Projektseiten nehmen sie aus <code>$katalog</code>; das Toolkit darunter (Bits UI)
			bindet nur der Katalog selbst ein. Farben kommen nur aus den Token, hell und dunkel folgen dem
			System oder dem Schalter „Farben“ unten auf jeder Seite.
		</p>
		<p>
			Entwürfe zur Wahl: <a href={resolve('/katalog/entwuerfe/logo')}>Logo (drei Entwürfe)</a>.
		</p>
		<nav aria-label="Inhalt des Katalogs">
			<ul class="plain inhalt">
				{#each INHALT as t (t.id)}
					<li><a href="#{t.id}">{t.titel}</a></li>
				{/each}
			</ul>
		</nav>
	</header>

	<section id="grundteile" aria-labelledby="grundteile-titel">
		<h2 id="grundteile-titel">Grundteile</h2>
		<p class="muted">
			Bedienteile mit Tastatur, Fokus und Vorlese-Programm. Das Verhalten kommt aus Bits UI, das
			Aussehen aus den Token.
		</p>
		<div class="raster">
			<Beispiel
				id="knopf"
				name="Knopf"
				herkunft="eigen"
				beschreibung="Eine Handlung. „haupt“ für die eine wichtigste, „leise“ für Nebensachen. Während etwas läuft, steht dort ein Satz und der Knopf ist gesperrt."
				code={SCHNIPSEL.knopf}
			>
				<div class="reihe">
					<Knopf
						art="haupt"
						onclick={() => {
							klicks += 1;
						}}>Zählen</Knopf
					>
					<Knopf
						laeuft={laeuft ? 'wird gespeichert' : null}
						onclick={() => {
							laeuft = true;
							setTimeout(() => {
								laeuft = false;
							}, 1500);
						}}>Speichern</Knopf
					>
					<Knopf
						art="leise"
						onclick={() => {
							klicks = 0;
						}}>Zurücksetzen</Knopf
					>
				</div>
				<p class="small" aria-live="polite">{klicks}-mal gezählt</p>
			</Beispiel>

			<Beispiel
				id="schalter"
				name="Schalter"
				herkunft="Bits UI Switch"
				beschreibung="An oder aus, sofort wirksam. Leertaste schaltet; die Beschriftung steht daneben und gehört dazu."
				code={SCHNIPSEL.schalter}
			>
				<Schalter
					bind:an={nurOffene}
					beschriftung="Nur offene zeigen"
					hinweis="Erledigte bleiben gespeichert."
				/>
				<p class="small">Jetzt: {nurOffene ? 'nur offene' : 'alle'}</p>
			</Beispiel>

			<Beispiel
				id="wahl"
				name="Wahl"
				herkunft="Bits UI RadioGroup"
				beschreibung="Eine aus wenigen Möglichkeiten, nebeneinander, sofort wirksam. Tab geht auf die gewählte, die Pfeile wechseln. So sind „Farben“ unten auf jeder Seite und „Kacheln/Spalten“ auf der Startseite gebaut."
				code={SCHNIPSEL.wahl}
			>
				<Wahl
					bind:wert={ansicht}
					beschriftung="Ansicht"
					optionen={[
						{ wert: 'kacheln', text: 'Kacheln' },
						{ wert: 'spalten', text: 'Spalten' }
					]}
				/>
				<p class="small">Gewählt: {ansicht}</p>
			</Beispiel>

			<Beispiel
				id="auswahl"
				name="Auswahl"
				herkunft="Bits UI Select"
				beschreibung="Eine aus mehreren Möglichkeiten. Pfeile wählen, Enter übernimmt, Escape schließt; die Liste bleibt im Bildschirm."
				code={SCHNIPSEL.auswahl}
			>
				<Auswahl beschriftung="Zeitraum" optionen={ZEITRAUM} bind:wert={zeitraum} />
				<p class="small">Gewählt: {zeitraum}</p>
			</Beispiel>

			<Beispiel
				id="dialog"
				name="Dialog"
				herkunft="Bits UI Dialog"
				beschreibung="Ein Fenster für eine Rückfrage. Der Fokus bleibt darin, Escape schließt, danach steht er wieder auf dem Knopf."
				code={SCHNIPSEL.dialog}
			>
				<Dialog
					titel="Eintrag löschen?"
					beschreibung="Im Katalog wird nichts gelöscht; das ist nur ein Beispiel."
					ausloeser="Löschen…"
				>
					{#snippet children(schliessen: () => void)}
						<Knopf
							art="haupt"
							onclick={() => {
								geloescht = true;
								schliessen();
							}}>Ja, löschen</Knopf
						>
					{/snippet}
				</Dialog>
				{#if geloescht}<p class="small" role="status">Gelöscht (nur zum Schein).</p>{/if}
			</Beispiel>

			<Beispiel
				id="kurzinfo"
				name="Kurzinfo"
				herkunft="Bits UI Tooltip"
				beschreibung="Ein kurzer Zusatz beim Zeigen oder beim Fokus. Am Handy gibt es kein Zeigen: Das Wichtige muss schon im Text stehen."
				code={SCHNIPSEL.kurzinfo}
			>
				<p>
					Fristen <Kurzinfo text="Zählt nur offene Einträge mit Datum" name="Mehr zu Fristen"
						>ⓘ</Kurzinfo
					>
				</p>
			</Beispiel>

			<Beispiel
				id="menue"
				name="Menü"
				herkunft="Bits UI DropdownMenu"
				beschreibung="Ein Knopf mit einer Liste von Links, etwa „Mehr“ in der Kopfzeile. Enter oder Pfeil runter öffnet, Pfeile wählen, Enter folgt dem Link, Escape schließt. Knopf und Punkte mindestens 44 px."
				code={SCHNIPSEL.menue}
			>
				<Menue
					beschriftung="Mehr"
					punkte={[
						{ text: 'Grundteile', href: '#grundteile' },
						{ text: 'Menü', href: '#menue', aktiv: true },
						{ text: 'Reiter', href: '#reiter' }
					]}
				/>
			</Beispiel>

			<Beispiel
				id="schublade"
				name="Schublade"
				herkunft="Bits UI Dialog"
				beschreibung="Ein Knopf (44 px), der von links eine Leiste über die Seite schiebt, etwa die Seitenleiste am Handy. Der Fokus bleibt darin; Escape, ein Klick daneben oder „Schließen“ schließen. Ohne Seitensperre."
				code={SCHNIPSEL.schublade}
			>
				<Schublade beschriftung="Beispiel-Schublade öffnen" titel="Beispiel">
					<Liste punkte={['Anstehend', 'Projekte', 'Daten']} />
				</Schublade>
			</Beispiel>

			<Beispiel
				id="klappfeld"
				name="Klappfeld"
				herkunft="Bits UI Popover"
				beschreibung="Ein Knopf, der ein kleines Feld mit Inhalt aufklappt, etwa die Meldungen an der Glocke. Escape oder ein Klick daneben schließen; die Seite bleibt bedienbar (keine Sperre)."
				code={SCHNIPSEL.klappfeld}
			>
				<Klappfeld name="Beispiel-Klappfeld öffnen" titel="Kurz erklärt">
					{#snippet knopf()}ⓘ{/snippet}
					{#snippet children(schliessen)}
						<p class="small">Das Feld schließt mit Escape; der Fokus geht zurück auf den Knopf.</p>
						<button type="button" class="button" onclick={schliessen}>Schließen</button>
					{/snippet}
				</Klappfeld>
			</Beispiel>

			<Beispiel
				id="minilinie"
				name="Minilinie"
				herkunft="eigen"
				beschreibung="Ein kleiner Verlauf als Linie (0 bis 100), etwa die Prozessorlast der letzten Minute oben rechts. Farbe aus der Bedeutung, nur Token; bei reduzierter Bewegung ohne Animation."
				code={SCHNIPSEL.minilinie}
			>
				<p class="minilinien">
					<Minilinie werte={[12, 18, 15, 30, 26, 22, 35, 28, 24, 20, 19, 23]} bedeutung="ok" />
					<Minilinie werte={[60, 72, 81, 77, 85, 79]} bedeutung="wartet" />
					<Minilinie
						werte={[91, 95, 97]}
						bedeutung="problem"
						beschriftung="Prozessor zuletzt 97 %"
					/>
				</p>
			</Beispiel>

			<Beispiel
				id="reiter"
				name="Reiter"
				herkunft="Bits UI Tabs"
				beschreibung="Mehrere Ansichten an einer Stelle. Pfeile links und rechts wechseln, Pos1 und Ende springen."
				code={SCHNIPSEL.reiter}
			>
				<Reiter titel={['Plan', 'Stand']}>
					{#snippet children(gewaehlt)}
						{#if gewaehlt === 0}<Gantt daten={gantt()} />{:else}<Fortschritt
								daten={FORTSCHRITT}
							/>{/if}
					{/snippet}
				</Reiter>
			</Beispiel>

			<Beispiel
				id="aufklapper"
				name="Aufklapper"
				herkunft="Bits UI Collapsible"
				beschreibung="Ein Abschnitt zum Auf- und Zuklappen. Für Einträge mit Anker (#frage-…) bleibt <details>, weil der Sprung es aufklappt."
				code={SCHNIPSEL.aufklapper}
			>
				<Aufklapper titel="Ältere Einträge">
					<Liste punkte={['Token zusammengelegt', 'Zeitachse fertig', 'Plan bestätigt']} />
				</Aufklapper>
			</Beispiel>

			<Beispiel
				id="tabelle"
				name="Tabelle"
				herkunft="eigen"
				beschreibung="Breite Tabellen rollen in ihrem eigenen Rahmen, nie die Seite. Schatten am Rand zeigen, dass dort mehr kommt; rollt der Rahmen, ist er mit Tab erreichbar."
				code={SCHNIPSEL.tabelle}
			>
				<Tabelle
					beschriftung="Wo was liegt"
					beschriftungZeigen
					mindestbreite={36}
					spalten={ABLAGE_SPALTEN}
					zeilen={ABLAGE_ZEILEN}
				/>
			</Beispiel>
		</div>
	</section>

	<section id="text" aria-labelledby="text-titel">
		<h2 id="text-titel">Text</h2>
		<div class="raster">
			<Beispiel
				id="abschnitt"
				name="Abschnitt"
				beschreibung="Eine Karte mit Überschrift und einem Satz darunter; gliedert die Seite."
				code={SCHNIPSEL.abschnitt}
			>
				<Abschnitt titel="Zeitplan" unterzeile="Stand: heute">
					<Absatz>Inhalt des Abschnitts.</Absatz>
				</Abschnitt>
			</Beispiel>
			<Beispiel
				id="absatz"
				name="Absatz und Text"
				beschreibung="Absatz für eigenen Fließtext; Text macht aus Leerzeilen Absätze."
				code={`${SCHNIPSEL.absatz}\n${SCHNIPSEL.text}`}
			>
				<Absatz>Ein Absatz Fließtext mit ruhigem Zeilenabstand.</Absatz>
				<Text text={ZWEI_ABSAETZE} />
			</Beispiel>
			<Beispiel
				id="liste"
				name="Liste"
				beschreibung="Punkte oder nummeriert."
				code={SCHNIPSEL.liste}
			>
				<Liste punkte={['Erstens', 'Zweitens']} nummeriert />
			</Beispiel>
			<Beispiel
				id="hinweis"
				name="Hinweis"
				beschreibung="Ein hervorgehobener Satz; die Farbe folgt der Bedeutung, das Wort steht immer dabei."
				code={SCHNIPSEL.hinweis}
			>
				{#each BEDEUTUNGEN as b (b)}
					<Hinweis bedeutung={b}>Bedeutung „{wort(b)}“.</Hinweis>
				{/each}
			</Beispiel>
			<Beispiel
				id="marke"
				name="Marke"
				herkunft="Etappe B1"
				beschreibung="Ein Stand als kurzes Wort mit Bedeutung, für Zellen und Listen. Das Wort steht immer da, die Farbe nie allein."
				code={SCHNIPSEL.marke}
			>
				<p class="marken">
					<Marke bedeutung="ok">bezahlt</Marke>
					<Marke bedeutung="laeuft">in Druck</Marke>
					<Marke bedeutung="wartet" wort="offen" />
					<Marke bedeutung="problem">fehlt</Marke>
					<Marke bedeutung="info">neu</Marke>
					<Marke bedeutung="neutral">geplant</Marke>
				</p>
			</Beispiel>
			<Beispiel
				id="stand-hinweis"
				name="Stand-Hinweis"
				herkunft="Etappe B1"
				beschreibung="„Stand vom …“ an einer Momentaufnahme. Wie alt der Stand ist, rechnet der Server; ab dem Alter aus alt_ab (ohne Angabe 7 Tage) wird er gelb und sagt es in Worten."
				code={SCHNIPSEL.standHinweis}
			>
				<p><StandHinweis stand={tagIso(-2)} alter={{ tage: 2, stufe: 'frisch' }} /></p>
				<p><StandHinweis stand={tagIso(-12)} alter={{ tage: 12, stufe: 'alt' }} /></p>
			</Beispiel>
			<Beispiel
				id="bild"
				name="Bild"
				beschreibung="Ein Bild aus assets/ des eigenen Projekts, mit Text für Vorlese-Programme; vergrößert sich beim Antippen. Braucht eine Projektseite, darum hier ohne Beispiel."
				code={SCHNIPSEL.bild}
			>
				<p class="muted small">Nur auf Projektseiten (das Bild liegt im Projektordner).</p>
			</Beispiel>
		</div>
	</section>

	<section id="daten" aria-labelledby="daten-titel">
		<h2 id="daten-titel">Daten</h2>
		<p class="muted">
			Bausteine für Dateien aus <code>daten/</code>. Sie nehmen den Inhalt oder gleich das Objekt
			aus <code>ladeDaten</code> und zeigen dann Laden und Fehler selbst.
		</p>
		<div class="raster">
			<Beispiel
				id="kennzahlen"
				name="Kennzahlen"
				beschreibung="Wenige wichtige Zahlen mit Einheit und Hinweis. Berechnet über eine leere Liste ergibt 0; anzahl und anteil zählen auch nach beliebigem Feld und Wert; mit stand darunter „Stand vom …“."
				code={SCHNIPSEL.kennzahlen}
			>
				<Kennzahlen daten={KENNZAHLEN} />
			</Beispiel>
			<Beispiel
				id="ampel"
				name="Ampel"
				beschreibung="Je Zeile ein Zustand mit Farbe und Wort; mit stand dahinter, wann zuletzt geprüft."
				code={SCHNIPSEL.ampel}
			>
				<Ampel daten={AMPEL} />
			</Beispiel>
			<Beispiel
				id="fortschritt"
				name="Fortschritt"
				beschreibung="Balken in Prozent oder als erreicht von gesamt, auch mit Kommazahl und Einheit („0,2 von 1,5 GB“)."
				code={SCHNIPSEL.fortschritt}
			>
				<Fortschritt daten={FORTSCHRITT} />
			</Beispiel>
			<Beispiel
				id="tabelle-daten"
				name="Tabelle aus Daten"
				herkunft="Etappe B1"
				beschreibung="Die Tabelle nimmt eine Liste aus daten/ (etwa eigen-kosten.toml). Summenzeile vom Server, ein Link je Zeile, Statuswörter als Marke."
				code={SCHNIPSEL.tabelleDaten}
			>
				<Tabelle
					beschriftung="Kosten im Oktober"
					beschriftungZeigen
					daten={KOSTEN}
					liste="posten"
					abgeleitet={KOSTEN_ABGELEITET}
					spalten={KOSTEN_SPALTEN}
				/>
			</Beispiel>
			<Beispiel
				id="kanban"
				name="Kanban"
				beschreibung="Spalten mit Karten aus daten/kanban.toml; am Handy untereinander. Läuft aus (Etappe B2): Die Spalten aus dem Datenspeicher zeigt das Backlog."
				code={SCHNIPSEL.kanban}
			>
				<Kanban daten={KANBAN} />
			</Beispiel>
			<Beispiel
				id="backlog"
				name="Backlog"
				beschreibung="Die Aufgabenliste der Projektseite: Aufgaben, Arbeiten und die eine sichtbare Frage in Gruppen untereinander (Wartet auf dich, Läuft, Als Nächstes, Wartet auf andere), je Arbeit eine Zeile mit Etappe, wer dran ist und Frist, aufklappbar zu den Schritten; alles vom Server aus dem Datenspeicher. Steht von selbst auf jeder Projektseite, dort mit Frage und Aufgaben zum Beantworten."
				code={SCHNIPSEL.backlog}
			>
				<Backlog backlog={backlog()} />
			</Beispiel>
			<Beispiel
				id="fristenliste"
				name="Fristenliste"
				beschreibung="Etappe B3: alle offenen Fristen eines Projekts nach Datum, die überfälligen zuerst, aus daten/fristen.toml, den Aufgaben, der sichtbaren Frage und den Arbeiten; bewegliche Fristen (Ereignis plus Abstand) mit ihrem Datum, sobald das Ereignis eingetreten ist, sonst am Ende mit „hängt an …“. Nie aus dem Zeitstrahl. Nur mit Anmeldung."
				code={SCHNIPSEL.fristenliste}
			>
				<Fristenliste liste={fristenListe()} />
			</Beispiel>
			<Beispiel
				id="fluss"
				name="Fluss"
				beschreibung="Kästen mit Pfeilen für einen Ablauf."
				code={SCHNIPSEL.fluss}
			>
				<Fluss daten={FLUSS} />
			</Beispiel>
			<Beispiel
				id="fragen"
				name="Fragen"
				beschreibung="Die offene Frage aus daten/fragen.toml: je Projekt eine, die übrigen warten ohne Text (Etappe B2). Beantworten geht im Widget „Frage“."
				code={SCHNIPSEL.fragen}
			>
				<Fragen daten={fragen()} abgeleitet={fragenAbgeleitet()} />
			</Beispiel>
			<Beispiel
				id="orte"
				name="Orte"
				beschreibung="Dateien und Ordner auf diesem PC aus daten/orte.toml, je mit dem Knopf „Öffnen“ (der Pfad steht nie auf der Seite). Im Katalog öffnet der Knopf nichts."
				code={SCHNIPSEL.orte}
			>
				<Orte daten={orte()} projekt="beispiel" fetchFn={katalogFetch} />
			</Beispiel>
			<Beispiel
				id="gantt"
				name="Gantt"
				beschreibung="Balkenplan aus daten/gantt.toml auf der gemeinsamen Zeitachse."
				code={SCHNIPSEL.gantt}
			>
				<Gantt daten={gantt()} />
			</Beispiel>
			<Beispiel
				id="zeitstrahl"
				name="Zeitstrahl"
				beschreibung="Einträge aus daten/zeitstrahl.toml als Verlauf; die Ereignisse rechnet der Server, darum hier der Verlauf unten im Abschnitt Zeit."
				code={SCHNIPSEL.zeitstrahl}
			>
				<p class="muted small">Siehe „Verlauf“ unter Zeit.</p>
			</Beispiel>
		</div>
	</section>

	<section id="zeit" aria-labelledby="zeit-titel">
		<h2 id="zeit-titel">Zeit</h2>
		<p class="muted">
			Die gemeinsame Zeitachse aus Etappe 3a-1: Verlauf, Dichtestreifen, Zeitachse und Balkenplan.
		</p>
		<div class="raster">
			<Beispiel
				id="verlauf"
				name="Verlauf"
				beschreibung="Ereignisse nach Tagen; Heute und Gestern offen, Routine in einer Zeile, Kommendes oben."
				code={SCHNIPSEL.verlauf}
			>
				<Verlauf {ereignisse} modus="projekt" ebene={4} />
			</Beispiel>
			<Beispiel
				id="dichtestreifen"
				name="Dichtestreifen"
				beschreibung="Wie viel an welchem Tag passiert ist; ein Tag lässt sich wählen (Pfeile, Enter)."
				code={SCHNIPSEL.dichtestreifen}
			>
				<Dichtestreifen
					{ereignisse}
					onwaehle={(tag: string) => {
						gewaehlterTag = tag;
					}}
				/>
				<p class="small" aria-live="polite">
					{gewaehlterTag ? `Gewählt: ${gewaehlterTag}` : 'Noch kein Tag gewählt.'}
				</p>
				<Textfassung fassung={dichteText(ereignisse, heuteTag)} />
			</Beispiel>
			<Beispiel
				id="zeitachse"
				name="Zeitachse"
				beschreibung="Die Beschriftung der Zeit, Grundlage von Balkenplan und Dichtestreifen; die Stufe passt sich der Breite an."
				code={SCHNIPSEL.zeitachse}
			>
				<div class="achse" bind:clientWidth={achsenBreite}>
					{#if achsenBreite > 0}
						<Zeitachse
							fenster={{ von: heute - 30, bis: heute + 10 }}
							breite={achsenBreite}
							{heute}
						/>
					{/if}
				</div>
			</Beispiel>
			<Beispiel
				id="zeit-gantt"
				name="Balkenplan (Zeit)"
				beschreibung="Zoom mit + und -, Pfeile verschieben, Pos1 springt auf heute."
				code={SCHNIPSEL.zeitgantt}
			>
				{#if 'fehler' in plan}
					<p class="block-error">{plan.fehler}</p>
				{:else}
					<ZeitGantt data={plan} />
					<Textfassung fassung={ganttText(plan)} />
				{/if}
			</Beispiel>
		</div>
	</section>

	<section id="als-text" aria-labelledby="als-text-titel">
		<h2 id="als-text-titel">Als Text</h2>
		<p class="muted">
			Jeder Baustein, der ein Bild zeigt (Balken, Ring, Kurve, Streifen, Pfeile), hat eine
			Textfassung mit denselben Zahlen: eine Tabelle, die Vorlese-Programme immer lesen und die der
			Knopf „Als Text“ sichtbar macht. Hier steht sie offen. Ein Test hält Bild und Text gleich.
		</p>
		<div class="raster">
			<Beispiel
				id="text-gantt"
				name="Balkenplan als Text"
				beschreibung="Je Balken Name, von, bis, Dauer und Stand. Wer Gantt einbindet, setzt die Textfassung darunter."
				code={SCHNIPSEL.textGantt}
			>
				<ZeitGantt data={TB.GANTT} />
				<Textfassung fassung={ganttText(TB.GANTT)} offen />
			</Beispiel>
			<Beispiel
				id="text-dichte"
				name="Dichtestreifen als Text"
				beschreibung="Je Tag mit Einträgen die Zahl und wie viele davon wichtig sind, neuester Tag zuerst."
				code={SCHNIPSEL.textDichte}
			>
				<Dichtestreifen ereignisse={dichteBeispiel} />
				<Textfassung fassung={dichteText(dichteBeispiel, heuteTag)} offen />
			</Beispiel>
			<Beispiel
				id="text-ring"
				name="Ring als Text"
				beschreibung="Anteil, Wert und Stufe. Ohne Knopf, weil Wert und Name schon als Text unter dem Ring stehen."
				code={SCHNIPSEL.textRing}
			>
				<Ring {...TB.RING} textOffen />
			</Beispiel>
			<Beispiel
				id="text-kurve"
				name="Kurve als Text"
				beschreibung="Jetzt, Spitze, Tiefst, Mittel und wie viele Messpunkte die Linie hat."
				code={SCHNIPSEL.textKurve}
			>
				<Sparkline {...TB.KURVE} textOffen />
			</Beispiel>
			<Beispiel
				id="text-fortschritt"
				name="Fortschritt als Text"
				beschreibung="Je Balken Prozent, Stand und Hinweis."
				code={SCHNIPSEL.textFortschritt}
			>
				<Progress rows={TB.FORTSCHRITT} textOffen />
			</Beispiel>
			<Beispiel
				id="text-kennzahlen"
				name="Kennzahlen als Text"
				beschreibung="Die Endwerte, auch während die Kacheln noch hochzählen."
				code={SCHNIPSEL.textKennzahlen}
			>
				<Kpi items={TB.KENNZAHLEN} textOffen />
			</Beispiel>
			<Beispiel
				id="text-fluss"
				name="Fluss als Text"
				beschreibung="Je Kasten, wohin seine Pfeile führen."
				code={SCHNIPSEL.textFluss}
			>
				<Flow data={TB.FLUSS} textOffen />
			</Beispiel>
			<Beispiel
				id="text-pc-kurve"
				name="PC-Kurve als Text"
				beschreibung="Prozessor und Arbeitsspeicher: Mittel, Spitze, Tiefst und Zahl der Messwerte. Eine Lücke zählt nicht als 0."
				code={SCHNIPSEL.textPcKurve}
			>
				<PcKurve werte={TB.PC_KURVE} textOffen />
			</Beispiel>
			<Beispiel
				id="text-eingebaut"
				name="Schon Text"
				beschreibung="Diese Bausteine sind selbst Listen oder Tabellen mit allen Zahlen; sie brauchen keine zweite Fassung."
				code={SCHNIPSEL.textEingebaut}
			>
				<ul class="small">
					{#each eingebaut as b (b.name)}
						<li><b>{b.name}</b>: {b.grund}</li>
					{/each}
				</ul>
			</Beispiel>
		</div>
	</section>

	<section id="widgets" aria-labelledby="widgets-titel">
		<h2 id="widgets-titel">Widgets</h2>
		<p class="muted">
			Die Bausteine des Unter-Dashboards (Konzept „Anstehendes“, A4). Das Designteam zeigt je Widget
			drei Entwürfe und wählt einen; der gewählte steht oben auf jeder Projektseite. Zahlen, Fristen
			und Zustände kommen vom Server, Farbe, Satz und Reihenfolge entstehen hier. Regeln: nur Token,
			relative Maße, ruhig, Tastatur und Vorlese-Programm, am Handy eine Spalte.
		</p>

		{#snippet attrappe(e: AnstehendEintrag)}
			<div class="attrappe" role="group" aria-label="Antwort (Beispiel, sendet nichts)">
				{#each ['Ja', 'Nein'] as a (a)}
					<button
						type="button"
						class="button"
						onclick={() => {
							antwortSatz = `„${a}“ auf „${e.titel}“ (im Katalog nicht gesendet).`;
						}}>{a}</button
					>
				{/each}
				<!-- Entscheidung: „Etwas anderes …“ gehört zu jeder Klick-Antwort. -->
				<EtwasAnderes
					entwurf="katalog:{e.id}"
					bezug={e.titel}
					senden={(eigene: string) => {
						antwortSatz = `Eigene Worte auf „${e.titel}“ (im Katalog nicht gesendet): ${eigene}`;
						return Promise.resolve(null);
					}}
				/>
			</div>
		{/snippet}

		<Beispiel
			id="widget-stand"
			name="Stand"
			beschreibung="Gesamtstatus des Projekts: Ampel mit Satz, dazu wie viel wartet, läuft und als Nächstes kommt."
			code={SCHNIPSEL.stand}
		>
			<div class="entwuerfe">
				<Entwurf
					name="Kachel mit Ampel"
					gewaehlt
					satz="Gewählt: Ampel als Rand und Punkt mit Satz, die Zahlen als ruhige Marken. Ein Blick zeigt Lage und Menge; die Farbe steht nie allein."
				>
					<Stand gesamt={w.stand.gesamt} schwerpunkt={w.stand.schwerpunkt} kurz={w.stand.kurz} />
				</Entwurf>
				<Entwurf
					name="Kompakte Karte"
					satz="Bisher: Ampel und Satz. Wie viel ansteht, sagt sie nicht."
				>
					<StandEntwurf form="kompakt" stand={w.stand} />
				</Entwurf>
				<Entwurf
					name="Eine Zeile"
					satz="Alles in einer Zeile ohne Karte. Am Handy bricht sie in drei Zeilen um und wird unruhig."
				>
					<StandEntwurf form="zeile" stand={w.stand} />
				</Entwurf>
			</div>
		</Beispiel>

		<Beispiel
			id="widget-frage"
			name="Frage"
			beschreibung="Was auf dich wartet: die eine aktuelle Frage zum Beantworten, weitere zugeklappt, dann Aufgaben und Arbeiten."
			code={SCHNIPSEL.frage}
		>
			<div class="entwuerfe">
				<Entwurf
					name="Liste mit Gruppen"
					gewaehlt
					satz="Gewählt: eine Frage offen, die anderen unter „noch N warten“, darunter Aufgaben und Arbeiten. Eine Karte für alles, was du tun kannst; jeder Anker findet seinen Eintrag."
				>
					<Frage
						zahl={w.stand.kurz.wartet_auf_dich}
						fragen={w.fragen}
						aufgaben={w.aufgaben}
						arbeiten={arbeitenFuerDich}
						antwort={attrappe}
					/>
				</Entwurf>
				<Entwurf
					name="Karte mit der einen Frage"
					satz="Nur die Frage, groß. Aufgaben und Arbeiten bräuchten eine zweite Karte."
				>
					<FrageEntwurf form="einzeln" fragen={w.fragen} zahl={w.stand.kurz.wartet_auf_dich} />
				</Entwurf>
				<Entwurf
					name="Kachel mit Zähler"
					satz="Große Zahl, erster Titel. Schnell gelesen, aber beantworten geht nicht."
				>
					<FrageEntwurf form="zaehler" fragen={w.fragen} zahl={w.stand.kurz.wartet_auf_dich} />
				</Entwurf>
			</div>
			{#if antwortSatz}<p class="small" role="status">{antwortSatz}</p>{/if}
		</Beispiel>

		<Beispiel
			id="widget-arbeiten"
			name="Arbeiten"
			beschreibung="Woran die Chats arbeiten: läuft, wartet, als Nächstes, je höchstens zwei Zeilen."
			code={SCHNIPSEL.arbeiten}
		>
			<div class="entwuerfe">
				<Entwurf
					name="Liste mit Gruppen"
					gewaehlt
					satz="Gewählt: Gruppen untereinander mit Zahl; jeder Eintrag klappt auf und hat seinen Anker. Liest sich am Handy von oben nach unten."
				>
					<Arbeiten {gruppen} />
				</Entwurf>
				<Entwurf
					name="Drei Spalten"
					satz="Kleines Kanban. Bei 360 px werden die Spalten so schmal, dass Titel zerfallen."
				>
					<ArbeitenEntwurf form="spalten" {gruppen} />
				</Entwurf>
				<Entwurf
					name="Zähler-Kachel"
					satz="Nur Zahlen. Ruhig, aber man sieht nicht, woran gearbeitet wird."
				>
					<ArbeitenEntwurf form="zaehler" {gruppen} />
				</Entwurf>
			</div>
		</Beispiel>

		<Beispiel
			id="widget-fristen"
			name="Fristen"
			beschreibung="Die nächsten Fristen mit „in N Tagen“; heute gelb, vorbei rot."
			code={SCHNIPSEL.fristen}
		>
			<div class="entwuerfe">
				<Entwurf
					name="Liste der Fristen"
					gewaehlt
					satz="Gewählt: die nächsten drei mit Datum, Abstand und Sache. Man sieht, was danach kommt."
				>
					<Fristen
						naechste={w.stand.kurz.naechste_frist ?? ''}
						tageBis={w.stand.kurz.tage_bis}
						fristStufe={w.stand.kurz.frist_stufe}
						eintraege={[...w.aufgaben, ...w.arbeiten]}
					/>
				</Entwurf>
				<Entwurf
					name="Nächste Frist groß"
					satz="Bisher: ein Datum. Die zweite Frist zwei Tage später bleibt unsichtbar."
				>
					<FristenEntwurf form="gross" eintraege={[...w.aufgaben, ...w.arbeiten]} />
				</Entwurf>
				<Entwurf
					name="Kalenderblatt"
					satz="Hübsch, aber braucht Platz und sagt wenig über die Sache."
				>
					<FristenEntwurf form="kalender" eintraege={[...w.aufgaben, ...w.arbeiten]} />
				</Entwurf>
			</div>
		</Beispiel>

		<Beispiel
			id="widget-zuletzt"
			name="Zuletzt"
			beschreibung="Was zuletzt passiert ist: Meldungen und Vorgänge, ungelesene hervorgehoben, mit „gelesen“."
			code={SCHNIPSEL.zuletzt}
		>
			<div class="entwuerfe">
				<Entwurf
					name="Liste mit Neu-Marken"
					gewaehlt
					satz="Gewählt: Sache vorn, Art und Zeit leise dahinter, „neu“ und „gelesen“ dort, wo man liest."
				>
					<Zuletzt
						{aktualisierungen}
						ungelesen={meldungsZahlen(aktualisierungen).neu}
						angemeldet
						ongelesen={(id: string) => {
							gelesen = [...gelesen, id];
							zuletztTitel?.focus();
						}}
						bind:titel={zuletztTitel}
					/>
				</Entwurf>
				<Entwurf
					name="Zeitleiste"
					satz="Uhrzeit links, Sache rechts. Die Zeit-Spalte nimmt am Handy ein Drittel der Breite."
				>
					<ZuletztEntwurf form="zeitleiste" {aktualisierungen} />
				</Entwurf>
				<Entwurf name="Zähler-Kachel" satz="Nur „N neu“. Zeigt nicht, was passiert ist.">
					<ZuletztEntwurf form="zaehler" {aktualisierungen} />
				</Entwurf>
			</div>
		</Beispiel>
	</section>
</div>

<style>
	.katalog {
		display: grid;
		gap: 1.5rem;
		min-width: 0;
	}
	.kopf p {
		max-width: 45rem;
	}
	.inhalt {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem 1rem;
	}
	section {
		display: grid;
		gap: 0.75rem;
		min-width: 0;
		scroll-margin-top: 5rem;
	}
	section > h2 {
		margin: 0;
		font-size: var(--text-3xl);
	}
	section > p {
		margin: 0;
		max-width: 45rem;
	}
	.raster {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 22rem), 1fr));
		gap: 0.75rem;
		min-width: 0;
	}
	.marken {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.minilinien {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin: 0;
	}
	.reihe {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.entwuerfe {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 17rem), 1fr));
		gap: 1.25rem 1rem;
		min-width: 0;
	}
	.achse {
		min-width: 0;
	}
	.attrappe {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin: 0.3rem 0;
	}
</style>
