<script lang="ts">
	/**
	 * Vorschau eines Website-Entwurfs: eine HTML-Datei aus projects/<slug>/assets/
	 * in einem abgeschirmten Rahmen. sandbox ohne allow-same-origin: der Entwurf
	 * sieht weder das Dashboard noch dessen Daten; der Rust-Server gibt die Datei
	 * zusätzlich mit einer Content-Security-Policy ohne Netzzugriff aus.
	 */
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { resolve } from '$app/paths';
	import OldLink from '#lib/components/OldLink.svelte';
	import { assetStand, mitVersion } from '#lib/assets.svelte.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// Live-Lücken: Ändert sich der Entwurf in assets/, bekommt die Adresse eine neue
	// Version (`?v=`); der Rahmen lädt ihn dann neu, ohne dass die Seite neu lädt.
	const adresse = $derived(mitVersion(data.vorschau, assetStand.version(data.slug)));

	let stage = $state<HTMLElement | null>(null);
	let full = $state(false);

	async function toggleFull(): Promise<void> {
		if (document.fullscreenElement !== null) {
			await document.exitFullscreen();
			return;
		}
		await stage?.requestFullscreen();
	}

	$effect(() => {
		const sync = (): void => {
			full = document.fullscreenElement !== null;
		};
		document.addEventListener('fullscreenchange', sync);
		return () => {
			document.removeEventListener('fullscreenchange', sync);
		};
	});
</script>

<svelte:head>
	<title>{seitenTitel(`Vorschau: ${data.title}`)}</title>
</svelte:head>

<p class="crumbs muted small">
	<a href={resolve('/')}>Übersicht</a> ›
	<a href={resolve('/projects/[slug]', { slug: data.slug })}>{data.title}</a> › Vorschau
</p>

<div class="bar">
	<h1>Vorschau: {data.title}</h1>
	<div class="actions">
		<button type="button" class="button primary" onclick={() => void toggleFull()}>
			{full ? 'Vollbild verlassen' : '⛶ Vollbild'}
		</button>
		<OldLink href={adresse} class="button" external>In neuem Tab öffnen</OldLink>
	</div>
</div>
<p class="muted small">
	Das ist ein Entwurf, kein Server im Netz. Er läuft abgeschirmt: keine Verbindung nach außen, kein
	Zugriff auf das Dashboard. Im Vollbild beendet <kbd>Esc</kbd> die Ansicht.
</p>

<div class="stage" class:full bind:this={stage}>
	<iframe
		src={adresse}
		title="Entwurf: {data.title}"
		sandbox="allow-scripts"
		referrerpolicy="no-referrer"
		loading="eager"
	></iframe>
</div>

<style>
	.bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
	}
	.bar h1 {
		margin: 0;
	}
	.actions {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}
	.stage {
		margin-top: 12px;
		border: 1px solid var(--line);
		border-radius: 14px;
		overflow: hidden;
		background: var(--papier);
		height: min(78vh, 1100px);
		box-shadow: var(--shadow-gross);
	}
	.stage.full {
		border: 0;
		border-radius: 0;
		height: 100vh;
	}
	iframe {
		display: block;
		width: 100%;
		height: 100%;
		border: 0;
	}
</style>
