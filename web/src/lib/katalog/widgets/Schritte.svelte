<script lang="ts">
	/**
	 * Die Schritte einer Arbeit (Zweig teilschritte, T4; f4 „Wie du es
	 * siehst“): eine nummerierte Liste mit Beschriftung, je Schritt Titel,
	 * Stand als Wort, wer dran ist, worauf er wartet und seit wann. Ein
	 * Stillstand über einen Tag steht als Satz da; wartet ein Schritt auf
	 * dich, führt ein Link zu der Frage oder Aufgabe, solange der Server sie
	 * als offen meldet (sonst gäbe es den Anker nicht). Keine Knöpfe, keine
	 * Fortschrittsleiste: die Zahl steht schon in der Zeile darüber. Farbe
	 * (Rand links) nur zusätzlich zum Wort.
	 */
	import type { Schritt } from '$vertrag';
	import {
		SATZ_STILLSTAND,
		SCHRITT_BEDEUTUNG,
		schrittStand,
		schrittWer,
		stillstand,
		verweisZiel,
		zeitText
	} from './worte.ts';

	interface Props {
		schritte: readonly Schritt[];
		/** Titel der Arbeit, für die Beschriftung der Liste. */
		titel: string;
		/**
		 * Die Verweise, die der Server als offene Frage oder Aufgabe kennt
		 * (`offene_verweise`); jeder auf dich wartende Schritt mit einem davon bekommt den Link.
		 */
		offeneVerweise?: readonly string[];
		/** Für Tests; sonst eine Uhr, die jede Minute weiterläuft. */
		jetzt?: number;
	}
	let { schritte, titel, offeneVerweise = [], jetzt }: Props = $props();

	// Minutentakt, damit der 24-h-Satz auch auf einer offen gelassenen Seite erscheint.
	let uhr = $state(Date.now());
	$effect(() => {
		const takt = setInterval(() => {
			uhr = Date.now();
		}, 60_000);
		return () => {
			clearInterval(takt);
		};
	});
	const nun = $derived(jetzt ?? uhr);
</script>

<ol class="plain schritte" aria-label="Schritte von „{titel}“">
	{#each schritte as s (s.nr)}
		{@const angaben = [schrittStand(s), schrittWer(s), zeitText(s)]
			.filter((t) => t !== null)
			.join(' · ')}
		{@const ziel =
			s.stand === 'wartet' &&
			s.wartet_auf === 'inhaber' &&
			s.verweis !== null &&
			offeneVerweise.includes(s.verweis)
				? verweisZiel(s.verweis)
				: null}
		<li class="b-{SCHRITT_BEDEUTUNG[s.stand]}">
			<span class="nr">{s.nr}.</span>
			<span class="titel">{s.titel}</span>
			<span class="angaben">{angaben}</span>
			{#if ziel}<a href="#{ziel.anker}">{ziel.wort}</a>{/if}
			{#if stillstand(s, nun)}<span class="still">{SATZ_STILLSTAND}</span>{/if}
		</li>
	{/each}
</ol>

<style>
	.schritte {
		display: grid;
		gap: 0.3rem;
		margin: 0.3rem 0 0.2rem 1.2rem;
		font-size: var(--text-sm);
	}
	li {
		padding-left: 0.4rem;
		border-left: 0.2rem solid var(--ton, var(--line));
		overflow-wrap: anywhere;
	}
	.nr,
	.angaben,
	.still {
		color: var(--muted);
	}
	.angaben,
	.still {
		display: block;
	}
	.still {
		font-style: italic;
	}
</style>
