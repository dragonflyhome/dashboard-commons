<script lang="ts">
	/**
	 * Backlog (Etappe B2, Modul Backlog): Aufgaben, Arbeiten und die eine
	 * sichtbare Frage eines Projekts als Spalten nach Stand und Status (die
	 * Kanban-Sicht aus dem Datenspeicher, statt `kanban.toml` von Hand), dazu
	 * „Wer ist dran“: was wartet, je Verantwortlichem (du, ein Chat, jemand
	 * draußen). Alles rechnet der Server (`GET /api/projekte/<slug>/backlog`);
	 * der Baustein zeigt nur.
	 *
	 * Klick-Antworten gibt es hier nicht: Jede Karte führt zu ihrem Eintrag
	 * oben auf der Seite (Überblick oder „Aufgaben für dich“), wo Haken, Knöpfe
	 * und „Etwas anderes …“ stehen. So gibt es jede Antwort nur an einer Stelle.
	 *
	 * Auf jeder Projektseite setzt das Dashboard den Baustein selbst ein
	 * (`BacklogRahmen` im Rahmen der Seite); in `Seite.svelte` nicht noch
	 * einmal einbauen. Mit `backlog` lässt er sich auch mit Beispieldaten zeigen.
	 */
	import type { Backlog, BacklogKarte, BacklogSpalte } from '$vertrag';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { ART_WORT, fristDringend, weitereFragenSatz } from '#lib/anstehend.ts';
	import { fmtDate } from '#lib/format.ts';
	import { zielHref } from '#lib/zieladresse.ts';

	interface Props {
		backlog: Backlog;
	}
	let { backlog }: Props = $props();

	const spalten = $derived(backlog.spalten);
	const leer = $derived(spalten.every((s) => s.karten.length === 0));
	const kennung = $derived(`backlog-${backlog.slug}`);

	/** Die Zahl der Karten einer Spalte als Text. */
	function zahl(s: BacklogSpalte): string {
		return String(s.karten.length);
	}
</script>

{#snippet karte(k: BacklogKarte)}
	{@const href = zielHref(k.ziel)}
	<li class="kkarte" class:frage={k.art === 'frage'}>
		<span class="art muted small">{ART_WORT[k.art]}</span>
		{#if href !== null}
			<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- zielHref baut mit resolve() -->
			<a class="titel" {href}>{k.titel}</a>
		{:else}
			<span class="titel">{k.titel}</span>
		{/if}
		{#if k.frist !== null || k.hinweis !== null}
			<span class="meta small">
				{#if k.frist}
					<span class:achtung={fristDringend(k.frist_stufe)}>bis {fmtDate(k.frist)}</span>
				{/if}
				{#if k.frist !== null && k.hinweis !== null}·{/if}
				{#if k.hinweis}
					<span class="muted">{k.hinweis}</span>
				{/if}
			</span>
		{/if}
	</li>
{/snippet}

<section class="backlog" aria-labelledby="{kennung}-titel">
	<p class="nur-vorlesen" id="{kennung}-titel">Backlog nach Stand</p>
	{#if backlog.speicher_fehlt}
		<p class="muted small">
			Der Datenspeicher fehlt gerade: Aufgaben und Arbeiten stehen hier erst wieder, wenn er da ist.
		</p>
	{/if}
	{#if leer}
		<p class="muted" data-leer>Nichts offen.</p>
	{:else}
		<div class="spalten">
			{#each spalten as s (s.art)}
				<section class="spalte" aria-labelledby="{kennung}-{s.art}">
					<h3 class="kopf" id="{kennung}-{s.art}">
						<StatusDot bedeutung={s.bedeutung} decorative />
						<span class="name">{s.titel}</span>
						<span class="anzahl">{zahl(s)}</span>
					</h3>
					{#if s.karten.length === 0}
						<p class="muted small leer">–</p>
					{:else}
						<ul class="plain karten">
							{#each s.karten as k (`${k.art}-${k.id}`)}
								{@render karte(k)}
							{/each}
						</ul>
					{/if}
				</section>
			{/each}
		</div>
	{/if}

	{#if backlog.wartend.length > 0}
		<h3 class="wer-titel">Wer ist dran</h3>
		<ul class="plain gruppen">
			{#each backlog.wartend as g (g.wer)}
				<li class="gruppe">
					<p class="gruppe-titel">
						<b>{g.titel}</b> <span class="anzahl">{g.karten.length}</span>
					</p>
					<ul class="plain karten">
						{#each g.karten as k (`${k.art}-${k.id}`)}
							{@render karte(k)}
						{/each}
					</ul>
				</li>
			{/each}
		</ul>
	{/if}
	{#if backlog.fragen_weitere > 0}
		<p class="muted small">
			Fragen: eine steht oben; {weitereFragenSatz(backlog.fragen_weitere)} (ohne Text, sie rücken nach
			der Antwort nach).
		</p>
	{/if}
</section>

<style>
	.backlog {
		margin: 0.6em 0 1em;
	}
	.spalten {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
		gap: 0.6rem;
		align-items: start;
	}
	.spalte {
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: color-mix(in srgb, var(--line) 25%, var(--card));
		padding: 0.5rem 0.6rem 0.6rem;
		min-width: 0;
	}
	.kopf {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin: 0 0 0.4rem;
		font-size: var(--text-md);
	}
	.name {
		flex: 1;
	}
	.anzahl {
		font-size: var(--text-xs);
		color: var(--muted);
		font-weight: 400;
	}
	.karten {
		display: grid;
		gap: 0.4rem;
	}
	.kkarte {
		display: grid;
		gap: 0.1rem;
		background: var(--card);
		border: 1px solid var(--line);
		border-radius: 0.5rem;
		padding: 0.4rem 0.6rem;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.kkarte.frage {
		border-left: 0.2rem solid var(--warn);
	}
	.art {
		font-size: var(--text-xs);
	}
	.meta {
		color: var(--muted);
	}
	.achtung {
		color: var(--warn-text);
		font-weight: 600;
	}
	.leer {
		margin: 0;
	}
	.wer-titel {
		margin: 1rem 0 0.4rem;
		font-size: var(--text-md);
	}
	.gruppen {
		display: grid;
		gap: 0.75rem;
	}
	.gruppe-titel {
		margin: 0 0 0.3rem;
	}
</style>
