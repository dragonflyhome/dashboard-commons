<script lang="ts">
	/**
	 * Die vier Teile der Projektseite (Etappe B8), über dem Inhalt aus
	 * Seite.svelte, jeder ein Abschnitt mit h2 im Inhaltsverzeichnis und mit
	 * genau einer Frage:
	 *
	 * 1. Stand („Wie geht es dem Projekt?“): Satz, vier Zahlen, Ampel des
	 *    Servers mit Grund; klein dazu der Bau (nur rot oder während er läuft)
	 *    und die Notizen.
	 * 2. Aufgabenliste („Was ist jetzt zu tun, und von wem?“): das Backlog als
	 *    einzige Liste; in „Wartet auf dich“ die Frage mit Antwort und der
	 *    Kasten der Aufgaben.
	 * 3. Plan („Wo stehen wir im Ganzen?“): Etappen aus plan.toml mit Stand
	 *    und Zählung vom Server.
	 * 4. Zeitleiste („Was ist geschehen, was kommt?“): ungelesene Meldungen
	 *    mit „gelesen“, darunter der Verlauf des Projekts (14 Tage, „ältere
	 *    anzeigen“), Kommendes oben.
	 *
	 * Ersetzt das Unter-Dashboard, den eigenen Platz des Aufgaben-Kastens, das
	 * Backlog als eigenen Kasten und den Bau als eigene Zeile. Hier stehen
	 * Laden, Live-Kanal, Mitternacht, Anker und Fokus; die Bausteine zeigen nur.
	 * Neu geladen wird bei `change` für Anstehend oder die Aufgaben des
	 * Projekts, nach dem Nachholen, bei Vorgängen des Projekts, um Mitternacht
	 * und (nur der Plan) bei einer Änderung am Inhalt des Projekts.
	 */
	import { page } from '$app/state';
	import { tick, untrack } from 'svelte';
	import { Abschnitt, Backlog } from '$katalog';
	import Plan from '$katalog/Plan.svelte';
	import Stand from '$katalog/widgets/Stand.svelte';
	import Eintrag from '$katalog/widgets/Eintrag.svelte';
	import Meldung from '$katalog/widgets/Meldung.svelte';
	import Verlauf from '$katalog/zeit/Verlauf.svelte';
	import AntwortStatus from '#lib/components/anstehend/AntwortStatus.svelte';
	import FrageAntwort from '#lib/components/anstehend/FrageAntwort.svelte';
	import AufgabenKasten from '#lib/components/aufgaben/AufgabenKasten.svelte';
	import BauKopf from '#lib/components/projekt/BauKopf.svelte';
	import { frageSchluessel } from '#lib/antworten.ts';
	import { aufgaben } from '#lib/aufgaben.svelte.ts';
	import { bauLage } from '#lib/bau.svelte.ts';
	import { live, betrifftProjekt, type InhaltAenderung } from '#lib/live.svelte.ts';
	import { ApiError } from '#lib/api/client.ts';
	import {
		SATZ_NICHT_MEHR_DA,
		SATZ_NOTIZEN_OHNE_ANMELDUNG,
		SATZ_OHNE_ANMELDUNG,
		meldungGelesen
	} from '#lib/anstehend.ts';
	import {
		ZEITLEISTE_AELTER,
		ZEITLEISTE_TAGE,
		etappenNamen,
		gruppenAnker,
		ladeBacklog,
		ladePlan,
		listenAnker,
		ladeProjektVerlauf,
		ungeleseneMeldungen,
		zeitleisteAnker,
		zeitleisteEreignisse,
		type Ladung
	} from '#lib/projektteile.ts';
	import { beiTageswechsel } from '#lib/tageswechsel.ts';
	import { ladeUnterDaten, type UnterDaten } from '#lib/unterdashboard.ts';
	import { hinweiseFuerKasten } from '#lib/zeit/laden.ts';
	import { ankerAus, ankerZumEintrag, ansteuern, gleicherAnkerKlick } from '#lib/ziel.ts';
	import { mitZugang } from '#lib/zugang.ts';
	import type { Backlog as BacklogDaten, ProjektPlan, Verlauf as VerlaufDaten } from '$vertrag';

	interface Props {
		slug: string;
		/** Archiviert (Entscheidung archiv-status): seine Aufgaben stehen nicht in der allgemeinen Liste. */
		archiviert?: boolean;
		/** Für Tests; sonst das globale fetch. */
		fetchFn?: typeof fetch;
	}
	let { slug, archiviert = false, fetchFn = (...a) => fetch(...a) }: Props = $props();

	// ----- Laden: je Quelle eine laufende Nummer, eine späte Antwort überschreibt nie eine neuere -----

	let daten = $state<UnterDaten | null>(null);
	let geladenFuer = $state<string | null>(null);
	let liste = $state<Ladung<BacklogDaten> | null>(null);
	let plan = $state<Ladung<ProjektPlan> | null>(null);
	let verlauf = $state<Ladung<VerlaufDaten> | null>(null);
	/** Die Zeitleiste zeigt so viele Tage bis heute. */
	let tage = $state(ZEITLEISTE_TAGE);
	const nr = { stand: 0, backlog: 0, plan: 0, verlauf: 0 };
	const SATZ_LISTE_FEHLT = 'Die Aufgabenliste lässt sich gerade nicht laden.';

	async function ladeStand(fuer: string): Promise<void> {
		const n = ++nr.stand;
		const d = await ladeUnterDaten(fuer, fetchFn);
		if (fuer !== slug || n !== nr.stand) return;
		daten = d;
		geladenFuer = fuer;
	}
	async function ladeListe(fuer: string): Promise<void> {
		const n = ++nr.backlog;
		const b = await ladeBacklog(fuer, mitZugang(fetchFn));
		if (fuer !== slug || n !== nr.backlog) return;
		liste = b;
	}
	async function ladePlanFuer(fuer: string): Promise<void> {
		const n = ++nr.plan;
		const p = await ladePlan(fuer, mitZugang(fetchFn));
		if (fuer !== slug || n !== nr.plan) return;
		plan = p;
	}
	async function ladeVerlaufFuer(fuer: string, t: number): Promise<void> {
		const n = ++nr.verlauf;
		const v = await ladeProjektVerlauf(fuer, t, mitZugang(fetchFn));
		if (fuer !== slug || n !== nr.verlauf) return;
		verlauf = v;
	}
	function allesLaden(s: string): void {
		void ladeStand(s);
		void ladeListe(s);
		void ladePlanFuer(s);
		// Ohne Abhängigkeit: „Ältere anzeigen“ lädt selbst und setzt die Seite nicht zurück.
		void ladeVerlaufFuer(
			s,
			untrack(() => tage)
		);
	}

	$effect(() => {
		const s = slug;
		daten = null;
		liste = null;
		plan = null;
		verlauf = null;
		tage = ZEITLEISTE_TAGE;
		allesLaden(s);
		const aus = [
			live.onChange((c) => {
				const betrifft =
					(c.area === 'anstehend' && (c.path === 'anstehend' || c.path === `anstehend/${s}`)) ||
					(c.area === 'aufgaben' && c.path === `aufgaben/${s}`);
				if (betrifft) allesLaden(s);
			}),
			live.onNachholen(() => {
				allesLaden(s);
			}),
			live.onVorgang((v) => {
				if (v.slug === s) allesLaden(s);
			}),
			// Um Mitternacht neu rechnen lassen: Frist morgen wird heute, die Zeitleiste rückt weiter.
			beiTageswechsel(() => {
				allesLaden(s);
			}),
			// daten/plan.toml geändert: der Plan und die Liste (Etappen-Titel, Folge in „Als Nächstes“).
			live.onInhalt((a: InhaltAenderung) => {
				if (!betrifftProjekt(a, s)) return;
				void ladePlanFuer(s);
				void ladeListe(s);
			})
		];
		return () => {
			for (const ab of aus) ab();
		};
	});

	// Die Aufgaben selbst lädt der Rahmen (`aufgaben.benutzen`, bei archivierten `ladenFuer`).
	const stand = $derived(daten?.stand ?? null);
	// Der Kopf (BauKopf) sagt bei roter Ampel nicht „Seite in Ordnung“.
	$effect(() => {
		if (geladenFuer === slug) bauLage.standMelden(slug, stand?.gesamt.ampel === 'rot');
	});
	const angemeldet = $derived(daten?.angemeldet ?? false);
	const wartet = $derived(daten?.wartet ?? null);
	const notizenZahl = $derived(stand?.notizen_zahl ?? stand?.notizen.length ?? 0);
	const etappen = $derived(etappenNamen(plan?.wert ?? null));
	const backlog = $derived(liste?.wert ?? null);
	const meldungen = $derived(
		ungeleseneMeldungen(daten?.eintraege ?? [], verlauf?.wert?.ereignisse ?? [])
	);
	const ereignisse = $derived(zeitleisteEreignisse(verlauf?.wert?.ereignisse ?? [], meldungen));

	// ----- Antworten und „gelesen“ -----

	/**
	 * Fokusziel nach einer Antwort: eine Zeile nur für Vorlese-Programme in
	 * „Wartet auf dich“ (tabindex -1), nicht die Meldung selbst (Auflage aus A2).
	 */
	let huelle = $state<HTMLElement | null>(null);
	function nachAntwort(): Promise<void> {
		void ladeListe(slug);
		void ladeVerlaufFuer(slug, tage);
		return ladeStand(slug);
	}

	let gelesenLaeuft = $state<string | null>(null);
	let gelesenFehler = $state<string | null>(null);
	/**
	 * Fokusziel der Zeitleiste nach „gelesen“ und „Ältere anzeigen“: eine Zeile
	 * nur für Vorlese-Programme, die auch ohne Meldungen steht, nicht der
	 * Kasten mit allen Meldungen (Auflage aus A2).
	 */
	let zeitFokus = $state<HTMLElement | null>(null);
	/** „gelesen“: danach neu laden; den Fokus nur nehmen, wenn er noch auf dem Knopf oder body liegt. */
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
			await Promise.all([ladeStand(slug), ladeVerlaufFuer(slug, tage)]);
		} finally {
			gelesenLaeuft = null;
		}
		await tick();
		const aktiv = document.activeElement;
		if (aktiv === null || aktiv === document.body || aktiv === knopf) zeitFokus?.focus();
	}

	/** Der Knopf verschwindet mit dem Klick; der Fokus bleibt in der Zeitleiste. */
	function aelterAnzeigen(): void {
		tage = ZEITLEISTE_AELTER;
		zeitFokus?.focus();
		void ladeVerlaufFuer(slug, tage);
	}

	// ----- Anker: `#<art>-<id>` aufklappen, hervorheben, Fokus, sobald die Daten da sind -----

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
		// Fragen, Aufgaben und Arbeiten stehen in der Aufgabenliste, Meldungen kommen mit dem Stand.
		const bereit =
			geladenFuer === slug &&
			liste !== null &&
			(ziel.art !== 'arbeit' || plan !== null) &&
			(ziel.art !== 'aufgabe' || aufgaben.stand !== 'laedt');
		if (!bereit || angesteuert === h) return;
		angesteuert = h;
		const wissen = ziel.art === 'aufgabe' ? aufgaben.stand === 'da' : angemeldet;
		const ohneListe = ziel.art === 'arbeit' && liste?.wert === null && !liste.anmelden;
		void tick().then(() => {
			if (ansteuern(h)) ankerSatz = null;
			else if (ohneListe) ankerSatz = SATZ_LISTE_FEHLT;
			else ankerSatz = wissen ? SATZ_NICHT_MEHR_DA : SATZ_OHNE_ANMELDUNG;
		});
	});

	/** Derselbe Anker noch einmal (Etappe 3b-2): direkt ansteuern, auch wenn zugeklappt. */
	function beiKlick(ev: MouseEvent): void {
		const id = gleicherAnkerKlick(ev);
		if (id !== null) ansteuern(id);
	}
</script>

<svelte:document onclick={beiKlick} />

<!-- In „Wartet auf dich“ der Aufgabenliste: die Frage mit Antwort und der Kasten der Aufgaben. -->
{#snippet fuerDich()}
	<p class="nur-vorlesen" tabindex="-1" bind:this={huelle}>Fragen und Aufgaben für dich</p>
	{#if wartet !== null && wartet.fragen.length > 0}
		<ul class="plain fragen">
			{#each wartet.fragen as e, i (e.id)}
				<li>
					<Eintrag eintrag={e} offen={i === 0}>
						{#key frageSchluessel(e)}
							<FrageAntwort eintrag={e} onbeantwortet={nachAntwort} abschnitt="projekt:{slug}" />
						{/key}
					</Eintrag>
				</li>
			{/each}
		</ul>
	{/if}
	<AufgabenKasten
		titel="Aufgaben für dich"
		ebene={4}
		stand={aufgaben.stand}
		aufgaben={aufgaben.fuer(slug, archiviert)}
		fehler={aufgaben.meldung}
		beschaeftigt={aufgaben.beschaeftigt}
		mitProjekt={false}
		leerVerbergen
		onhaken={(id: string, antwort: string | null) => void aufgaben.haken(id, antwort)}
		oneigene={(id: string, eigene: string) => aufgaben.rueckmelden(id, eigene)}
	/>
{/snippet}

{#if ankerSatz}
	<p class="anker-satz" role="status">{ankerSatz}</p>
{/if}
<!-- Die Ansage steht immer im Dokument (Auflage aus A2). -->
<AntwortStatus projekt={slug} ziel={huelle} abschnitt="projekt:{slug}" />

<div class="teile" data-teile>
	<Abschnitt titel="Stand">
		{#if stand !== null}
			<Stand
				gesamt={stand.gesamt}
				schwerpunkt={stand.schwerpunkt}
				kurz={stand.kurz}
				speicherFehlt={stand.speicher_fehlt}
				teil={{ slug, fertig7: stand.fertig_7_tage, springen: backlog !== null }}
			>
				<BauKopf {slug} nurAuffaellig />
				{#if notizenZahl > 0}
					<details class="notizen small">
						<summary>Notizen ({notizenZahl})</summary>
						{#if stand.notizen.length === 0}
							<p class="muted">{SATZ_NOTIZEN_OHNE_ANMELDUNG}</p>
						{:else}
							<ul>
								{#each stand.notizen as n, i (i)}
									<li>{n}</li>
								{/each}
							</ul>
						{/if}
					</details>
				{/if}
			</Stand>
		{:else if geladenFuer === slug}
			<p class="muted">Der Stand lässt sich gerade nicht laden.</p>
		{:else}
			<p class="muted">Stand wird geladen …</p>
		{/if}
	</Abschnitt>

	<Abschnitt titel="Aufgabenliste">
		<div id={listenAnker(slug)}>
			{#if backlog !== null}
				<Backlog {backlog} {etappen} {fuerDich} fuerDichZahl={wartet?.zahl ?? 0} />
			{:else if liste?.anmelden}
				<p class="muted">{SATZ_OHNE_ANMELDUNG}</p>
			{:else if liste !== null}
				<!-- Fragen und Aufgaben kommen nicht aus /backlog: Sie bleiben beantwortbar. -->
				<p class="muted">{SATZ_LISTE_FEHLT}</p>
				<section aria-labelledby={gruppenAnker(slug, 'dich')}>
					<h3 id={gruppenAnker(slug, 'dich')} tabindex="-1">
						Wartet auf dich <span class="muted">({wartet?.zahl ?? 0})</span>
					</h3>
					{@render fuerDich()}
				</section>
			{:else}
				<p class="muted">Aufgabenliste wird geladen …</p>
			{/if}
		</div>
	</Abschnitt>

	<Abschnitt titel="Plan">
		{#if plan?.wert}
			<Plan plan={plan.wert} />
		{:else if plan !== null}
			<p class="muted">
				{plan.anmelden ? SATZ_OHNE_ANMELDUNG : 'Der Plan lässt sich gerade nicht laden.'}
			</p>
		{:else}
			<p class="muted">Plan wird geladen …</p>
		{/if}
	</Abschnitt>

	<Abschnitt titel="Zeitleiste">
		<div id={zeitleisteAnker(slug)}>
			<p class="nur-vorlesen" tabindex="-1" bind:this={zeitFokus}>Zeitleiste</p>
			{#if meldungen.length > 0}
				<h3>Neue Meldungen ({meldungen.length})</h3>
				<ul class="plain widget-liste">
					{#each meldungen as m (m.id)}
						<li>
							<Meldung
								meldung={m}
								knopf={angemeldet}
								gesperrt={gelesenLaeuft !== null}
								ongelesen={(id: string, knopf: HTMLButtonElement) => {
									void gelesen(id, knopf);
								}}
							/>
						</li>
					{/each}
				</ul>
			{/if}
			{#if gelesenFehler}
				<p class="block-error small" role="alert">{gelesenFehler}</p>
			{/if}
		</div>
		{#if verlauf?.wert}
			<Verlauf
				{ereignisse}
				hinweise={hinweiseFuerKasten(verlauf.wert.hinweise, false)}
				ebene={3}
				leer="In diesen Tagen ist nichts geschehen, und nichts steht an."
			/>
			{#if tage < ZEITLEISTE_AELTER}
				<button type="button" class="pill" onclick={aelterAnzeigen}>Ältere anzeigen</button>
			{/if}
		{:else if verlauf !== null}
			<p class="muted">
				{verlauf.anmelden ? SATZ_OHNE_ANMELDUNG : 'Die Zeitleiste lässt sich gerade nicht laden.'}
			</p>
		{:else}
			<p class="muted">Zeitleiste wird geladen …</p>
		{/if}
	</Abschnitt>
</div>

<style>
	.teile {
		min-width: 0;
	}
	.anker-satz {
		margin: 0.5rem 0;
		padding: 0.4rem 0.75rem;
		border-left: 0.25rem solid var(--accent);
		background: var(--accent-soft);
		border-radius: 0.4rem;
	}
	.notizen {
		margin-top: 0.3rem;
	}
	.notizen summary {
		cursor: pointer;
	}
	.notizen ul {
		margin: 0.2rem 0 0;
		padding-left: 1.2rem;
	}
	.fragen {
		display: grid;
		gap: 0.4rem;
		margin: 0 0 0.6rem;
	}
	h3 {
		margin: 0 0 0.3rem;
		font-size: var(--text-md);
	}
	.pill {
		border: 1px solid var(--line);
		cursor: pointer;
		font: inherit;
		font-size: var(--text-sm);
	}
</style>
