<script lang="ts">
	/**
	 * Zeitstrahl aus `daten/zeitstrahl.toml` als Verlauf (Etappe 3a-1):
	 * Kommendes über der Heute-Linie, Vergangenes darunter. Die Ereignisse
	 * rechnet der Server (`abgeleitet.ereignisse`, für alle Teile zusammen);
	 * je Teil zeigt der Baustein die Ereignisse seiner Einträge. Darüber, wenn
	 * der Server es gerechnet hat: erledigte Einträge und der nächste Termin.
	 */
	import type { Abgeleitet, Zeitstrahl } from '$vertrag';
	import Daten from './intern/Daten.svelte';
	import Verlauf from './zeit/Verlauf.svelte';
	import { ereignisseDesTeils } from '#lib/zeit/verlauf.ts';
	import { teile } from '#lib/daten.ts';
	import {
		erledigtSatz,
		istGeladen,
		liste,
		listenPfad,
		naechsterSatz,
		type Quelle
	} from './abgeleitet.ts';

	interface Props {
		daten: Quelle<Zeitstrahl>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
	}
	let { daten, teil, abgeleitet }: Props = $props();

	/** Die Einträge aller Teile: Eine Kennung wie schritt-2 gehört dem Teil, der sie trägt. */
	const alleEintraege = $derived.by(() => {
		const d = daten;
		const datei = istGeladen(d) ? d.inhalt : d;
		return datei === null ? [] : teile(datei).flatMap((t) => t.inhalt.eintrag);
	});

	function saetze(z: Zeitstrahl, name: string | null, abg: Abgeleitet | null): string[] {
		const l = liste(abg, listenPfad('eintrag', name));
		return [erledigtSatz(l), naechsterSatz(l, 'datum', (i) => z.eintrag[i]?.titel)].filter(
			(s): s is string => s !== null
		);
	}
</script>

<Daten {daten} {teil} {abgeleitet}>
	{#snippet children(z: Zeitstrahl, name: string | null, abg: Abgeleitet | null)}
		{@const s = saetze(z, name, abg)}
		{#if s.length > 0}
			<p class="muted small zusatz">{s.join(' · ')}</p>
		{/if}
		{#if abg?.ereignisse === undefined}
			<p class="muted small">
				Der Zeitstrahl erscheint, sobald der Server die Datei gelesen hat (über ladeDaten).
			</p>
		{:else}
			<!-- Steht der Name des Teils als h3 darüber (intern/Daten.svelte), sind Jahre und
			     „Kommt noch“ eine Ebene tiefer, h4 (Auflage aus 3a-1). -->
			<Verlauf
				ereignisse={ereignisseDesTeils(abg.ereignisse, z.eintrag, alleEintraege)}
				modus="projekt"
				ebene={teil === undefined && name !== null ? 4 : 3}
			/>
		{/if}
	{/snippet}
</Daten>

<style>
	.zusatz {
		margin: 0.4em 0 0;
	}
</style>
