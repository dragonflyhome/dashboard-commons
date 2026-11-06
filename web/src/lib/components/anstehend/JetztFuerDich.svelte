<script lang="ts">
	/**
	 * „Jetzt für dich“ auf der Startseite (Etappe A1, Konzept [uebersicht]):
	 * je Projekt die aktuelle Frage (eine auf einmal, mit „noch N Fragen
	 * warten“) und was unter Jetzt steht, mit Link auf „Anstehend“. Aufgaben
	 * stehen im Kasten „Für dich“ darunter und fehlen hier (seit dem
	 * Prüfbericht steht diese Karte zuerst).
	 *
	 * Etappe 3b-2: Auch Fragen nur aus der Stufe Jetzt (wirksame Stufe vom
	 * Server); ausgewählt wird in `jetztFuerDich` (#lib/anstehend.ts), nicht
	 * hier.
	 *
	 * Die Daten kommen aus dem gemeinsamen Speicher `anstehend` (dieselbe
	 * Abfrage wie die Zahl am Reiter). Ohne Liste (lädt, ohne Anmeldung,
	 * Fehler) oder ohne Einträge fehlt die Karte ganz, ohne Satz.
	 *
	 * Etappe A2: Die Frage lässt sich hier direkt beantworten. Danach lädt die
	 * Liste neu, die nächste Frage rückt nach; die Rückmeldung steht oben in
	 * der Karte. Der Fokus geht dabei auf die Überschrift (tabindex -1, außerhalb
	 * der Ansage), damit er nicht auf body fällt, wenn die Frage verschwindet.
	 *
	 * Wer hier geantwortet hat, behält die Karte, bis er die Seite verlässt
	 * (dann mit „Gerade wartet nichts weiter auf dich.“), auch wenn nichts mehr
	 * ansteht und die Meldung nach 15 s weg ist. So bleibt das Fokusziel stehen,
	 * und die Seite springt nicht unter dem Fokus weg. Eine Meldung von einer
	 * anderen Seite zeigt die Karte nur, solange sie steht.
	 */
	import { resolve } from '$app/paths';
	import { projektAusZiel } from '#lib/zieladresse.ts';
	import AntwortStatus from './AntwortStatus.svelte';
	import FrageAntwort from './FrageAntwort.svelte';
	import { antworten, type Antworten } from '#lib/antworten.svelte.ts';
	import { frageSchluessel } from '#lib/antworten.ts';
	import {
		ART_WORT,
		ART_ZEICHEN,
		anker,
		archivFristenStart,
		fristDringend,
		fristWorte,
		jetztKarten,
		type AnstehendAnsicht
	} from '#lib/anstehend.ts';
	import { teilen } from '#lib/unterdashboard.ts';
	import type { AnstehendEintrag } from '$vertrag';

	interface Props {
		/** Ausgewertet (`auswerten`); `null`: nichts zeigen. */
		liste: Pick<AnstehendAnsicht, 'eintraege' | 'archiv_fristen'> | null;
		/** So viele Projekte, dann „und N weitere“. */
		hoechstens?: number;
		/** Nach dem Beantworten einer Frage: neu laden. */
		onbeantwortet?: () => void | Promise<void>;
		/** Für Tests; sonst der gemeinsame Speicher der Antworten. */
		speicher?: Antworten;
	}
	let { liste, hoechstens = 5, onbeantwortet, speicher = antworten }: Props = $props();

	const kennung = $props.id();
	// Projektgruppen: eine Karte je Gruppe (zählt bei „weitere Projekte“ einmal).
	const projekte = $derived(liste === null ? [] : jetztKarten(liste.eintraege));
	const teil = $derived(teilen(projekte, hoechstens));
	/**
	 * Entscheidung archiv-status: nahe Fristen in archivierten
	 * Projekten, je eine eigene Zeile ohne Projekttitel (nicht im Tab-Titel,
	 * nicht in der Glocke).
	 */
	// Auflage B3: auf der Startseite nur Fristen mit Datum (die ohne Datum haben kein Ende).
	const archivFristen = $derived(archivFristenStart(liste?.archiv_fristen));
	/** Fokusziel für die Rückmeldung: die Überschrift der Karte. */
	let titel = $state<HTMLElement | null>(null);
	/** Hier wurde geantwortet: Die Karte bleibt bis zum Verlassen der Seite. */
	let behalten = $state(false);
	const zeigen = $derived(
		teil.vorn.length > 0 ||
			archivFristen.length > 0 ||
			behalten ||
			(liste !== null && speicher.meldungFuer(null) !== null)
	);

	function nachAntwort(): void | Promise<void> {
		behalten = true;
		return onbeantwortet?.();
	}

	/** Nur den Anker aus `ziel`; die Adresse baut resolve(). */
	function hash(e: AnstehendEintrag): string {
		const i = e.ziel.indexOf('#');
		return i >= 0 ? e.ziel.slice(i + 1) : anker(e);
	}
</script>

{#snippet zeile(e: AnstehendEintrag)}
	{@const frist = fristWorte(e.tage_bis)}
	<li>
		<span class="art"><span aria-hidden="true">{ART_ZEICHEN[e.art]}</span> {ART_WORT[e.art]}</span>
		{#if e.ziel_da}
			<a href="{resolve('/projects/[slug]', { slug: e.projekt })}#{hash(e)}">{e.titel}</a>
		{:else}
			<span>{e.titel} <span class="muted">(nicht mehr da)</span></span>
		{/if}
		{#if frist}<span class="muted small" class:achtung={fristDringend(e.frist_stufe)}>
				· Frist {frist}</span
			>{/if}
		{#if e.vermerk}<span class="muted small"> · {e.vermerk}</span>{/if}
		{#if e.art === 'frage'}
			{#key frageSchluessel(e)}
				<FrageAntwort eintrag={e} onbeantwortet={nachAntwort} abschnitt="start" {speicher} />
			{/key}
		{/if}
	</li>
{/snippet}

{#if zeigen}
	<section class="card jetzt-fuer-dich" aria-labelledby="{kennung}-titel">
		<div class="kopf">
			<h2 id="{kennung}-titel" tabindex="-1" bind:this={titel}>Jetzt für dich</h2>
			<a class="small" href={resolve('/anstehend')}>Alles in „Anstehend“</a>
		</div>
		<AntwortStatus {speicher} ziel={titel} abschnitt="start" />
		{#if teil.vorn.length === 0 && archivFristen.length === 0 && liste !== null}
			<p class="muted small">Gerade wartet nichts weiter auf dich.</p>
		{/if}
		<ul class="plain projekte">
			{#each teil.vorn as k (k.schluessel)}
				<li>
					<h3>{k.titel}</h3>
					{#each k.teile as p (p.projekt)}
						{#if k.gruppe !== null}<h4>{p.titel}</h4>{/if}
						<ul class="plain eintraege">
							{#if p.frage}{@render zeile(p.frage)}{/if}
							{#each p.jetzt as e (`${e.art}-${e.id}`)}{@render zeile(e)}{/each}
						</ul>
					{/each}
				</li>
			{/each}
		</ul>
		{#if archivFristen.length > 0}
			<ul class="plain eintraege archiv-fristen" aria-label="Fristen im Archiv">
				{#each archivFristen as f (`${f.ziel}-${f.frist}`)}
					{@const slug = projektAusZiel(f.ziel)}
					<li>
						<span>{f.satz}</span>
						{#if slug !== null}
							<a class="small" href={resolve('/projects/[slug]', { slug })}>Zur Projektseite</a>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
		{#if teil.rest.length > 0}
			<p class="small">
				<a href={resolve('/anstehend')}
					>und {teil.rest.length === 1
						? 'ein weiteres Projekt'
						: `${String(teil.rest.length)} weitere Projekte`}</a
				>
			</p>
		{/if}
	</section>
{/if}

<style>
	.jetzt-fuer-dich {
		margin-bottom: 1rem;
		border-top: 0.25rem solid var(--warn);
		min-width: 0;
	}
	.kopf {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.25rem 0.75rem;
		margin-bottom: 0.4rem;
	}
	h2 {
		margin: 0;
	}
	h3 {
		font-size: var(--text-sm);
		color: var(--muted);
		font-weight: 600;
		margin: 0 0 0.15rem;
	}
	h4 {
		font-size: var(--text-xs);
		color: var(--muted);
		font-weight: 600;
		margin: 0.2rem 0 0.1rem;
	}
	.archiv-fristen {
		margin-top: 0.6rem;
	}
	.projekte {
		display: grid;
		gap: 0.6rem;
	}
	.eintraege {
		display: grid;
		gap: 0.2rem;
	}
	.eintraege li {
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.art {
		color: var(--muted);
		font-size: var(--text-xs);
		margin-right: 0.25rem;
	}
	.achtung {
		color: var(--warn-text);
	}
</style>
