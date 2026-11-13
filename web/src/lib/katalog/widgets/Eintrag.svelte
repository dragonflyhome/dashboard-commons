<script lang="ts">
	/**
	 * Ein Eintrag zum Aufklappen (Frage, Arbeit, Aufgabe) in den Widgets. Die
	 * id ist der Anker `<art>-<id>`: Kommt man über `#arbeit-ar-1`, klappt
	 * ansteuern() (#lib/ziel.ts) ihn und alle umgebenden `<details>` auf.
	 * Darum hier `<details>` und nicht der Aufklapper aus Bits UI.
	 */
	import type { Snippet } from 'svelte';
	import type { AnstehendEintrag } from '$vertrag';
	import { ART_ZEICHEN, anker, kurzDatum, zustandWort } from '#lib/anstehend.ts';
	import { chatWort } from '#lib/aufgaben.ts';
	import Schritte from './Schritte.svelte';
	import { fristText, schritteKurz, wannSatz, wartetAufWort } from './worte.ts';

	interface Props {
		eintrag: AnstehendEintrag;
		/** Am Anfang offen (die aktuelle Frage). */
		offen?: boolean;
		/** Inhalt vor den Einzelheiten (etwa die Antwort auf eine Frage). */
		children?: Snippet;
	}
	let { eintrag: e, offen = false, children }: Props = $props();

	const zustand = $derived(zustandWort(e));
	const wartet = $derived(wartetAufWort(e.wartet_auf));
	const frist = $derived(fristText(e));
	const schritte = $derived(e.schritte ?? []);
	// Als Konstante: ein Leerzeichen am Ende eines Elements schneidet Svelte ab.
	const TRENNER = ', ';
</script>

<details class="eintrag" id={anker(e)} open={offen}>
	<!-- Bei Arbeiten kein ▸: das Dreieck des Aufklappers steht schon davor (T5, Hinweis aus T4). -->
	<summary
		>{#if e.art !== 'arbeit'}<span class="zeichen" aria-hidden="true">{ART_ZEICHEN[e.art]}</span
			>{/if}
		<span class="titel">{e.titel}</span>{#if schritte.length > 0}<span class="trenner"
				>{TRENNER}</span
			><span class="kurz small muted">{schritteKurz(schritte)}</span>{/if}</summary
	>
	{@render children?.()}
	{#if schritte.length > 0}<Schritte
			{schritte}
			titel={e.titel}
			offeneVerweise={e.offene_verweise ?? []}
		/>{/if}
	<ul class="plain einzel small">
		<!-- Mit Schritten sagt der wartende Schritt schon, worauf gewartet wird (keine doppelte Ansage). -->
		{#if zustand}<li>
				Stand: {zustand}{#if wartet && e.art === 'arbeit' && schritte.length === 0}, {wartet}{/if}
			</li>{/if}
		{#if e.vermerk}<li>{e.vermerk}</li>{/if}
		{#if frist}<li>Frist: {frist}</li>{/if}
		<!-- Etappe 3b (Fund des Prüfers): ein ganzer Satz statt „Stufe: Bald (Vorschlag des Chats)“. -->
		<li>{wannSatz(e)}</li>
		{#if e.gestellt}<li>gestellt am {kurzDatum(e.gestellt)}</li>{/if}
		{#if e.von}<li class="muted">von {chatWort(e.von)}</li>{/if}
	</ul>
</details>

<style>
	.eintrag {
		border-radius: 0.4rem;
	}
	summary {
		cursor: pointer;
	}
	/* Nur für Vorleseprogramme, aber im Fluss und unsichtbar: .nur-vorlesen ist absolut und stünde in innerText als eigene Zeile „,“. */
	.trenner {
		opacity: 0;
	}
	.kurz {
		display: block;
		font-weight: 400;
		margin-left: 1.2rem;
	}
	.zeichen {
		color: var(--muted);
		margin-right: 0.2rem;
	}
	.einzel {
		margin: 0.25rem 0 0.4rem 1.2rem;
		display: grid;
		gap: 0.1rem;
		font-weight: 400;
	}
</style>
