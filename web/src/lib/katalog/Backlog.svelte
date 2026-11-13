<script lang="ts">
	/**
	 * Backlog (Etappe B2, Modul Backlog; seit B8 die Aufgabenliste der
	 * Projektseite): was jetzt zu tun ist und von wem, in Gruppen untereinander
	 * statt Kanban-Spalten: „Wartet auf dich“, „Läuft“, „Als Nächstes“ (in der
	 * Reihenfolge der Etappen, wie der Server sie ordnet), „Wartet auf andere“.
	 * Je Eintrag eine Zeile mit Titel, Etappe, wer dran ist und Frist; Arbeiten
	 * mit Schritten klappen zu ihren Schritten auf (`Schritte`). Fertige
	 * Arbeiten stehen hier nicht (die zeigt der Plan). Alles rechnet der
	 * Server (`GET /api/projekte/<slug>/backlog`); Gruppen und Wörter kommen
	 * aus `#lib/projektteile.ts`, der Baustein zeigt nur.
	 *
	 * Mit `fuerDich` setzt die Projektseite in „Wartet auf dich“ die Frage mit
	 * Antwort und den Kasten der Aufgaben ein; dann stehen hier nur noch die
	 * Arbeiten, die auf dich warten, und jede Antwort gibt es nur an einer
	 * Stelle. Ohne (etwa im Katalog) führen Fragen und Aufgaben als Link zu
	 * ihrem Eintrag.
	 *
	 * Jede Arbeit trägt den Anker `arbeit-<id>`, jede Gruppe `liste-<art>-<slug>`.
	 * Auf jeder Projektseite setzt das Dashboard den Baustein selbst ein; in
	 * `Seite.svelte` nicht noch einmal einbauen.
	 */
	import type { Snippet } from 'svelte';
	import type { Backlog, BacklogKarte } from '$vertrag';
	import { fristDringend, weitereFragenSatz } from '#lib/anstehend.ts';
	import { fristWort } from '#lib/aufgaben.ts';
	import { fmtDate } from '#lib/format.ts';
	import {
		NAECHSTES_SICHTBAR,
		gruppenAnker,
		listenGruppen,
		offeneVerweise,
		zeilenAngaben
	} from '#lib/projektteile.ts';
	import { zielHref } from '#lib/zieladresse.ts';
	import Schritte from './widgets/Schritte.svelte';
	import { schritteKurz } from './widgets/worte.ts';

	interface Props {
		backlog: Backlog;
		/** Titel der Etappen nach `<projekt>/<kennung>` (aus dem Plan). */
		etappen?: ReadonlyMap<string, string>;
		/** Frage mit Antwort und Aufgaben-Kasten der Projektseite (Gruppe „Wartet auf dich“). */
		fuerDich?: Snippet;
		/** Mit `fuerDich`: wie viel auf dich wartet (dieselbe Zahl wie im Stand). */
		fuerDichZahl?: number;
	}
	let { backlog, etappen = new Map<string, string>(), fuerDich, fuerDichZahl }: Props = $props();

	const gruppen = $derived(listenGruppen(backlog, fuerDich !== undefined));
	const verweise = $derived(offeneVerweise(backlog));
	const SATZ_LEER: Record<string, string> = {
		dich: 'Nichts wartet auf dich.',
		laeuft: 'Gerade läuft nichts.',
		naechstes: 'Nichts geplant.',
		andere: 'Nichts wartet auf andere.'
	};
</script>

{#snippet zeile(k: BacklogKarte)}
	{@const angaben = zeilenAngaben(k, etappen)}
	{@const id = k.art === 'arbeit' ? `arbeit-${k.id}` : undefined}
	{@const href = k.art === 'arbeit' ? null : zielHref(k.ziel)}
	{#snippet kopf()}
		{#if href !== null}
			<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- zielHref baut mit resolve() -->
			<a class="titel" {href}>{k.titel}</a>
		{:else}
			<span class="titel">{k.titel}</span>
		{/if}
		<span class="angaben small">
			{#each angaben as a, i (i)}<span>{a}</span>{/each}
			<!-- Heute oder überfällig: das Wort dazu, nicht nur Farbe und Fettdruck. -->
			{#if k.frist}<span class:achtung={fristDringend(k.frist_stufe)}
					>{fristDringend(k.frist_stufe)
						? fristWort(k.frist, k.frist_stufe)
						: `bis ${fmtDate(k.frist)}`}</span
				>{/if}
		</span>
		{#if k.schritte.length > 0}<span class="kurz small muted">{schritteKurz(k.schritte)}</span>{/if}
	{/snippet}
	<li class:frage={k.art === 'frage'}>
		{#if k.schritte.length > 0}
			<details class="zeile" {id}>
				<summary>{@render kopf()}</summary>
				<Schritte schritte={k.schritte} titel={k.titel} offeneVerweise={verweise} />
			</details>
		{:else}
			<div class="zeile ohne" {id}>{@render kopf()}</div>
		{/if}
	</li>
{/snippet}

<div class="backlog">
	{#if backlog.speicher_fehlt}
		<p class="muted small">
			Der Datenspeicher fehlt gerade: Aufgaben und Arbeiten stehen hier erst wieder, wenn er da ist.
		</p>
	{/if}
	{#each gruppen as g (g.art)}
		{@const zahl =
			g.art === 'dich' && fuerDich !== undefined ? (fuerDichZahl ?? 0) : g.karten.length}
		<section class="gruppe" aria-labelledby={gruppenAnker(backlog.slug, g.art)}>
			<h3 id={gruppenAnker(backlog.slug, g.art)} tabindex="-1">
				{g.titel} <span class="anzahl">({zahl})</span>
			</h3>
			{#if g.art === 'dich' && fuerDich !== undefined}
				{@render fuerDich()}
			{/if}
			{#if g.karten.length > 0}
				<ul class="plain zeilen">
					{#each g.art === 'naechstes' ? g.karten.slice(0, NAECHSTES_SICHTBAR) : g.karten as k (`${k.art}-${k.id}`)}
						{@render zeile(k)}
					{/each}
				</ul>
				{#if g.art === 'naechstes' && g.karten.length > NAECHSTES_SICHTBAR}
					<details class="mehr">
						<summary class="small">und {g.karten.length - NAECHSTES_SICHTBAR} weitere</summary>
						<ul class="plain zeilen">
							{#each g.karten.slice(NAECHSTES_SICHTBAR) as k (`${k.art}-${k.id}`)}
								{@render zeile(k)}
							{/each}
						</ul>
					</details>
				{/if}
			{:else if zahl === 0}
				<p class="muted small leer">{SATZ_LEER[g.art]}</p>
			{/if}
		</section>
	{/each}
	{#if backlog.fragen_weitere > 0 && fuerDich === undefined}
		<p class="muted small">
			Fragen: eine steht oben; {weitereFragenSatz(backlog.fragen_weitere)} (ohne Text, sie rücken nach
			der Antwort nach).
		</p>
	{/if}
</div>

<style>
	.backlog {
		margin: 0.2em 0 0.4em;
	}
	.gruppe + .gruppe {
		margin-top: 0.9rem;
	}
	h3 {
		margin: 0 0 0.3rem;
		font-size: var(--text-md);
	}
	.anzahl {
		color: var(--muted);
		font-weight: 400;
	}
	.zeilen > li {
		border-top: 1px solid var(--line);
		padding: 0.4rem 0;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.zeilen > li.frage {
		border-left: 0.2rem solid var(--warn);
		padding-left: 0.5rem;
	}
	summary {
		cursor: pointer;
	}
	.titel {
		font-weight: 600;
		margin-right: 0.5rem;
	}
	/* Ohne Schritte steht kein Dreieck davor: so weit eingerückt wie mit. */
	.ohne {
		padding-left: 1.1rem;
	}
	.angaben {
		color: var(--muted);
	}
	.angaben > span + span::before {
		content: ' · ';
	}
	.kurz {
		display: block;
		margin-left: 1.1rem;
	}
	.ohne .kurz {
		margin-left: 0;
	}
	.achtung {
		color: var(--warn-text);
		font-weight: 600;
	}
	.leer {
		margin: 0;
	}
	.mehr {
		margin-top: 0.3rem;
	}
</style>
