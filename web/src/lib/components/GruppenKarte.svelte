<script lang="ts">
	/**
	 * Karte einer Projektgruppe auf der Startseite (Entscheidung
	 * projektgruppe): Titel der Gruppe mit dem Status des
	 * dringenderen Teils, darunter je Teil eine Zeile (Status, Titel,
	 * Zusammenfassung). Die Karte führt zur Gruppenseite; die Teile zu ihren
	 * eigenen Seiten stehen dort.
	 */
	import { resolve } from '$app/paths';
	import type { Bedeutung, GruppeAnzeige, ProjectSummary } from '$vertrag';
	import StatusDot from './StatusDot.svelte';

	interface Props {
		gruppe: GruppeAnzeige;
		teile: ProjectSummary[];
		bedeutung: Bedeutung;
		status: string;
	}
	let { gruppe, teile, bedeutung, status }: Props = $props();
</script>

<a class="tile card" href={resolve('/gruppen/[slug]', { slug: gruppe.slug })}>
	<h2><StatusDot {bedeutung} label={status} /> {gruppe.titel}</h2>
	<p class="meta muted small">
		{status} · {teile.length === 1 ? 'ein Teil' : `${String(teile.length)} Teile`}
	</p>
	<ul class="plain teile">
		{#each teile as t (t.slug)}
			<li>
				<StatusDot bedeutung={t.bedeutung} label={t.status} />
				<span
					><b>{t.title}</b>{#if t.summary}<span class="muted"> · {t.summary}</span>{/if}</span
				>
			</li>
		{/each}
	</ul>
</a>

<style>
	.tile {
		display: block;
		color: var(--text);
		transition:
			transform 160ms ease,
			box-shadow 160ms ease,
			border-color 160ms ease;
	}
	.tile:hover,
	.tile:focus-visible {
		text-decoration: none;
		transform: translateY(-2px);
		border-color: var(--accent);
		box-shadow: var(--shadow-hover);
	}
	h2 {
		font-size: var(--text-lg);
		display: flex;
		align-items: baseline;
	}
	.meta {
		margin-top: -4px;
	}
	.teile {
		display: grid;
		gap: 4px;
		margin-top: 8px;
	}
	.teile li {
		display: flex;
		align-items: baseline;
		gap: 2px;
		min-width: 0;
		overflow-wrap: anywhere;
	}
</style>
