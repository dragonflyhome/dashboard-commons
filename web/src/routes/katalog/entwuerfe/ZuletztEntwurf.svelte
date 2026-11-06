<script lang="ts">
	/**
	 * Nicht gewählte Entwürfe für „Zuletzt“ (Etappe 3a-3): „Zeitleiste“
	 * (Uhrzeit links, Punkt und Sache rechts) und „Zähler-Kachel“ (nur „N
	 * neu“ und die letzte Zeit). Nur zum Ansehen im Katalog.
	 */
	import type { Aktualisierung } from '$vertrag';
	import { kurzZeit } from '#lib/unterdashboard.ts';
	import { meldungsZahlen } from '#lib/anstehend.ts';

	interface Props {
		form: 'zeitleiste' | 'zaehler';
		aktualisierungen: readonly Aktualisierung[];
	}
	let { form, aktualisierungen }: Props = $props();
	const neu = $derived(meldungsZahlen(aktualisierungen).neu);
</script>

<div class="widget card">
	<h2>Zuletzt</h2>
	{#if form === 'zeitleiste'}
		<ol class="plain leiste">
			{#each aktualisierungen as a (`${a.quelle}-${a.id}`)}
				<li>
					<span class="zeit muted small">{kurzZeit(a.zeit)}</span>
					<span class="punkt" class:b-problem={!a.gelesen} aria-hidden="true"></span>
					<span>{a.titel}</span>
				</li>
			{/each}
		</ol>
	{:else}
		<p class="zahl">{neu} neu</p>
		{#if aktualisierungen[0]}<p class="muted small">
				zuletzt {kurzZeit(aktualisierungen[0].zeit)}
			</p>{/if}
	{/if}
</div>

<style>
	.leiste {
		display: grid;
		gap: 0.4rem;
	}
	.leiste li {
		display: grid;
		grid-template-columns: 6.5rem 0.75rem minmax(0, 1fr);
		align-items: baseline;
		gap: 0.4rem;
	}
	.punkt {
		width: 0.6rem;
		height: 0.6rem;
		border-radius: 50%;
		background: var(--ton, var(--plan));
	}
	.zahl {
		font-size: var(--text-4xl);
		font-weight: 700;
		margin: 0.2rem 0;
	}
</style>
