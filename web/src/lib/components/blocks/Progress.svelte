<script lang="ts">
	import type { ProgressRow } from '$vertrag';
	import { ausFortschrittStufe, klasse } from '#lib/bedeutung.ts';
	import type { FortschrittBalken } from '#lib/daten.ts';
	import { fortschrittText } from '#lib/textfassung.ts';
	import Textfassung from '../Textfassung.svelte';

	interface Props {
		/** Als Anzeige-Baustein (`ProgressRow`) oder aus daten/fortschritt.toml. */
		rows: (ProgressRow | FortschrittBalken)[];
		/** Textfassung gleich sichtbar (Katalog). */
		textOffen?: boolean;
	}
	let { rows, textOffen = false }: Props = $props();
</script>

<div class="progress">
	{#each rows as r (r.label)}
		<div class="meter">
			<div class="head"><span>{r.label}</span><span class="muted small">{r.note}</span></div>
			<div
				class="bar"
				role="progressbar"
				aria-valuemin="0"
				aria-valuemax="100"
				aria-valuenow={Math.round(r.percent)}
				aria-label={r.label}
			>
				<div class="fill {klasse(ausFortschrittStufe(r.stufe))}" style:width="{r.percent}%"></div>
			</div>
			<div class="muted small">{r.text}</div>
		</div>
	{/each}
</div>
<!-- Etappe 3b: Textfassung mit denselben Prozenten. -->
<Textfassung fassung={fortschrittText(rows)} offen={textOffen} />

<style>
	.progress {
		display: grid;
		gap: 10px;
		margin: 0.6em 0 1em;
	}
	.head {
		display: flex;
		justify-content: space-between;
		gap: 10px;
	}
	.bar {
		height: 12px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--line) 70%, transparent);
		overflow: hidden;
	}
	.fill {
		height: 100%;
		border-radius: 999px;
		/* Ton aus der Klasse der Bedeutung (app.css, bedeutung.ts). */
		background: var(--ton, var(--ok));
		transition: width 700ms cubic-bezier(0.2, 0.8, 0.2, 1);
		animation: grow 700ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	@keyframes grow {
		from {
			width: 0;
		}
	}
</style>
