<script lang="ts">
	/**
	 * Nicht gewählte Entwürfe für „Frage“ (Etappe 3a-3): „Karte mit der
	 * einen Frage“ (nur die Frage groß, Antwort-Knöpfe, „noch N warten“) und
	 * „Kachel mit Zähler“ (große Zahl, erster Titel). Die Knöpfe senden hier
	 * nichts. Nur zum Ansehen im Katalog.
	 */
	import type { AnstehendEintrag } from '$vertrag';

	interface Props {
		form: 'einzeln' | 'zaehler';
		fragen: readonly AnstehendEintrag[];
		zahl: number;
	}
	let { form, fragen, zahl }: Props = $props();
	const erste = $derived(fragen[0] ?? null);
	let gewaehlt = $state<string | null>(null);
</script>

{#if form === 'einzeln'}
	<div class="widget card">
		<h2>Frage</h2>
		{#if erste}
			<p class="gross">{erste.titel}</p>
			<div class="knoepfe" role="group" aria-label="Antwort (Beispiel, sendet nichts)">
				{#each ['Ja', 'Nein'] as w (w)}
					<button
						type="button"
						class="button"
						aria-pressed={gewaehlt === w}
						onclick={() => {
							gewaehlt = w;
						}}>{w}</button
					>
				{/each}
			</div>
			{#if fragen.length > 1}<p class="muted small">noch {fragen.length - 1} warten</p>{/if}
		{/if}
	</div>
{:else}
	<div class="widget card b-wartet">
		<h2>Wartet auf dich</h2>
		<p class="zahl">{zahl}</p>
		{#if erste}<p class="small">zuerst: {erste.titel}</p>{/if}
	</div>
{/if}

<style>
	.gross {
		font-size: var(--text-lg);
		font-weight: 600;
	}
	.knoepfe {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-bottom: 0.4rem;
	}
	.zahl {
		font-size: var(--text-5xl);
		font-weight: 700;
		line-height: 1;
		margin: 0.2rem 0 0.4rem;
	}
</style>
