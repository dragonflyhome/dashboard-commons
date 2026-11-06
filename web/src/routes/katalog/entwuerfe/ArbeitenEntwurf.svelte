<script lang="ts">
	/**
	 * Nicht gewählte Entwürfe für „Arbeiten“ (Etappe 3a-3): „Drei Spalten“
	 * (kleines Kanban: läuft, wartet, als Nächstes nebeneinander) und
	 * „Zähler-Kachel“ (nur die drei Zahlen). Nur zum Ansehen im Katalog.
	 */
	import type { ArbeitsGruppeAnsicht } from '$katalog/widgets/typen.ts';

	interface Props {
		form: 'spalten' | 'zaehler';
		gruppen: readonly ArbeitsGruppeAnsicht[];
	}
	let { form, gruppen }: Props = $props();
</script>

<div class="widget card">
	<h2>Arbeiten</h2>
	{#if form === 'spalten'}
		<div class="spalten">
			{#each gruppen as g (g.wort)}
				<div class="spalte">
					<h3>{g.wort}</h3>
					<ul class="plain widget-liste small">
						{#each g.eintraege as e (e.id)}<li>{e.titel}</li>{/each}
					</ul>
				</div>
			{/each}
		</div>
	{:else}
		<ul class="plain zaehler">
			{#each gruppen as g (g.wort)}
				<li><span class="zahl">{g.eintraege.length + g.weitere}</span> {g.wort}</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.spalten {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 0.5rem;
	}
	.spalte {
		min-width: 0;
		padding: 0.3rem;
		border-radius: 0.4rem;
		background: var(--plan-soft);
	}
	.zaehler {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
	}
	.zahl {
		display: block;
		font-size: var(--text-4xl);
		font-weight: 700;
		line-height: 1;
	}
</style>
