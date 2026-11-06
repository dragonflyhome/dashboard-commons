<script lang="ts">
	/**
	 * Das Unter-Dashboard (Etappe A1, Konzept [unter_dashboard]): eine Reihe
	 * kleiner Karten oben auf jeder Projektseite, alte und neue Form. Es sitzt
	 * im Rahmen der Projektseite, nicht in Seite.svelte; die Projekt-Chats
	 * pflegen es nicht selbst.
	 *
	 * Karten: Stand (Ampel mit Satz), Wartet auf dich (Aufgaben als Link auf
	 * ihren Anker im Kasten „Aufgaben für dich“, Fragen und Arbeiten hier
	 * selbst zum Aufklappen; die eine Frage steht offen und lässt sich direkt
	 * beantworten, Etappe A2), Arbeiten (läuft, wartet, als Nächstes), Nächste
	 * Frist, Notizen. Leere Karten fehlen; ohne Stand fehlt die ganze Reihe.
	 * Fehlt der Datenspeicher, kommt der Stand trotzdem (Notizen und Fragen
	 * aus den Dateien), mit einem Satz in der Karte „Stand“.
	 *
	 * Ziele: Kommt man über `#<art>-<id>`, klappt der Eintrag auf, ist
	 * hervorgehoben und hat den Fokus, sobald die Daten da sind; fehlt er,
	 * steht ein kurzer Satz da.
	 *
	 * Etappe A3: Karte „Aktualisierungen“ mit den letzten Meldungen und was
	 * zuletzt passiert ist (höchstens fünf, ungelesene hervorgehoben, Meldungen
	 * mit Anker `#meldung-<id>` zum Aufklappen und „gelesen“). Ungelesene
	 * Meldungen, die nicht unter den fünf sind, stehen aufklappbar darunter,
	 * damit jeder Link aus dem Band hier ein Ziel hat. Ohne Anmeldung stehen
	 * in der Karte nur die Zahlen (neu, gesamt), keine Titel und Texte.
	 *
	 * Etappe 3a-3 (A4): Die Karten sind die Widgets aus dem Katalog
	 * (web/src/lib/katalog/widgets: Stand, Frage, Arbeiten, Fristen, Zuletzt;
	 * Entwürfe und Wahl im /katalog). Hier bleiben Laden, Live-Kanal,
	 * Mitternacht, Anker und Fokus; die Widgets zeigen nur.
	 */
	import { page } from '$app/state';
	import { tick } from 'svelte';
	import AntwortStatus from '#lib/components/anstehend/AntwortStatus.svelte';
	import FrageAntwort from '#lib/components/anstehend/FrageAntwort.svelte';
	import { antworten } from '#lib/antworten.svelte.ts';
	import { frageSchluessel } from '#lib/antworten.ts';
	import { live } from '#lib/live.svelte.ts';
	import { aufgaben } from '#lib/aufgaben.svelte.ts';
	import {
		SATZ_NICHT_MEHR_DA,
		SATZ_NOTIZEN_OHNE_ANMELDUNG,
		SATZ_OHNE_ANMELDUNG,
		meldungGelesen
	} from '#lib/anstehend.ts';
	import { ladeUnterDaten, teilen, type UnterDaten } from '#lib/unterdashboard.ts';
	import { bauLage } from '#lib/bau.svelte.ts';
	import { ApiError } from '#lib/api/client.ts';
	import Stand from '$katalog/widgets/Stand.svelte';
	import Frage from '$katalog/widgets/Frage.svelte';
	import Arbeiten from '$katalog/widgets/Arbeiten.svelte';
	import Fristen from '$katalog/widgets/Fristen.svelte';
	import Zuletzt from '$katalog/widgets/Zuletzt.svelte';
	import { beiTageswechsel } from '#lib/tageswechsel.ts';
	import { ankerAus, ansteuern, ankerZumEintrag, gleicherAnkerKlick } from '#lib/ziel.ts';
	import type { AnstehendEintrag } from '$vertrag';

	interface Props {
		slug: string;
		/** Für Tests; sonst das globale fetch. */
		fetchFn?: typeof fetch;
	}
	let { slug, fetchFn = (...a) => fetch(...a) }: Props = $props();

	let daten = $state<UnterDaten | null>(null);
	let geladenFuer = $state<string | null>(null);

	/** Laufende Nummer der Anfragen: Eine späte Antwort überschreibt nie eine neuere (Auflage aus A1). */
	let anfrage = 0;

	/** Lädt neu; ältere Antworten (anderes Projekt oder überholt) zählen nicht mehr. */
	async function laden(fuer: string): Promise<void> {
		const nr = ++anfrage;
		const d = await ladeUnterDaten(fuer, fetchFn);
		if (fuer !== slug || nr !== anfrage) return;
		daten = d;
		geladenFuer = fuer;
	}

	$effect(() => {
		const s = slug;
		// Ein anderes Projekt: die Hülle von eben nicht weiter festhalten.
		behalten = false;
		void laden(s);
		const offChange = live.onChange((c) => {
			const betrifft =
				(c.area === 'anstehend' && (c.path === 'anstehend' || c.path === `anstehend/${s}`)) ||
				(c.area === 'aufgaben' && c.path === `aufgaben/${s}`);
			if (betrifft) void laden(s);
		});
		const offNachholen = live.onNachholen(() => {
			void laden(s);
		});
		// Live-Lücken: Die Karte „Zuletzt“ zeigt die Vorgänge des Projekts; auch reine
		// Vorgänge (anlegen, weiter, Bau, Passkey) ohne change anstehend laden neu.
		const offVorgang = live.onVorgang((v) => {
			if (v.slug === s) void laden(s);
		});
		// Um Mitternacht neu rechnen lassen: Frist morgen wird heute (Bald wird Jetzt).
		const offTag = beiTageswechsel(() => {
			void laden(s);
		});
		return () => {
			offChange();
			offNachholen();
			offVorgang();
			offTag();
		};
	});

	// Etappe 3a+ (Abnahme, Nachbesserung 1): Auswahl, Zahlen und Gruppen kommen
	// fertig aus anstehend.ts (`unterDashboardAnsicht`); hier wird nur angezeigt.
	const stand = $derived(daten?.stand ?? null);
	// Der Kopf (BauKopf) sagt bei roter Karte nicht „Seite in Ordnung“.
	$effect(() => {
		if (geladenFuer === slug) bauLage.standMelden(slug, stand?.gesamt.ampel === 'rot');
	});
	const angemeldet = $derived(daten?.angemeldet ?? false);
	const aktualisierungen = $derived(daten?.aktualisierungen ?? []);
	const meldungsZahlen = $derived(daten?.meldungsZahlen ?? null);
	const ungelesenZahl = $derived(daten?.ungelesen ?? 0);
	const weitereMeldungen = $derived(daten?.weitereMeldungen ?? []);

	let gelesenLaeuft = $state<string | null>(null);
	let gelesenFehler = $state<string | null>(null);
	let aktTitel = $state<HTMLElement | null>(null);
	/** „gelesen“ hier: danach neu laden; den Fokus nur nehmen, wenn er noch auf dem Knopf oder body liegt. */
	async function gelesen(id: string, knopf: HTMLButtonElement): Promise<void> {
		gelesenLaeuft = id;
		gelesenFehler = null;
		try {
			await meldungGelesen(id, fetchFn);
		} catch (err) {
			gelesenFehler =
				err instanceof ApiError ? err.message : 'Die Meldung ließ sich nicht als gelesen ablegen.';
		}
		try {
			await laden(slug);
		} finally {
			gelesenLaeuft = null;
		}
		await tick();
		const aktiv = document.activeElement;
		if (aktiv === null || aktiv === document.body || aktiv === knopf) aktTitel?.focus();
	}

	const wartet = $derived(daten?.wartet ?? null);
	const arbeitsGruppen = $derived(daten?.arbeitsGruppen ?? []);
	const fristKandidaten = $derived(daten?.fristKandidaten ?? []);
	const notizen = $derived(teilen(stand?.notizen ?? [], 3));

	const hatFrist = $derived(stand?.kurz.naechste_frist != null);
	// Etappe B2: ohne Anmeldung nur die Zahl der Notizen (open[] ist persönlicher Text).
	const notizenZahl = $derived(stand?.notizen_zahl ?? stand?.notizen.length ?? 0);
	const hatNotizen = $derived(notizenZahl > 0);
	// Der Server liefert den Stand auch ohne Datenspeicher (Notizen, Fragen);
	// ohne Stand reicht, was die Liste hat (Fragen, Aufgaben, Arbeiten, Meldungen).
	// Notizen kommen nur mit dem Stand und zählen darum nicht extra (Auflage aus A1).
	const zeigen = $derived(daten?.zeigen ?? false);

	/**
	 * Fokusziel nach einer Antwort: die Überschrift der Reihe (nur für
	 * Vorlese-Programme zu sehen, tabindex -1), nicht die Meldung selbst und
	 * nicht der ganze Abschnitt: Manche Vorlese-Programme läsen sonst den
	 * ganzen Inhalt samt Meldung vor (Auflage aus A2). Wer hier geantwortet
	 * hat, behält die Reihe, bis die Seite wechselt, auch wenn danach nichts
	 * mehr zu zeigen ist; so fällt der Fokus nicht auf body.
	 */
	let huelle = $state<HTMLElement | null>(null);
	let behalten = $state(false);
	function nachAntwort(): Promise<void> {
		behalten = true;
		return laden(slug);
	}

	// Ziel ansteuern, sobald die Daten da sind (einmal je Anker).
	const hash = $derived(ankerAus(page.url.hash));
	let angesteuert = '';
	let ankerSatz = $state<string | null>(null);
	$effect(() => {
		const h = hash;
		const ziel = ankerZumEintrag(h);
		if (ziel === null) {
			ankerSatz = null;
			angesteuert = '';
			return;
		}
		const bereit = geladenFuer === slug && (ziel.art !== 'aufgabe' || aufgaben.stand !== 'laedt');
		if (!bereit || angesteuert === h) return;
		angesteuert = h;
		const aufgabenDa = aufgaben.stand === 'da';
		// Meldungen stehen nur mit Anmeldung da (ohne nur ihre Zahl).
		const wissen = ziel.art === 'aufgabe' ? aufgabenDa : angemeldet;
		void tick().then(() => {
			if (ansteuern(h)) ankerSatz = null;
			else ankerSatz = wissen ? SATZ_NICHT_MEHR_DA : SATZ_OHNE_ANMELDUNG;
		});
	});

	/**
	 * Etappe 3b-2: Derselbe Anker noch einmal (etwa aus der Karte „Fristen“)
	 * ändert die Adresse nicht, der Effekt oben läuft darum nicht. Ein Klick
	 * darauf steuert den Eintrag direkt an, auch wenn er inzwischen zugeklappt
	 * ist. Ein anderer Anker bleibt beim Effekt (kein doppeltes Ansteuern).
	 */
	function beiKlick(ev: MouseEvent): void {
		const id = gleicherAnkerKlick(ev);
		if (id !== null) ansteuern(id);
	}
</script>

<svelte:document onclick={beiKlick} />

<!-- Etappe A2: die Antwort auf eine Frage, im Widget „Frage“ unter dem Titel. -->
{#snippet antwort(e: AnstehendEintrag)}
	{#key frageSchluessel(e)}
		<FrageAntwort eintrag={e} onbeantwortet={nachAntwort} abschnitt="projekt:{slug}" />
	{/key}
{/snippet}

{#if ankerSatz}
	<p class="anker-satz" role="status">{ankerSatz}</p>
{/if}

<!-- Die Ansage steht immer im Dokument (Auflage aus A2); Fokusziel ist die Überschrift. -->
<AntwortStatus projekt={slug} ziel={huelle} abschnitt="projekt:{slug}" />
{#if zeigen || behalten || antworten.meldungFuer(slug) !== null}
	<section class="unter" aria-labelledby="unter-titel">
		<h2 id="unter-titel" class="nur-vorlesen" tabindex="-1" bind:this={huelle}>
			Überblick über das Projekt
		</h2>
		{#if zeigen}
			<div class="reihe">
				{#if stand}
					<Stand
						gesamt={stand.gesamt}
						schwerpunkt={stand.schwerpunkt}
						kurz={stand.kurz}
						speicherFehlt={stand.speicher_fehlt}
					/>
				{/if}

				{#if wartet !== null && wartet.zahl > 0}
					<Frage
						zahl={wartet.zahl}
						fragen={wartet.fragen}
						aufgaben={wartet.aufgaben}
						arbeiten={wartet.arbeiten}
						aufgabenZahl={wartet.aufgabenZahl}
						{angemeldet}
						{antwort}
					/>
				{/if}

				{#if arbeitsGruppen.length > 0}
					<Arbeiten gruppen={arbeitsGruppen} />
				{/if}

				{#if hatFrist && stand?.kurz.naechste_frist}
					<Fristen
						naechste={stand.kurz.naechste_frist}
						tageBis={stand.kurz.tage_bis}
						fristStufe={stand.kurz.frist_stufe}
						eintraege={fristKandidaten}
						dateiFristen={stand.fristen}
					/>
				{/if}

				{#if daten?.hatAktualisierungen}
					<Zuletzt
						{aktualisierungen}
						weitere={weitereMeldungen}
						zahlen={meldungsZahlen}
						ungelesen={ungelesenZahl}
						{angemeldet}
						laeuft={gelesenLaeuft}
						fehler={gelesenFehler}
						ongelesen={(id: string, knopf: HTMLButtonElement) => {
							void gelesen(id, knopf);
						}}
						bind:titel={aktTitel}
					/>
				{/if}

				{#if hatNotizen}
					<div class="karte widget card">
						<h2>Notizen <span class="widget-anzahl">{notizenZahl}</span></h2>
						{#if (stand?.notizen.length ?? 0) === 0}
							<p class="muted small">{SATZ_NOTIZEN_OHNE_ANMELDUNG}</p>
						{/if}
						<ul class="notizen small">
							{#each notizen.vorn as n, i (i)}
								<li>{n}</li>
							{/each}
						</ul>
						{#if notizen.rest.length > 0}
							<details class="widget-mehr">
								<summary class="small">und {notizen.rest.length} weitere</summary>
								<ul class="notizen small">
									{#each notizen.rest as n, i (i)}
										<li>{n}</li>
									{/each}
								</ul>
							</details>
						{/if}
					</div>
				{/if}
			</div>
		{/if}
	</section>
{/if}

<style>
	.unter {
		margin: 0.75rem 0 1rem;
	}
	/* Am Bildschirm nebeneinander, am Handy eine Spalte (.widget in app.css). */
	.reihe {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
	}
	.notizen {
		margin: 0;
		padding-left: 1.2rem;
		display: grid;
		gap: 0.2rem;
	}
	.anker-satz {
		margin: 0.5rem 0;
		padding: 0.4rem 0.75rem;
		border-left: 0.25rem solid var(--accent);
		background: var(--accent-soft);
		border-radius: 0.4rem;
	}
	@media (max-width: 37.5rem) {
		.reihe {
			flex-direction: column;
		}
	}
</style>
