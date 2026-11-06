<script lang="ts">
	/**
	 * Nicht gewählte Entwürfe für „Fristen“ (Etappe 3a-3): „Nächste Frist
	 * groß“ (bisherige Karte: ein Datum, ein Link) und „Kalenderblatt“ (Tag
	 * und Monat als Blatt). Nur zum Ansehen im Katalog.
	 */
	import type { AnstehendEintrag } from '$vertrag';
	import { fristWorte, kurzDatum } from '#lib/anstehend.ts';
	import { nachFrist } from '$katalog/widgets/worte.ts';

	interface Props {
		form: 'gross' | 'kalender';
		eintraege: readonly AnstehendEintrag[];
	}
	let { form, eintraege }: Props = $props();
	const erste = $derived(nachFrist(eintraege)[0] ?? null);
	const MONATE = [
		'Jan',
		'Feb',
		'Mär',
		'Apr',
		'Mai',
		'Jun',
		'Jul',
		'Aug',
		'Sep',
		'Okt',
		'Nov',
		'Dez'
	];
	const teile = $derived(erste?.frist?.split('-') ?? []);
</script>

{#if erste?.frist}
	{#if form === 'gross'}
		<div class="widget card">
			<h2>Nächste Frist</h2>
			<p class="gross">
				<b>{kurzDatum(erste.frist)}</b>{#if fristWorte(erste.tage_bis)}, {fristWorte(
						erste.tage_bis
					)}{/if}
			</p>
			<p class="small">{erste.titel}</p>
		</div>
	{:else}
		<div class="widget card kalender">
			<h2>Fristen</h2>
			<div class="blatt" aria-hidden="true">
				<span class="monat">{MONATE[Number(teile[1]) - 1]}</span>
				<span class="tag">{Number(teile[2])}</span>
			</div>
			<p class="small">{kurzDatum(erste.frist)}: {erste.titel}</p>
		</div>
	{/if}
{/if}

<style>
	.gross {
		font-size: var(--text-lg);
	}
	.blatt {
		display: inline-grid;
		width: 4.5rem;
		text-align: center;
		border: 1px solid var(--line);
		border-radius: 0.5rem;
		overflow: hidden;
		margin-bottom: 0.4rem;
	}
	.monat {
		background: var(--bad-soft);
		color: var(--bad-text);
		font-size: var(--text-xs);
		font-weight: 600;
	}
	.tag {
		font-size: var(--text-4xl);
		font-weight: 700;
	}
</style>
