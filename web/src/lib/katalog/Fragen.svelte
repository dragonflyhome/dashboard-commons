<script lang="ts">
	/**
	 * Fragen an den Inhaber aus `daten/fragen.toml`, nur zum Lesen (beantworten
	 * geht im Überblick). Seit Etappe B2 nur die eine sichtbare Frage der
	 * Warteschlange (`abgeleitet.warteschlange` vom Server) und wie viele
	 * dahinter warten.
	 */
	import type { Abgeleitet, Fragen } from '$vertrag';
	import FragenListe from '#lib/components/projekt/FragenListe.svelte';
	import Daten from './intern/Daten.svelte';
	import { istGeladen, type Quelle } from './abgeleitet.ts';
	import { BROWSER_ADRESSE } from '#lib/anmeldung.ts';

	interface Props {
		daten: Quelle<Fragen>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
	}
	let { daten, teil, abgeleitet }: Props = $props();

	/** Die Warteschlange vom Server (aus `abgeleitet` oder aus `ladeDaten`); ohne: alle Fragen. */
	const warteschlange = $derived(
		(abgeleitet ?? (istGeladen(daten) ? daten.abgeleitet : null))?.warteschlange ?? null
	);
</script>

<!-- Etappe B2: Fragen an dich sind persönlich; ohne Anmeldung kommt nur dieser Satz. -->
<Daten
	{daten}
	{teil}
	{abgeleitet}
	anmeldenSatz="Anmelden, um die Fragen an dich zu sehen: in Microsoft Edge unter {BROWSER_ADRESSE}."
>
	{#snippet children(f: Fragen)}
		<FragenListe fragen={f.frage} {warteschlange} />
	{/snippet}
</Daten>
