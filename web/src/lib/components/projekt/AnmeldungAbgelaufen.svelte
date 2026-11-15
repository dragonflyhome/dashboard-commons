<script lang="ts">
	/**
	 * Schritt E2b: Die Anmeldung ist abgelaufen, während auf der Seite etwas
	 * Ungesichertes offen ist (`teilFehlt` gibt „abgelaufen“, bei einem fremden
	 * Windows-Konto „fremd“). Statt wegzunavigieren (der Text ginge verloren) ein
	 * Hinweis mit der Anmeldung in einem neuen Tab; danach `neu`.
	 *
	 * Der Hinweis haftet am unteren Rand des Fensters (sticky), solange sein
	 * Bereich zu sehen ist: Wer weiter unten tippt, sieht ihn. Er ist höchstens
	 * 40vh hoch, und `scroll-padding-bottom` hält das Feld mit dem Fokus über
	 * ihm (nicht verdeckt, WCAG 2.4.11; Klasse `haftend` in app.css). Er nimmt nie den
	 * Fokus; `role="alert"` sagt ihn an, und der Aufrufer stellt ihn hinter die
	 * Eingaben, damit Tab vom Textfeld aus zu Link und Knopf kommt.
	 * Der Knopf fragt die Anmeldung neu (`zugang.auffrischen`), bevor `neu` läuft.
	 */
	import { resolve } from '$app/paths';
	import { SATZ_FREMDES_KONTO } from '#lib/anmeldung.ts';
	import { zugang } from '#lib/zugang.ts';

	interface Props {
		/** Was mit der Seite ist und was nach der Anmeldung zu tun ist, ein Satz. */
		satz: string;
		/** Beschriftung des Knopfs, etwa „neu versuchen“. */
		knopf: string;
		neu: () => void;
		/** Ein fremdes Windows-Konto ist angemeldet (`teilFehlt` gibt „fremd“). */
		fremd?: boolean;
	}
	let { satz, knopf, neu, fremd = false }: Props = $props();
</script>

<section class="card abgelaufen haftend" role="alert">
	<h2>{fremd ? 'Anmeldung mit diesem Konto nicht möglich' : 'Anmeldung abgelaufen'}</h2>
	{#if fremd}
		<p>{SATZ_FREMDES_KONTO}</p>
	{/if}
	<p>{satz}</p>
	<p class="wege">
		<a class="button primary" href={resolve('/anmelden')} target="_blank" rel="noopener"
			>In neuem Tab anmelden</a
		>
		<button
			type="button"
			class="button"
			onclick={() => {
				void zugang.auffrischen();
				neu();
			}}>{knopf}</button
		>
	</p>
</section>

<style>
	/* Haften und Höhe: .haftend in app.css. */
	.abgelaufen {
		/* Im haftenden Stapel (NeueFormRahmen) 0, dort trägt gap den Abstand. */
		margin-top: var(--karte-oben, 16px);
		border-left: 4px solid var(--bad);
	}
	.abgelaufen h2 {
		margin-top: 0;
	}
	.wege {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
</style>
