<script lang="ts">
	/**
	 * Rahmen eines Projekts (Etappe 4v): Kopf aus projekt.toml, auf der
	 * Übersicht die vier Teile Stand, Aufgabenliste, Plan und Zeitleiste
	 * (Etappe B8, `ProjektTeile`), Unterseiten. Setzt den Projekt-Kontext für `$projekt`. Steht
	 * unter `/projects/<slug>` (Unterseiten `/projects/<slug>/<name>`).
	 *
	 * Eingebettet als Teil der Gruppenseite (Entscheidung
	 * projektgruppe): ohne Tab-Titel und Überschrift (die trägt
	 * der Kopf des Teils), die Unterseiten immer als Chips. Fehler beim
	 * Zeichnen fängt die Grenze dieses Teils; Fehler aus Klick-Funktionen
	 * lassen sich am Fenster keinem Teil zuordnen und bleiben dort der
	 * Konsole (die anderen Teile bleiben so unberührt).
	 */
	import type { Snippet } from 'svelte';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { resolve } from '$app/paths';
	import { Marke } from '$katalog';
	import { ausStatus } from '#lib/bedeutung.ts';
	import { setzeProjekt } from '#lib/projekt/laden.svelte.ts';
	import ProjektTeile from '#lib/components/projekt/ProjektTeile.svelte';
	import Fehlerbild from '#lib/components/projekt/Fehlerbild.svelte';
	import AbschnittVerzeichnis from '#lib/components/projekt/AbschnittVerzeichnis.svelte';
	import { aufgaben } from '#lib/aufgaben.svelte.ts';
	import { istChunkFehler } from '#lib/fassung.ts';
	import { fassung } from '#lib/fassung.svelte.ts';
	import type { ProjektAntwort } from '$vertrag';

	interface Props {
		kopf: ProjektAntwort;
		/** Die gezeigte Unterseite (`null`: Übersicht). */
		aktiv: string | null;
		/** Als Teil der Gruppenseite. */
		eingebettet?: boolean;
		children: Snippet;
	}
	let { kopf, aktiv, eingebettet = false, children }: Props = $props();

	setzeProjekt(() => kopf.slug);

	const k = $derived(kopf.kopf);
	const slug = $derived(kopf.slug);
	const seiten = $derived([...k.seiten].sort((a, b) => a.order - b.order));

	// Der Kasten der Aufgaben (in der Aufgabenliste) zeigt die offenen Aufgaben: laden, solange er da ist.
	$effect(() => aufgaben.benutzen());
	// Entscheidung archiv-status: Die Aufgaben eines archivierten Projekts
	// stehen nicht in der allgemeinen Liste; seine Seite holt sie selbst.
	const archiviert = $derived(k.status === 'archiviert');
	$effect(() => {
		if (archiviert) void aufgaben.ladenFuer(slug);
	});

	/**
	 * Etappe 4b: Fehler in Klick-Funktionen der Projektseite fängt die
	 * Fehlergrenze nicht (so die Svelte-Doku). Der Rahmen hört darum am Fenster
	 * auf `error` und `unhandledrejection` (auch eine async-Funktion, die
	 * scheitert) und zeigt dieselbe Meldung an Stelle der Seite; Kopf,
	 * Navigation und der Rest des Dashboards laufen weiter.
	 */
	let fensterFehler = $state.raw<{ fehler: unknown } | null>(null);
	/** Teile und Inhalt der Seite: daraus liest das Inhaltsverzeichnis die Abschnitte. */
	let seiteEl = $state<HTMLElement | null>(null);
	/** „neu versuchen“: die Seite noch einmal zeichnen. */
	let runde = $state(0);
	$effect(() => {
		if (eingebettet) return;
		// Gemeldet wird selbst (Konsole); preventDefault: der Fehler ist hier behandelt.
		const beiFehler = (e: ErrorEvent): void => {
			e.preventDefault();
			// Fehlt nur ein Teil eines alten Baus: einmal neu laden statt des Fehlerbilds.
			if (istChunkFehler(e.error ?? e.message) && fassung.chunkFehler()) return;
			console.error('Fehler auf der Projektseite:', e.error ?? e.message);
			fensterFehler = { fehler: e.error ?? e.message };
		};
		const beiAbgelehnt = (e: PromiseRejectionEvent): void => {
			e.preventDefault();
			if (istChunkFehler(e.reason) && fassung.chunkFehler()) return;
			console.error('Fehler auf der Projektseite:', e.reason);
			fensterFehler = { fehler: e.reason };
		};
		window.addEventListener('error', beiFehler);
		window.addEventListener('unhandledrejection', beiAbgelehnt);
		return () => {
			window.removeEventListener('error', beiFehler);
			window.removeEventListener('unhandledrejection', beiAbgelehnt);
		};
	});
	// Eine andere Seite des Projekts beginnt ohne den alten Fehler.
	let gezeigt = '';
	$effect.pre(() => {
		const jetzt = `${slug}/${aktiv ?? ''}`;
		if (jetzt !== gezeigt) {
			gezeigt = jetzt;
			fensterFehler = null;
		}
	});
</script>

<svelte:head>
	{#if !eingebettet}
		<title>{seitenTitel(k.title)}</title>
	{/if}
</svelte:head>

<!-- Kopfleiste, Fassung 3: Den Weg (Projekte › Projekt › Unterseite) tragen die
     Brotkrumen oben; die frühere Zeile „Übersicht › Projekt“ hier entfällt (Abnahme). -->
{#if !eingebettet}
	{#if archiviert}
		<p class="archiviert muted">Dieses Projekt ist archiviert.</p>
	{/if}
	<!-- Etappe B8: das Status-Wort einmal, in der Überschrift. -->
	<h1>{k.title} <Marke bedeutung={ausStatus(k.status)} wort={k.status} /></h1>
{/if}
<!-- Nur Absicht: worum es geht (kein Stand; den rechnet der Teil „Stand“). -->
{#if k.summary}
	<p class="lead">{k.summary}</p>
{/if}
{#if !eingebettet && k.chat}
	<p class="muted small">Verwaltet von Chat „{k.chat}“</p>
{/if}

{#if seiten.length > 0}
	<nav class="seiten" class:eingebettet aria-label="Seiten des Projekts">
		<a
			class="pill"
			href={resolve('/projects/[slug]', { slug })}
			aria-current={aktiv === null ? 'page' : undefined}>Übersicht</a
		>
		{#each seiten as s (s.name)}
			<a
				class="pill"
				href={resolve('/projects/[slug]/[name]', { slug, name: s.name })}
				aria-current={aktiv === s.name ? 'page' : undefined}>{s.title}</a
			>
		{/each}
	</nav>
{/if}

<!-- Lange Seiten bekommen ein Inhaltsverzeichnis; auf der Übersicht stehen die vier Teile immer darin. -->
{#if !eingebettet}
	<AbschnittVerzeichnis ziel={seiteEl} ab={aktiv === null ? 4 : 6} />
{/if}

<div class="seite" bind:this={seiteEl}>
	{#if aktiv === null}
		<ProjektTeile {slug} {archiviert} />
	{/if}

	<div class="inhalt">
		{#if fensterFehler !== null}
			<Fehlerbild
				satz="Bei einer Bedienung der Seite ist etwas schiefgegangen; der Chat des Projekts kann es beheben."
				fehler={fensterFehler.fehler}
				datei={null}
				neu={() => {
					fensterFehler = null;
					runde += 1;
				}}
			/>
		{:else}
			{#key runde}
				{@render children()}
			{/key}
		{/if}
	</div>
</div>

<style>
	h1 {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.25rem 0.6rem;
		overflow-wrap: anywhere;
	}
	.lead {
		font-size: var(--text-lg);
	}
	.seiten {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin: 8px 0 4px;
	}
	.seiten a {
		text-decoration: none;
		border: 1px solid transparent;
	}
	.seiten a[aria-current='page'] {
		background: var(--accent-soft);
		border-color: var(--accent);
		/* --accent hat auf --accent-soft hell nur 4,16:1; --accent-text 5,39:1 (kontrast.spec.ts). */
		color: var(--accent-text);
		font-weight: 600;
	}
	.seite,
	.inhalt {
		min-width: 0;
	}
	/* Kopfleiste, Fassung 3: Am Rechner (ab 1024 px) stehen die Unterseiten in
	   der Seitenleiste unter dem Projekt; die Chips bleiben nur darunter. */
	@media (min-width: 64rem) {
		.seiten:not(.eingebettet) {
			display: none;
		}
	}
</style>
