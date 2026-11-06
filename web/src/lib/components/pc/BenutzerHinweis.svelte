<script lang="ts">
	/**
	 * Hinweis statt (oder über) Benutzer-Daten der PC-Seite: ohne
	 * Windows-Anmeldung mit Link auf dieselbe Seite unter localhost,
	 * ohne Boten mit dem letzten Stand.
	 */
	import { page } from '$app/state';
	import { localhostAdresse } from '#lib/anmeldung.ts';
	import OldLink from '#lib/components/OldLink.svelte';
	import { benutzerHinweis, type Benutzerdaten } from '#lib/pc.ts';

	interface Props {
		stand: Benutzerdaten;
		letzterStand?: string | null;
	}
	let { stand, letzterStand = null }: Props = $props();

	const text = $derived(benutzerHinweis(stand, letzterStand));
</script>

{#if text}
	<p class="hinweis">
		{text}
		{#if stand === 'anmeldung-noetig'}
			<OldLink href={localhostAdresse(page.url.href)}>Über localhost öffnen</OldLink>
		{/if}
	</p>
{/if}

<style>
	.hinweis {
		padding: 10px 14px;
		border-radius: 10px;
		background: color-mix(in srgb, var(--line) 45%, transparent);
	}
</style>
