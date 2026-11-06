<script lang="ts">
	/**
	 * Widget „Arbeiten“ (Etappe 3a-3, A4), gewählter Entwurf „Liste mit
	 * Gruppen“: läuft, wartet, als Nächstes, je mit Zahl; je Gruppe höchstens
	 * zwei Einträge zum Aufklappen, der Rest unter „und N weitere“. Ohne
	 * Anmeldung kommen die Gruppen aus dem Stand (höchstens zwei, dazu die
	 * Zahl der übrigen). Leere Gruppen fallen weg.
	 *
	 * Warum diese Form: Drei Spalten nebeneinander (kleines Kanban) brechen
	 * am Handy um und zeigen dann lange Titel in schmalen Spalten; nur Zahlen
	 * (Zähler-Kachel) sagen nicht, woran gearbeitet wird. Die Gruppenliste
	 * liest sich in einer Spalte von oben nach unten, und jeder Eintrag hat
	 * seinen Anker `#arbeit-…`.
	 */
	import { teilen } from '#lib/unterdashboard.ts';
	import Eintrag from './Eintrag.svelte';
	import type { ArbeitsGruppeAnsicht } from './typen.ts';

	interface Props {
		gruppen: readonly ArbeitsGruppeAnsicht[];
	}
	let { gruppen }: Props = $props();

	const sichtbar = $derived(gruppen.filter((g) => g.eintraege.length > 0 || g.weitere > 0));
</script>

<div class="karte widget card arbeiten">
	<h2>Arbeiten</h2>
	{#each sichtbar as g (g.wort)}
		{@const t = teilen([...g.eintraege], 2)}
		<h3>{g.wort} <span class="widget-anzahl">{g.eintraege.length + g.weitere}</span></h3>
		<ul class="plain widget-liste">
			{#each t.vorn as e (e.id)}
				<li><Eintrag eintrag={e} /></li>
			{/each}
		</ul>
		{#if t.rest.length > 0}
			<details class="widget-mehr mehr">
				<summary class="small">und {t.rest.length} weitere</summary>
				<ul class="plain widget-liste">
					{#each t.rest as e (e.id)}
						<li><Eintrag eintrag={e} /></li>
					{/each}
				</ul>
			</details>
		{:else if g.weitere > 0}
			<p class="muted small">und {g.weitere} weitere</p>
		{/if}
	{/each}
</div>
