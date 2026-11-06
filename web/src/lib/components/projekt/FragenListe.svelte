<script lang="ts">
	/**
	 * Fragen an den Inhaber aus `daten/fragen.toml`, nur zum Lesen. Beantworten geht
	 * seit Etappe A2 oben im Überblick der Projektseite und in „Anstehend“.
	 *
	 * Etappe B2 (Fragen-Warteschlange): Mit `warteschlange` vom Server (aus dem
	 * Abgeleiteten von fragen.toml) steht nur die eine sichtbare Frage da, wie
	 * in Anstehend und im Überblick, dazu „noch N Fragen warten“ ohne deren
	 * Text. Ohne `warteschlange` (Beispiele, alte Daten) alle Fragen der Datei.
	 */
	import type { Frage, FrageArt, FragenAuswahl } from '$vertrag';
	import { fmtDate } from '#lib/format.ts';
	import { weitereFragenSatz } from '#lib/anstehend.ts';

	interface Props {
		fragen: Frage[];
		/** Die Warteschlange des Projekts vom Server; ohne: alle Fragen der Datei. */
		warteschlange?: FragenAuswahl | null | undefined;
	}
	let { fragen: alle, warteschlange = null }: Props = $props();

	const fragen = $derived(
		warteschlange === null
			? alle
			: alle.filter((f) => f.id === warteschlange.sichtbar && f.gestellt === warteschlange.gestellt)
	);
	const weitere = $derived(warteschlange?.weitere ?? 0);

	function artWort(a: FrageArt): string {
		switch (a) {
			case 'wahl':
				return 'Auswahl';
			case 'ja-nein':
				return 'Ja oder Nein';
			case 'text':
				return 'freie Antwort';
			case 'bestaetigen':
				return 'bestätigen';
		}
	}
</script>

{#if fragen.length === 0 && weitere > 0}
	<!-- Die sichtbare Frage steht in einem anderen Teil der Datei. -->
	<p class="muted small weitere">{weitereFragenSatz(weitere)}.</p>
{:else if fragen.length === 0}
	<!-- data-leer: Steht im Abschnitt sonst nichts, fehlt die ganze Karte (Abschnitt.svelte). -->
	<p class="muted" data-leer>{warteschlange === null ? 'Keine Fragen.' : 'Keine offene Frage.'}</p>
{:else}
	<ol class="fragen">
		{#each fragen as f (f.id)}
			<li>
				<p class="text"><b>{f.text}</b></p>
				<p class="muted small">{artWort(f.art)} · gestellt am {fmtDate(f.gestellt)}</p>
				{#if f.optionen.length > 0}
					<ul class="optionen">
						{#each f.optionen as o (o)}
							<li>{o}</li>
						{/each}
					</ul>
				{/if}
				{#if f.empfehlung}
					<p class="small">Empfehlung: {f.empfehlung}</p>
				{/if}
			</li>
		{/each}
	</ol>
	{#if weitere > 0}
		<p class="muted small weitere">Dahinter {weitereFragenSatz(weitere)}.</p>
	{/if}
	<p class="muted small">
		{warteschlange === null
			? 'Beantworten kannst du die Fragen oben im Überblick oder in „Anstehend“.'
			: 'Beantworten kannst du sie oben im Überblick oder in „Anstehend“; danach rückt die nächste nach.'}
	</p>
{/if}

<style>
	.fragen {
		margin: 0.6em 0;
		padding-left: 1.4em;
		display: grid;
		gap: 10px;
	}
	.fragen p {
		margin: 0;
	}
	.optionen {
		margin: 4px 0;
		padding-left: 1.2em;
	}
</style>
