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
	import BauKopf from '#lib/components/projekt/BauKopf.svelte';
	import Fehlerbild from '#lib/components/projekt/Fehlerbild.svelte';
	import AnmeldungAbgelaufen from '#lib/components/projekt/AnmeldungAbgelaufen.svelte';
	import AbschnittVerzeichnis from '#lib/components/projekt/AbschnittVerzeichnis.svelte';
	import { aufgaben } from '#lib/aufgaben.svelte.ts';
	import { eingabeDa, fokusZeigen, teilFehlt } from '#lib/ladefehler.ts';
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
	 * Navigation und der Rest des Dashboards laufen weiter. Ist auf der Seite
	 * etwas eingegeben (`eingabeDa`), bleibt die Seite stehen, die Meldung haftet
	 * hinter ihr am unteren Rand (`ueber`), und „schließen“ schließt sie nur, ohne
	 * die Seite neu zu zeichnen. Ebenso der Hinweis `AnmeldungAbgelaufen`: hinter
	 * der Seite, damit Tab vom Textfeld aus hinkommt; der Fokus bleibt, wo er ist.
	 */
	let fensterFehler = $state.raw<{ fehler: unknown; ueber: boolean } | null>(null);
	/** Anmeldung abgelaufen (oder fremdes Konto) bei offener Eingabe: Die Seite bleibt, dazu der Hinweis. */
	let anmeldung = $state<'abgelaufen' | 'fremd' | null>(null);
	/** Teile und Inhalt der Seite: daraus liest das Inhaltsverzeichnis die Abschnitte. */
	let seiteEl = $state<HTMLElement | null>(null);
	/** „neu versuchen“: die Seite noch einmal zeichnen. */
	let runde = $state(0);
	$effect(() => {
		if (eingebettet) return;
		// Gemeldet wird selbst (Konsole); preventDefault: der Fehler ist hier behandelt.
		// Fehlt ein Teil des Baus: abgemeldet zur Anmeldung, alter Bau einmal neu laden
		// (`teilFehlt`, Schritt E2b); abgemeldet mit offener Eingabe: die Seite
		// stehen lassen, dahinter der Hinweis; sonst das Fehlerbild.
		// Ein spätes Ergebnis nach einem Seitenwechsel gilt nicht mehr (wie in Grenze und Bauliste).
		const zeigen = (f: unknown): void => {
			const fuer = gezeigt;
			void teilFehlt(f).then((wie) => {
				if (wie === 'weiter' || fuer !== gezeigt) return;
				if (wie === 'abgelaufen' || wie === 'fremd') {
					anmeldung = wie;
					void fokusZeigen();
					return;
				}
				console.error('Fehler auf der Projektseite:', f);
				fensterFehler = { fehler: f, ueber: eingabeDa(document) };
				void fokusZeigen();
			});
		};
		const beiFehler = (e: ErrorEvent): void => {
			e.preventDefault();
			zeigen(e.error ?? e.message);
		};
		const beiAbgelehnt = (e: PromiseRejectionEvent): void => {
			e.preventDefault();
			zeigen(e.reason);
		};
		window.addEventListener('error', beiFehler);
		window.addEventListener('unhandledrejection', beiAbgelehnt);
		return () => {
			window.removeEventListener('error', beiFehler);
			window.removeEventListener('unhandledrejection', beiAbgelehnt);
		};
	});
	function teileFehler(e: unknown): void {
		console.error('Fehler in den Teilen der Projektseite:', e);
	}
	// Eine andere Seite des Projekts beginnt ohne den alten Fehler.
	let gezeigt = '';
	$effect.pre(() => {
		const jetzt = `${slug}/${aktiv ?? ''}`;
		if (jetzt !== gezeigt) {
			gezeigt = jetzt;
			fensterFehler = null;
			anmeldung = null;
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
		<!-- Wirft ein Teil trotz der Prüfung an der Grenze (projektteile.ts), bleibt der Inhalt darunter sichtbar.
		     Je Projekt neu: Ein Fehler bei einem Projekt gilt nicht für das nächste. -->
		{#key slug}
			<svelte:boundary onerror={teileFehler}>
				<ProjektTeile {slug} {archiviert} />

				<!-- eslint-disable-next-line @typescript-eslint/no-unused-vars -- der Fehler steht schon im Protokoll (onerror) -->
				{#snippet failed(_fehler: unknown, neu: () => void)}
					<p class="muted" role="status">
						Stand, Aufgabenliste, Plan und Zeitleiste lassen sich mit dem laufenden Dienst gerade
						nicht zeigen.
					</p>
					<button type="button" class="pill" onclick={neu}>neu versuchen</button>
				{/snippet}
			</svelte:boundary>
		{/key}
	{:else}
		<!-- Auf Unterseiten steht der Stand-Kopf nicht; ein roter oder laufender Bau bleibt sichtbar. -->
		<BauKopf {slug} nurAuffaellig />
	{/if}

	<div class="inhalt">
		{#if fensterFehler !== null && !fensterFehler.ueber}
			<Fehlerbild
				satz="Bei einer Bedienung der Seite ist etwas schiefgegangen; der Chat des Projekts kann es beheben."
				fehler={fensterFehler.fehler}
				datei={null}
				neu={() => {
					runde += 1;
					fensterFehler = null;
				}}
			/>
		{:else}
			{#key runde}
				{@render children()}
			{/key}
		{/if}
		<!-- Hinter der Seite (Tab-Reihenfolge nach den Eingaben), beide in einem Behälter am unteren Rand haftend. -->
		{#if anmeldung !== null || fensterFehler?.ueber}
			<div class="haftend stapel">
				{#if anmeldung !== null}
					<AnmeldungAbgelaufen
						fremd={anmeldung === 'fremd'}
						satz="Die Seite bleibt mit allem Eingegebenen stehen. Bitte in einem neuen Tab anmelden und die Bedienung danach noch einmal versuchen."
						knopf="Hinweis schließen"
						neu={() => {
							anmeldung = null;
						}}
					/>
				{/if}
				{#if fensterFehler?.ueber}
					<Fehlerbild
						satz="Bei einer Bedienung der Seite ist etwas schiefgegangen; die Seite bleibt mit allem Eingegebenen stehen."
						fehler={fensterFehler.fehler}
						datei={null}
						knopf="schließen"
						neu={() => {
							fensterFehler = null;
						}}
					/>
				{/if}
			</div>
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
	/* Hinweis und Fehlerbild bei offener Eingabe: untereinander statt übereinander
	   in einem haftenden Behälter (.haftend in app.css). */
	.stapel {
		display: flex;
		flex-direction: column;
		gap: 8px;
		/* Den Abstand trägt gap; der margin-top beider Karten fällt im Stapel weg. */
		--karte-oben: 0;
	}
	/* Kopfleiste, Fassung 3: Am Rechner (ab 1024 px) stehen die Unterseiten in
	   der Seitenleiste unter dem Projekt; die Chips bleiben nur darunter. */
	@media (min-width: 64rem) {
		.seiten:not(.eingebettet) {
			display: none;
		}
	}
</style>
