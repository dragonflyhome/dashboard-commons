<script lang="ts">
	/**
	 * Nicht gewählte Entwürfe für „Stand“ (Etappe 3a-3): „Kompakte Karte“
	 * (bisherige Karte: Ampel und Satz, ohne Zahlen) und „Eine Zeile“ (alles
	 * in einer Zeile, ohne Karte). Nur zum Ansehen im Katalog.
	 */
	import type { StandAnsicht } from '#lib/anstehend.ts';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { klasse } from '#lib/bedeutung.ts';
	import { kurzDatum } from '#lib/anstehend.ts';
	import { AMPEL } from '$katalog/widgets/worte.ts';

	interface Props {
		form: 'kompakt' | 'zeile';
		stand: StandAnsicht;
	}
	let { form, stand }: Props = $props();
	const a = $derived(AMPEL[stand.gesamt.ampel]);
</script>

{#if form === 'kompakt'}
	<div class="widget card {klasse(a.bedeutung)}">
		<h2>Stand</h2>
		<p class="grund">
			<StatusDot bedeutung={a.bedeutung} label="Ampel {a.wort}" />
			{stand.gesamt.grund}
		</p>
		<p class="muted small">{stand.gesamt.status} · {stand.schwerpunkt}</p>
	</div>
{:else}
	<p class="zeile {klasse(a.bedeutung)}">
		<StatusDot bedeutung={a.bedeutung} label="Ampel {a.wort}" />
		<b>{stand.gesamt.grund}</b>
		<span>{stand.kurz.laeuft} läuft</span>
		{#if stand.kurz.naechste_frist}<span>Frist {kurzDatum(stand.kurz.naechste_frist)}</span>{/if}
	</p>
{/if}

<style>
	.grund {
		font-weight: 600;
		margin-bottom: 0.3rem;
	}
	.zeile {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.2rem 0.75rem;
		margin: 0;
		padding: 0.5rem 0.75rem;
		border-left: 0.25rem solid var(--ton);
		border-radius: 0.4rem;
		background: var(--card);
		width: 100%;
	}
</style>
