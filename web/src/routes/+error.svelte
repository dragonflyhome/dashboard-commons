<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import {
		SATZ_ANMELDUNG_NOETIG,
		SATZ_FREMDES_KONTO,
		anmeldeWeg,
		localhostAdresse,
		type AnmeldeWeg
	} from '#lib/anmeldung.ts';
	import { statusSatz } from '#lib/api/client.ts';
	import OldLink from '#lib/components/OldLink.svelte';
	import { seitenTitel } from '#lib/titel.svelte.ts';

	const art = $derived(page.error?.art);
	// Dieselbe Seite unter localhost (Port bleibt); dort meldet Windows von selbst an.
	const ziel = $derived(localhostAdresse(page.url.href));
	/** Schritt E2: unter Windows der Satz zu Edge, sonst der neutrale (`/api/anmeldung/wer`). */
	let weg = $state<AnmeldeWeg | null>(null);
	onMount(() => {
		if (art === 'anmeldung') void anmeldeWeg().then((w) => (weg = w));
	});
	// Etappe 7c: zur Anmeldung mit Passkey (immer über localhost), danach zurück hierher.
	const passkeyZiel = $derived(
		localhostAdresse(
			new URL(
				`/anmelden?ziel=${encodeURIComponent(page.url.pathname + page.url.search)}`,
				page.url.href
			).href
		)
	);

	/**
	 * Etappe 3b (Fund des Prüfers): Der Satz zum Fehler ist der deutsche Satz
	 * des Servers (er hat immer einen), sonst einer zum Code. Die englischen
	 * Sätze von SvelteKit („Not Found“, „Internal Error“) zeigt die Seite nie,
	 * auch keinen Code, keine Pfade und keine Befehle.
	 */
	const ENGLISCH = /^(not found|internal error|error)$/i;
	const meldung = $derived((page.error?.message ?? '').trim());
	const eigenerSatz = $derived(meldung !== '' && !ENGLISCH.test(meldung) ? meldung : null);
	/** Tab-Titel: kurz, ohne Code. */
	const titel = $derived(
		art === 'anmeldung' || art === 'passkey'
			? 'Anmeldung nötig'
			: art === 'fremdes-konto'
				? 'Kein Zugang'
				: page.status === 404
					? 'Nicht gefunden'
					: 'Fehler'
	);
</script>

<svelte:head>
	<title>{seitenTitel(titel)}</title>
</svelte:head>

{#if art === 'anmeldung'}
	<section class="card">
		<h1>Anmeldung nötig</h1>
		<p class="big">{SATZ_ANMELDUNG_NOETIG}</p>
		{#if weg === 'windows'}
			<p>
				Mit deiner Windows-Anmeldung öffnet sich diese Seite in Microsoft Edge unter
				<OldLink href={ziel}>{localhostAdresse(page.url.origin)}</OldLink>. Andere Browser schicken
				deine Windows-Anmeldung nicht mit.
			</p>
			<p>
				<label class="muted" for="anmeldung-adresse"
					>Adresse zum Kopieren (anklicken, dann Strg+C):</label
				>
				<input
					id="anmeldung-adresse"
					class="adresse"
					type="text"
					readonly
					value={ziel}
					onfocus={(e) => {
						e.currentTarget.select();
					}}
				/>
			</p>
		{:else if weg === 'passkey'}
			<p>Anmelden geht hier mit einem Passkey.</p>
		{/if}
		<p><OldLink class="button primary" href={passkeyZiel}>Zur Anmeldung</OldLink></p>
	</section>
{:else if art === 'passkey'}
	<section class="card">
		<h1>Mit Passkey anmelden</h1>
		<p class="big">
			Dieses Windows-Konto ist nicht dein Hauptkonto. Mit einem freigegebenen Passkey geht es
			trotzdem.
		</p>
		<p><OldLink class="button primary" href={passkeyZiel}>Zur Anmeldung mit Passkey</OldLink></p>
	</section>
{:else if art === 'fremdes-konto'}
	<section class="card">
		<h1>Kein Zugang</h1>
		<p class="big">{SATZ_FREMDES_KONTO}</p>
		<p class="muted">
			Diese Seite ist nur für dein Windows-Konto. Mit einem freigegebenen Passkey geht es auch hier.
		</p>
		<p><OldLink class="button" href={passkeyZiel}>Zur Anmeldung</OldLink></p>
	</section>
{:else if page.status === 404}
	<section class="card">
		<h1>Diese Seite gibt es nicht</h1>
		<p class="big">
			{eigenerSatz ?? 'Vielleicht hat sich die Adresse geändert, oder das Lesezeichen ist alt.'}
		</p>
		<p><a class="button primary" href={resolve('/')}>Zur Übersicht</a></p>
	</section>
{:else}
	<section class="card">
		<h1>Das hat nicht geklappt</h1>
		<p class="big">{eigenerSatz ?? statusSatz(page.status)}</p>
		<p class="muted">
			Bitte gleich noch einmal versuchen. Hilft das nicht, weiß der Betreuer des Dashboards Rat.
		</p>
		<p><a class="button" href={resolve('/')}>Zur Übersicht</a></p>
	</section>
{/if}

<style>
	.adresse {
		display: block;
		width: 100%;
		max-width: 40rem;
		margin-top: 0.25rem;
		font: inherit;
		font-family: var(--mono);
	}
</style>
