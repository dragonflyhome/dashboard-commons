<script lang="ts">
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { resolve } from '$app/paths';
	import { flip } from 'svelte/animate';
	import { fade, fly } from 'svelte/transition';
	import ProjectCard from '#lib/components/ProjectCard.svelte';
	import GruppenKarte from '#lib/components/GruppenKarte.svelte';
	import Cockpit from '#lib/components/pc/Cockpit.svelte';
	import Verlauf from '#lib/katalog/zeit/Verlauf.svelte';
	import Kanban from '#lib/components/blocks/Kanban.svelte';
	import AufgabenKasten from '#lib/components/aufgaben/AufgabenKasten.svelte';
	import JetztFuerDich from '#lib/components/anstehend/JetztFuerDich.svelte';
	import { anstehend } from '#lib/anstehend.svelte.ts';
	import { startZahlen } from '#lib/anstehend.ts';
	import { aufgaben } from '#lib/aufgaben.svelte.ts';
	import { bauLage } from '#lib/bau.svelte.ts';
	import ProjektSpalten from '#lib/components/ProjektSpalten.svelte';
	import Wahl from '#lib/katalog/grund/Wahl.svelte';
	import { stepsToColumns } from '#lib/pc.ts';
	import { SATZ_VERLAUF_OHNE_ANMELDUNG } from '#lib/zeit/laden.ts';
	import {
		ansichtLesen,
		ansichtMerken,
		auswerten,
		type Ansicht,
		type Sortierung
	} from '#lib/projekte.ts';
	import { onMount } from 'svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let query = $state('');
	let status = $state<string>('alle');
	let sort = $state<Sortierung>('order');
	// Etappe 3b: Kacheln oder Spalten nach Status, je Browser gemerkt (Standard Kacheln).
	let ansicht = $state<Ansicht>('kacheln');
	onMount(() => {
		ansicht = ansichtLesen();
	});

	// Suchen, Filtern, Sortieren, Zählen und Spalten nur in #lib/projekte.ts.
	// Etappe 4c: der Bau-Stand der Seiten aus derselben Auswertung (live über `bau`).
	const a = $derived(
		auswerten(data.projects, { suche: query, status, sortierung: sort }, bauLage.live, data.gruppen)
	);
	const shown = $derived(a.gezeigt);
	const zeile = $derived(a.zeile);

	// Etappe 3a-1: nur die letzten Tage; der ganze Verlauf steht auf /pc.
	const pcVerlauf = $derived(data.verlauf.verlauf);
	const steps = $derived(stepsToColumns(data.pc.data.steps));

	// Was die Zahlen zählen (Reiter, Leiste, „Für dich“, Glocke).
	const zahlen = $derived(
		anstehend.stand === 'da' && anstehend.liste !== null ? startZahlen(anstehend.liste) : null
	);
	/** Auf der Startseite so viele Aufgaben offen, der Rest zum Aufklappen. */
	const AUFGABEN_OBEN = 5;

	// Dieser Kasten zeigt die Liste der offenen Aufgaben: laden, solange er da ist.
	$effect(() => aufgaben.benutzen());
</script>

<svelte:head>
	<title>{seitenTitel('Übersicht')}</title>
</svelte:head>

<h1>Auf einen Blick</h1>

{#if zahlen !== null}
	<p class="zahlen-zeile muted small" data-zahlen>
		{#each zahlen as z, i (z.wort)}{#if i > 0}
				·
			{/if}<span><b>{z.zahl}</b> {z.wort} ({z.wo})</span>{/each}
	</p>
{/if}

<!-- Prüfbericht: „Jetzt für dich“ zuerst, nicht erst hinter allen Aufgaben.
     Lädt mit dem Reiter (Layout); ohne Anmeldung, bei Fehler oder leer fehlt die Karte. -->
<JetztFuerDich
	liste={anstehend.stand === 'da' ? anstehend.liste : null}
	onbeantwortet={() => anstehend.laden()}
/>

<AufgabenKasten
	stand={aufgaben.stand}
	aufgaben={aufgaben.liste}
	fehler={aufgaben.meldung}
	beschaeftigt={aufgaben.beschaeftigt}
	hoechstens={AUFGABEN_OBEN}
	onhaken={(id: string, antwort: string | null) => void aufgaben.haken(id, antwort)}
	oneigene={(id: string, eigene: string) => aufgaben.rueckmelden(id, eigene)}
/>

<Cockpit report={data.pc} ladung={data.ladung} />

<section class="projects" aria-labelledby="projekte">
	<div class="section-head">
		<h2 id="projekte">Woran die Chats arbeiten</h2>
		<span class="muted small">{zeile}</span>
	</div>

	<div class="toolbar" role="search">
		<input
			type="search"
			placeholder="Suchen: Titel, Chat, Stichwort …"
			aria-label="Projekte durchsuchen"
			bind:value={query}
		/>
		<div class="chips" role="group" aria-label="Nach Status filtern">
			<button
				type="button"
				class="button"
				aria-pressed={status === 'alle'}
				onclick={() => (status = 'alle')}>alle</button
			>
			{#each a.statusWoerter as s (s)}
				<button
					type="button"
					class="button"
					aria-pressed={status === s}
					onclick={() => (status = s)}>{s}</button
				>
			{/each}
		</div>
		<label class="sort">
			<span class="muted small">Sortierung</span>
			<select bind:value={sort}>
				<option value="order">wie geplant</option>
				<option value="title">nach Titel</option>
				<option value="change">zuletzt geändert</option>
			</select>
		</label>
		<Wahl
			bind:wert={
				() => ansicht,
				(w: string) => {
					ansicht = w === 'spalten' ? 'spalten' : 'kacheln';
					ansichtMerken(ansicht);
				}
			}
			beschriftung="Ansicht"
			optionen={[
				{ wert: 'kacheln', text: 'Kacheln' },
				{ wert: 'spalten', text: 'Spalten' }
			]}
		/>
	</div>

	{#if shown.length === 0}
		<p class="muted" in:fade>Nichts gefunden. Andere Suche oder Filter „alle“ probieren.</p>
	{/if}

	{#if ansicht === 'spalten'}
		<ProjektSpalten spalten={a.spalten} />
	{:else}
		<div class="cards" aria-live="polite">
			<!-- Entscheidung projektgruppe: eine Gruppe als eine Karte an der Stelle ihres ersten Teils. -->
			{#each a.kacheln as k, i (k.schluessel)}
				<div animate:flip={{ duration: 250 }} in:fly={{ y: 10, delay: Math.min(i, 8) * 30 }}>
					{#if k.art === 'gruppe'}
						<GruppenKarte
							gruppe={k.gruppe}
							teile={k.teile}
							bedeutung={k.bedeutung}
							status={k.status}
						/>
					{:else}
						<ProjectCard project={k.projekt} bau={a.bau[k.projekt.slug] ?? null} />
					{/if}
				</div>
			{/each}
		</div>
	{/if}

	{#if data.broken.length > 0}
		<section class="card broken">
			<h2>Seiten mit Fehlern</h2>
			<p class="muted small">
				Diese Projekte lassen sich nicht lesen. Meist fehlt in projekt.toml ein Anführungszeichen
				oder ein Feld steht hinter einer [[tabelle]].
			</p>
			<ul>
				{#each data.broken as b (b.slug)}
					<li><code>{b.slug}</code>: {b.error}</li>
				{/each}
			</ul>
		</section>
	{/if}

	<!-- Entscheidung archiv-status: der einzige Weg ins Archiv, ohne Zahl und ohne Namen. -->
	<p class="archiv-link"><a href={resolve('/projekte/archiviert')}>Archiv</a></p>
</section>

<div class="two">
	<section class="card" aria-labelledby="historie">
		<h2 id="historie">Was mit dem PC passiert ist</h2>
		{#if pcVerlauf === null}
			<p class="muted">Der Verlauf lässt sich gerade nicht laden: {data.verlauf.fehler}</p>
		{:else}
			<Verlauf ereignisse={pcVerlauf.ereignisse} leer="In den letzten Tagen ist nichts passiert." />
			{#if data.verlauf.ohneAnmeldung}
				<p class="muted small" role="note">{SATZ_VERLAUF_OHNE_ANMELDUNG}</p>
			{/if}
		{/if}
		<p class="small"><a href={resolve('/pc')}>Ganzer Verlauf auf der PC-Seite</a></p>
	</section>
	<section class="card" aria-labelledby="fahrplan">
		<h2 id="fahrplan">Geplante nächste Schritte</h2>
		{#if steps.length === 0}
			<p class="muted">Nichts geplant.</p>
		{:else}
			<Kanban columns={steps} />
		{/if}
	</section>
</div>

<style>
	.projects {
		margin-top: 22px;
	}
	.archiv-link {
		margin: 0.8rem 0 0;
	}
	.zahlen-zeile {
		margin: -0.4rem 0 0.8rem;
	}
	.section-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		flex-wrap: wrap;
	}
	.toolbar {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		align-items: center;
		margin: 10px 0 16px;
	}
	.toolbar input {
		flex: 1 1 240px;
	}
	.chips {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
	}
	.sort {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.broken {
		margin-top: 20px;
		border-color: var(--bad);
	}
	.two {
		display: grid;
		grid-template-columns: 1fr;
		gap: 14px;
		margin-top: 22px;
	}
	@media (min-width: 62.5rem) {
		.two {
			grid-template-columns: 3fr 2fr;
		}
	}
</style>
