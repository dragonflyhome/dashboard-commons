<script lang="ts">
	/**
	 * Widget „Zuletzt“ (Etappe 3a-3, A4; Karte „Aktualisierungen“ aus A3),
	 * gewählter Entwurf „Liste mit Neu-Marken“: die letzten Meldungen und
	 * Vorgänge (höchstens fünf, wie der Server sie liefert), ungelesene fett
	 * mit „neu“; Meldungen klappen auf (Anker `#meldung-<id>`) und haben mit
	 * Anmeldung den Knopf „gelesen“; Vorgänge sind ein Link auf ihr Ziel.
	 * Ungelesene Meldungen außerhalb der fünf stehen unter „und N weitere
	 * ungelesene“, damit jeder Link aus dem Band hier ankommt. Ohne Anmeldung
	 * nur die Zahlen.
	 *
	 * Warum diese Form: Eine Zeitleiste mit Uhrzeit-Spalte braucht Breite und
	 * trennt Zeit und Sache; eine Zähler-Kachel zeigt nicht, was passiert ist.
	 * Hier steht die Sache vorn, Art und Zeit leise dahinter, und „gelesen“
	 * sitzt dort, wo man liest.
	 */
	import type { Aktualisierung, AnstehendEintrag, MeldungsZahlen } from '$vertrag';
	import { SATZ_OHNE_ANMELDUNG } from '#lib/anstehend.ts';
	import { kurzZeit } from '#lib/unterdashboard.ts';
	import { zielHref } from '#lib/zieladresse.ts';
	import Meldung from './Meldung.svelte';
	import { VORGANG_WORT } from './worte.ts';

	interface Props {
		aktualisierungen: readonly Aktualisierung[];
		/** Ungelesene Meldungen, die nicht unter den fünf stehen. */
		weitere?: readonly AnstehendEintrag[];
		/** Ohne Anmeldung: nur die Zahlen aus dem Stand. */
		zahlen?: MeldungsZahlen | null;
		ungelesen: number;
		angemeldet: boolean;
		/** Id der Meldung, deren „gelesen“ gerade läuft. */
		laeuft?: string | null;
		fehler?: string | null;
		ongelesen?: ((id: string, knopf: HTMLButtonElement) => void) | undefined;
		/** Die Überschrift (Fokusziel nach „gelesen“). */
		titel?: HTMLElement | null;
	}
	let {
		aktualisierungen,
		weitere = [],
		zahlen = null,
		ungelesen,
		angemeldet,
		laeuft = null,
		fehler = null,
		ongelesen,
		titel = $bindable(null)
	}: Props = $props();
</script>

<div class="karte widget card zuletzt">
	<h2 tabindex="-1" bind:this={titel}>
		Aktualisierungen
		{#if ungelesen > 0}<span class="widget-anzahl">{ungelesen} neu</span>{/if}
	</h2>
	{#if zahlen !== null && zahlen.gesamt > 0}
		<p>
			{zahlen.gesamt === 1 ? 'Eine Meldung' : `${String(zahlen.gesamt)} Meldungen`}{zahlen.neu > 0
				? `, davon ${String(zahlen.neu)} neu.`
				: '.'}
		</p>
		<p class="muted small">{SATZ_OHNE_ANMELDUNG}</p>
	{/if}
	{#if aktualisierungen.length > 0}
		<ul class="plain widget-liste liste">
			{#each aktualisierungen as a (`${a.quelle}-${a.id}`)}
				{@const href = zielHref(a.ziel)}
				<li class:ungelesen={!a.gelesen}>
					{#if a.quelle === 'meldung'}
						<Meldung meldung={a} knopf={angemeldet} gesperrt={laeuft !== null} {ongelesen} />
					{:else}
						<span class="widget-zeichen" aria-hidden="true">↻</span>
						{#if href !== null}
							<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- zielHref baut mit resolve() -->
							<a {href}>{a.titel}</a>
						{:else}
							<span>{a.titel}</span>
						{/if}
						<span class="muted small"> · {VORGANG_WORT[a.art] ?? a.art}, {kurzZeit(a.zeit)}</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	{#if weitere.length > 0}
		<details class="widget-mehr mehr">
			<summary class="small">und {weitere.length} weitere ungelesene</summary>
			<ul class="plain widget-liste">
				{#each weitere as e (e.id)}
					<li class="ungelesen">
						<Meldung
							meldung={{
								id: e.id,
								art: e.meldung?.art ?? 'info',
								titel: e.titel,
								zeit: e.meldung?.zuletzt ?? e.erstellt,
								anzahl: e.meldung?.anzahl ?? 1,
								behoben: false,
								gelesen: false,
								text: e.meldung?.text ?? null
							}}
							knopf={angemeldet}
							gesperrt={laeuft !== null}
							{ongelesen}
						/>
					</li>
				{/each}
			</ul>
		</details>
	{/if}
	{#if fehler}
		<p class="block-error small" role="alert">{fehler}</p>
	{/if}
</div>

<style>
	.ungelesen {
		font-weight: 600;
	}
	h2:focus {
		outline: 0.125rem solid var(--accent);
		outline-offset: 0.125rem;
	}
</style>
