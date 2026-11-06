<script lang="ts">
	/**
	 * „Anstehend“ (Etappe A1): alles, was ansteht, aus allen Projekten. Oben
	 * die Zahlen, darunter Ansicht und Filter (Zustand in der Adresse, damit
	 * Zurück und Teilen gehen), dann die Gruppen. Die Liste kommt aus dem
	 * gemeinsamen Speicher `anstehend` (dieselbe Abfrage wie die Zahl am
	 * Reiter); der Live-Kanal lädt sie neu.
	 */
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { projektAusZiel } from '#lib/zieladresse.ts';
	import { page } from '$app/state';
	import { onMount, tick } from 'svelte';
	import EintragZeile from '#lib/components/anstehend/EintragZeile.svelte';
	import EdgeHinweis from '#lib/components/EdgeHinweis.svelte';
	import AntwortStatus from '#lib/components/anstehend/AntwortStatus.svelte';
	import AlleFristen from '#lib/components/anstehend/AlleFristen.svelte';
	import { anstehend } from '#lib/anstehend.svelte.ts';
	import { aufgaben } from '#lib/aufgaben.svelte.ts';
	import {
		ARTEN,
		ART_MEHRZAHL,
		STUFE_WORT,
		artUmschalten,
		filterAus,
		gefiltert,
		gruppenTitel,
		nachPrioritaet,
		nachSchwerpunkt,
		sucheAus,
		zahlKacheln,
		type AnstehendFilter
	} from '#lib/anstehend.ts';
	import type { AnstehendEintrag, Stufe } from '$vertrag';

	const filter = $derived(filterAus(page.url.searchParams));
	const liste = $derived(anstehend.liste);
	const sichtbar = $derived(liste === null ? [] : gefiltert(liste.eintraege, filter));
	const prioritaet = $derived(nachPrioritaet(sichtbar));
	const schwerpunkte = $derived(nachSchwerpunkt(sichtbar));
	const archivFristen = $derived(liste?.archiv_fristen ?? []);
	const gesperrt = $derived(anstehend.beschaeftigt !== null || aufgaben.beschaeftigt !== null);
	const meldung = $derived(
		anstehend.umstufenFehler ?? anstehend.gelesenFehler ?? aufgaben.hakenFehler
	);

	onMount(() => {
		void anstehend.laden();
	});

	/** Die Zahl-Kacheln oben: je die Zahl der Liste, die ihr Link zeigt (mit Filter). */
	const kacheln = $derived(liste === null ? [] : zahlKacheln(liste.eintraege, filter));

	function setze(f: AnstehendFilter): void {
		// Die Route ist aufgelöst; dahinter steht nur die Suche (?ansicht=&art=&nur=).
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- resolve() kennt keine Suche
		void goto(resolve('/anstehend') + sucheAus(f), { reset: false });
	}
	function umstufen(e: AnstehendEintrag, stufe: Stufe | null): void {
		void anstehend.umstufen(e, stufe);
	}
	async function haken(id: string, antwort: string | null): Promise<void> {
		await aufgaben.haken(id, antwort);
		await anstehend.laden();
	}
	const onhaken = (id: string, antwort: string | null): void => {
		void haken(id, antwort);
	};
	/** „Etwas anderes …“ bei einer Aufgabe; danach steht sie „beim Chat“. */
	async function oneigene(id: string, eigene: string): Promise<string | null> {
		const satz = await aufgaben.rueckmelden(id, eigene);
		await anstehend.laden();
		return satz;
	}
	const onbeantwortet = (): Promise<void> => anstehend.laden();
	/**
	 * Etappe A3: Meldung wegklicken. Die Zeile verschwindet danach; liegt der
	 * Fokus noch auf ihrem Knopf (oder auf body), geht er auf die Überschrift.
	 */
	async function gelesen(e: AnstehendEintrag, knopf: HTMLButtonElement): Promise<void> {
		await anstehend.gelesen(e);
		await tick();
		const aktiv = document.activeElement;
		if (aktiv === null || aktiv === document.body || aktiv === knopf) titel?.focus();
	}
	const ongelesen = (e: AnstehendEintrag, knopf: HTMLButtonElement): void => {
		void gelesen(e, knopf);
	};
	/**
	 * Nach einer Antwort verschwindet die Frage samt Knopf; der Fokus geht dann
	 * auf die Überschrift der Seite (sie bleibt immer stehen), die Meldung sagt
	 * `AntwortStatus` an.
	 */
	let titel = $state<HTMLElement | null>(null);
</script>

<svelte:head>
	<title>{seitenTitel('Anstehend')}</title>
</svelte:head>

<!-- Fokusziel nach einer Antwort: steht immer da, außerhalb der Ansage. -->
<h1 tabindex="-1" bind:this={titel}>Anstehend</h1>

{#if anstehend.stand === 'laedt'}
	<p class="muted">Anstehendes wird geladen …</p>
{:else if anstehend.stand !== 'da' || liste === null}
	<EdgeHinweis class="card muted" satz={anstehend.fehler ?? ''} />
{:else}
	<!-- Jede Kachel ist ein Link auf ihr Ziel (die Stufe in der
	     Ansicht „nach Priorität“, bei „wartet auf dich“ der Filter dazu).
	     Jede Kachel zählt die Liste, die ihr Link zeigt, mit dem Filter (zahlKacheln). -->
	<ul class="plain zahlen" aria-label="Anzahl nach Stufe">
		{#each kacheln as k (k.wort)}
			<li>
				<a class="zahl-kachel {k.klasse}" href="{resolve('/anstehend')}{k.suche}"
					><span class="zahl">{k.zahl}</span> <span>{k.wort}</span></a
				>
			</li>
		{/each}
	</ul>

	<div class="steuerung">
		<div class="schalter" role="group" aria-label="Ansicht">
			<button
				type="button"
				class="button"
				aria-pressed={filter.ansicht === 'prioritaet'}
				onclick={() => {
					setze({ ...filter, ansicht: 'prioritaet' });
				}}>nach Priorität</button
			>
			<button
				type="button"
				class="button"
				aria-pressed={filter.ansicht === 'schwerpunkt'}
				onclick={() => {
					setze({ ...filter, ansicht: 'schwerpunkt' });
				}}>nach Schwerpunkt</button
			>
		</div>
		<div class="chips" role="group" aria-label="Filter">
			{#each ARTEN as a (a)}
				<button
					type="button"
					class="chip"
					aria-pressed={filter.arten.includes(a)}
					onclick={() => {
						setze(artUmschalten(filter, a));
					}}>{ART_MEHRZAHL[a]}</button
				>
			{/each}
			<button
				type="button"
				class="chip"
				aria-pressed={filter.nur}
				onclick={() => {
					setze({ ...filter, nur: !filter.nur });
				}}>nur was auf mich wartet</button
			>
			<!-- Entscheidung projektgruppe: Filter nach Gruppe (alle Teile), aus ?gruppe=. -->
			{#if filter.gruppe !== undefined}
				<button
					type="button"
					class="chip"
					aria-pressed="true"
					onclick={() => {
						setze({ ansicht: filter.ansicht, arten: filter.arten, nur: filter.nur });
					}}>nur {gruppenTitel(liste.eintraege, filter.gruppe)} ✕</button
				>
			{/if}
		</div>
	</div>

	<p class="muted small" aria-live="polite">
		{sichtbar.length === liste.eintraege.length
			? `${String(sichtbar.length)} Einträge`
			: `${String(sichtbar.length)} von ${String(liste.eintraege.length)} Einträgen`}
	</p>
	{#if meldung}
		<p class="block-error" role="alert">{meldung}</p>
	{/if}
	<AntwortStatus ziel={titel} abschnitt="anstehend" />
	{#each liste.hinweise as h (h)}
		<p class="muted small">{h}</p>
	{/each}

	<!-- Entscheidung archiv-status, Sicherheitsregel: nahe Fristen in
	     archivierten Projekten, neutral und ohne Projekttitel; sie zählen nicht mit. -->
	{#if archivFristen.length > 0}
		<ul class="plain archiv-fristen" aria-label="Fristen im Archiv">
			{#each archivFristen as f (`${f.ziel}-${f.frist}-${f.satz}`)}
				{@const slug = projektAusZiel(f.ziel)}
				<li class="card">
					<span>{f.satz}</span>
					{#if slug !== null}
						<a href={resolve('/projects/[slug]', { slug })}>Zur Projektseite</a>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}

	{#if filter.ansicht === 'prioritaet'}
		{#each prioritaet as g (g.stufe)}
			<section class="gruppe" aria-labelledby="stufe-{g.stufe}">
				<h2 id="stufe-{g.stufe}">
					{STUFE_WORT[g.stufe]} <span class="anzahl">{g.eintraege.length}</span>
				</h2>
				{#if g.eintraege.length === 0}
					<p class="muted small">Nichts in dieser Stufe.</p>
				{:else}
					<ul class="plain liste">
						{#each g.eintraege as e (`${e.art}-${e.projekt}-${e.id}`)}
							<EintragZeile
								eintrag={e}
								{gesperrt}
								onumstufen={umstufen}
								{onhaken}
								{oneigene}
								{onbeantwortet}
								{ongelesen}
							/>
						{/each}
					</ul>
				{/if}
			</section>
		{/each}
	{:else}
		{#each schwerpunkte as g, i (g.name)}
			<section class="gruppe" aria-labelledby="schwerpunkt-{i}">
				<h2 id="schwerpunkt-{i}">{g.name} <span class="anzahl">{g.anzahl}</span></h2>
				{#each g.projekte as p (p.projekt)}
					<h3>{p.titel}</h3>
					<ul class="plain liste">
						{#each p.eintraege as e (`${e.art}-${e.projekt}-${e.id}`)}
							<EintragZeile
								eintrag={e}
								mitSchwerpunkt={false}
								{gesperrt}
								onumstufen={umstufen}
								{onhaken}
								{oneigene}
								{onbeantwortet}
								{ongelesen}
							/>
						{/each}
					</ul>
				{/each}
			</section>
		{:else}
			<p class="muted">Nichts passt zu diesem Filter.</p>
		{/each}
	{/if}

	<!-- Etappe B3: die Fristen aller Projekte, zugeklappt unter der Liste. -->
	<AlleFristen />
{/if}

<style>
	.archiv-fristen {
		display: grid;
		gap: 0.5rem;
		margin: 0.6rem 0 1rem;
	}
	.archiv-fristen li {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem 1rem;
		justify-content: space-between;
		align-items: baseline;
	}
	.zahlen {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 0.6rem;
		margin-bottom: 1rem;
	}
	.zahl-kachel {
		display: flex;
		flex-direction: column;
		height: 100%;
		box-sizing: border-box;
		padding: 0.6rem 0.9rem;
		background: var(--card);
		border: 1px solid var(--line);
		border-top: 4px solid var(--line);
		border-radius: var(--radius);
		box-shadow: var(--shadow);
		color: inherit;
		text-decoration: none;
	}
	.zahl-kachel:hover span:last-child {
		text-decoration: underline;
	}
	.zahl-kachel.jetzt {
		border-top-color: var(--warn);
	}
	.zahl-kachel.dich {
		border-top-color: var(--accent);
	}
	.zahl {
		font-size: var(--text-4xl);
		font-weight: 700;
		line-height: 1.1;
	}
	.steuerung {
		display: flex;
		flex-wrap: wrap;
		gap: 0.6rem 1.2rem;
		align-items: center;
		margin-bottom: 0.4rem;
	}
	.schalter,
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
	}
	.chip {
		font: inherit;
		font-size: var(--text-md);
		padding: 0.3rem 0.85rem;
		min-height: 2.25rem;
		border-radius: 999px;
		border: 1px solid var(--line);
		background: var(--card);
		color: var(--text);
		cursor: pointer;
	}
	.chip:hover {
		background: var(--accent-soft);
	}
	.chip[aria-pressed='true'] {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--on-accent);
	}
	.gruppe {
		margin-top: 1.2rem;
	}
	.gruppe h3 {
		margin-top: 0.9rem;
		color: var(--muted);
	}
	.anzahl {
		font-size: var(--text-xs);
		font-weight: 600;
		color: var(--muted);
		margin-left: 0.3rem;
	}
	.liste {
		display: grid;
		gap: 0.5rem;
	}
	@media (max-width: 37.5rem) {
		/* Am Handy eine Spalte: Zahl und Wort nebeneinander, damit es kurz bleibt. */
		.zahlen {
			grid-template-columns: minmax(0, 1fr);
			gap: 0.35rem;
		}
		.zahl-kachel {
			flex-direction: row;
			align-items: baseline;
			gap: 0.6rem;
			padding: 0.35rem 0.8rem;
			border-top-width: 1px;
			border-left: 4px solid var(--line);
		}
		.zahl-kachel.jetzt {
			border-top-color: var(--line);
			border-left-color: var(--warn);
		}
		.zahl-kachel.dich {
			border-top-color: var(--line);
			border-left-color: var(--accent);
		}
		.zahl {
			font-size: var(--text-2xl);
		}
		.steuerung {
			flex-direction: column;
			align-items: stretch;
		}
	}
</style>
