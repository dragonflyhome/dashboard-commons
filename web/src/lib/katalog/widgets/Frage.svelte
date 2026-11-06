<script lang="ts">
	/**
	 * Widget „Frage“ (Etappe 3a-3, A4), gewählter Entwurf „Liste mit
	 * Gruppen“: Karte „Wartet auf dich“ mit allem, was der Inhaber tun kann, in
	 * dieser Reihenfolge: die eine aktuelle Frage offen zum Beantworten,
	 * weitere Fragen zugeklappt unter „noch N warten“; dann die Aufgaben
	 * (höchstens drei als Link auf ihren Anker im Kasten „Aufgaben für dich“,
	 * dann „und N weitere“); dann Arbeiten, die auf den Inhaber warten. Ohne
	 * Anmeldung nur der Satz mit den Zahlen des Servers.
	 *
	 * Warum diese Form: Eine Frage auf einmal (Konzept), aber nichts geht
	 * verloren: jeder Anker `#frage-…` findet seinen Eintrag, auch zugeklappt
	 * (ansteuern() klappt auf). Die Gruppen ersparen eine eigene Karte je Art;
	 * am Handy bleibt es eine Spalte.
	 *
	 * Welche Frage zuerst kommt, ist die Reihenfolge der Liste (heute vom
	 * Server, ab 3a+ aus der Oberfläche). Die Antwort selbst zeigt `antwort`
	 * (im Unter-Dashboard FrageAntwort; im Katalog eine Attrappe ohne Senden).
	 */
	import type { Snippet } from 'svelte';
	import type { AnstehendEintrag } from '$vertrag';
	import { ART_ZEICHEN, SATZ_OHNE_ANMELDUNG, anker } from '#lib/anstehend.ts';
	import { teilen } from '#lib/unterdashboard.ts';
	import Eintrag from './Eintrag.svelte';
	import { fristText } from './worte.ts';

	interface Props {
		/** Wie viele Einträge auf den Inhaber warten (vom Server oder gezählt). */
		zahl: number;
		fragen?: readonly AnstehendEintrag[];
		aufgaben?: readonly AnstehendEintrag[];
		/** Arbeiten, die auf den Inhaber warten. */
		arbeiten?: readonly AnstehendEintrag[];
		/** Ohne Anmeldung: Zahl der Aufgaben aus dem Stand (für den Satz). */
		aufgabenZahl?: number;
		angemeldet?: boolean;
		/** Antwort auf eine Frage; ohne nur die Einzelheiten. */
		antwort?: Snippet<[AnstehendEintrag]> | undefined;
	}
	let {
		zahl,
		fragen = [],
		aufgaben = [],
		arbeiten = [],
		aufgabenZahl = 0,
		angemeldet = true,
		antwort
	}: Props = $props();

	const erste = $derived(fragen[0] ?? null);
	const weitereFragen = $derived(fragen.slice(1));
	const aufg = $derived(teilen([...aufgaben], 3));
	const hatListe = $derived(fragen.length > 0 || aufgaben.length > 0 || arbeiten.length > 0);
	const gruppen = $derived(
		[fragen.length > 0, aufgaben.length > 0, arbeiten.length > 0].filter(Boolean).length
	);
</script>

{#snippet frageMitAntwort(e: AnstehendEintrag, offen: boolean)}
	<Eintrag eintrag={e} {offen}>
		{@render antwort?.(e)}
	</Eintrag>
{/snippet}

<div class="karte widget card frage">
	<h2>Wartet auf dich <span class="widget-anzahl">{zahl}</span></h2>
	{#if hatListe}
		{#if erste}
			{#if gruppen > 1}<h3>{fragen.length === 1 ? 'Frage' : 'Fragen'}</h3>{/if}
			<ul class="plain widget-liste">
				<li>{@render frageMitAntwort(erste, true)}</li>
			</ul>
			{#if weitereFragen.length > 0}
				<details class="widget-mehr">
					<summary class="small"
						>noch {weitereFragen.length}
						{weitereFragen.length === 1 ? 'Frage wartet' : 'Fragen warten'}</summary
					>
					<ul class="plain widget-liste">
						{#each weitereFragen as e (e.id)}
							<li>{@render frageMitAntwort(e, false)}</li>
						{/each}
					</ul>
				</details>
			{/if}
		{/if}
		{#if aufgaben.length > 0}
			{#if gruppen > 1}<h3>{aufgaben.length === 1 ? 'Aufgabe' : 'Aufgaben'}</h3>{/if}
			<ul class="plain widget-liste">
				{#each aufg.vorn as e (e.id)}
					<li>
						<a href="#{anker(e)}"
							><span class="widget-zeichen" aria-hidden="true">{ART_ZEICHEN.aufgabe}</span>
							{e.titel}</a
						>
						{#if e.frist}<span class="muted small"> · {fristText(e)}</span>{/if}
					</li>
				{/each}
				{#if aufg.rest.length > 0}
					<li class="small">
						<a href="#aufgaben-fuer-dich">und {aufg.rest.length} weitere Aufgaben</a>
					</li>
				{/if}
			</ul>
		{/if}
		{#if arbeiten.length > 0}
			{#if gruppen > 1}<h3>{arbeiten.length === 1 ? 'Arbeit' : 'Arbeiten'}</h3>{/if}
			<ul class="plain widget-liste">
				{#each arbeiten as e (e.id)}
					<li>
						<a href="#{anker(e)}"
							><span class="widget-zeichen" aria-hidden="true">{ART_ZEICHEN.arbeit}</span>
							{e.titel}</a
						>
					</li>
				{/each}
			</ul>
		{/if}
	{:else}
		<p class="small">
			{zahl === 1
				? 'Ein Eintrag wartet auf dich'
				: `${String(zahl)} Einträge warten auf dich`}{aufgabenZahl > 0
				? `, davon ${aufgabenZahl === 1 ? 'eine Aufgabe' : `${String(aufgabenZahl)} Aufgaben`}.`
				: '.'}
		</p>
		{#if !angemeldet}
			<p class="muted small">{SATZ_OHNE_ANMELDUNG}</p>
		{/if}
	{/if}
</div>
