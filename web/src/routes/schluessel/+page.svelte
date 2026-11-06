<script lang="ts">
	/**
	 * Schlüssel-Seite. Der Wert wird nie im Frontend gehalten: das Formular ist
	 * ein normales HTML-Formular, das direkt an den Rust-Server geht
	 * (POST /schluessel/<name>, Origin-Prüfung dort). Hier gibt es nur Status.
	 *
	 * Nach dem Speichern leitet der Server auf ?gespeichert=<name> um. Die Seite
	 * merkt sich den Namen beim Laden, rieselt einmal und nimmt den Parameter
	 * gleich wieder aus der Adresse (goto mit shallow und replace, ersetzt den
	 * Verlaufseintrag); ein Neuladen oder Zurück feiert also nicht noch einmal.
	 */
	import { untrack } from 'svelte';
	import { afterNavigate, goto } from '$app/navigation';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import Confetti from '#lib/components/Confetti.svelte';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import PasskeyVerwaltung from '#lib/components/passkey/PasskeyVerwaltung.svelte';
	import { fmtDateTime } from '#lib/format.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	// Einmal beim Laden gelesen, nicht abgeleitet: nach dem Entfernen des
	// Parameters bleibt die Bestätigung bis zum nächsten Aufruf stehen.
	const saved = untrack(() => page.url.searchParams.get('gespeichert'));
	const problem = $derived(page.url.searchParams.get('fehler'));
	const party = saved ? 1 : 0;
	// Etappe 3a+: nur was wirklich fehlt; „unbekannt“ (Bote meldet einen Fehler) zählt nicht als fehlt.
	const missing = $derived(data.keys.filter((k) => k.stand === 'fehlt').length);
	const unbekannt = $derived(data.keys.filter((k) => k.stand === 'unbekannt').length);

	// Erst nach dem Start des Routers (afterNavigate, nicht onMount): vorher
	// lehnt SvelteKit goto ab. Der Server leitet nur mit gespeichert oder nur
	// mit fehler um, nie mit beidem; also bleibt die blanke Adresse.
	// Genau einmal und nach der Adresse der Navigation (to.url), nicht nach
	// page.url: goto mit shallow ruft afterNavigate erneut, page.url behält
	// den Parameter dabei, und ohne Merker lief das endlos (Hotfix 3a-4).
	let bereinigt = false;
	afterNavigate(({ to }) => {
		if (bereinigt || !to?.url.searchParams.has('gespeichert')) return;
		bereinigt = true;
		void goto(resolve('/schluessel'), { shallow: true, replace: true });
	});
</script>

<svelte:head>
	<title>{seitenTitel('Schlüssel')}</title>
</svelte:head>

<!-- sofort: nach dem Speichern kommt die Seite schon mit ?gespeichert=… (trigger 1); danach ist er weg. -->
<Confetti trigger={party} sofort />

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Schlüssel</p>
<h1>Schlüssel und Passwörter</h1>
<p>
	Hier gibst du Passwörter ein, die die Chats brauchen. Der Wert geht direkt in den
	<b>Windows-Tresor</b> (Anmeldeinformationsverwaltung) auf diesem PC. Das Dashboard zeigt ihn danach
	nie wieder an, nur ob er da ist.
</p>
<p class="warn-box">
	⚠️ Nie hier eingeben: das Hauptpasswort deines Mail- oder Cloud-Kontos, Bankdaten, Ausweisnummern.
	Die gibst du nur direkt im jeweiligen Programm ein.
</p>
{#if saved}
	<p class="ok-box">✅ „{saved}“ ist gespeichert. Der Chat kann ihn jetzt benutzen.</p>
{/if}
{#if problem}<p class="block-error">{problem}</p>{/if}
{#if data.error}<p class="block-error">
		Die Liste der Schlüssel lässt sich nicht lesen: {data.error}
	</p>{/if}
{#if data.keys.length === 0 && !data.error}
	<p class="muted">Zurzeit braucht kein Chat einen Schlüssel.</p>
{:else}
	<p class="muted small">
		{data.keys.length} angefordert, {missing} fehl{missing === 1 ? 't' : 'en'}{#if unbekannt > 0},
			{unbekannt} unbekannt{/if}.
	</p>
{/if}

<section class="cards">
	{#each data.keys as k (k.name)}
		<article class="card key">
			<h2>{k.name}</h2>
			{#if k.saved_at}
				<p class="big"><StatusDot bedeutung="ok" /> gespeichert am {fmtDateTime(k.saved_at)}</p>
			{:else if k.stand === 'unbekannt'}
				<p class="big"><StatusDot bedeutung="neutral" /> Stand unbekannt</p>
			{:else}
				<p class="big"><StatusDot bedeutung="problem" /> fehlt</p>
			{/if}
			<p>{k.zweck}</p>
			{#if k.hinweis}<p class="muted">So kommst du dran: {k.hinweis}</p>{/if}
			{#if k.angefordert_von}
				<p class="muted small">
					Braucht der Chat „{k.angefordert_von}“ · im Tresor als <code>{k.target}</code>
				</p>
			{/if}
			{#if k.saved_at === null}
				{@render form(k.name)}
			{:else}
				<details>
					<summary>Neuen Wert eingeben (ersetzt den alten)</summary>
					{@render form(k.name)}
				</details>
			{/if}
		</article>
	{/each}
</section>

<!-- Etappe 7c: Passkeys für andere Windows-Konten (Anträge, Freigabe, Widerruf). -->
<PasskeyVerwaltung />

{#snippet form(name: string)}
	<!-- Natives Formular, absichtlich ohne SvelteKit-Eingriff: der Wert geht nur an den Rust-Server. -->
	<form
		class="key-form"
		method="post"
		action="/schluessel/{name}"
		autocomplete="off"
		data-sveltekit-reload
	>
		<label>
			Benutzername (freiwillig, landet nur im Tresor):
			<input
				type="text"
				name="benutzer"
				autocomplete="off"
				autocapitalize="off"
				spellcheck="false"
				maxlength="200"
			/>
		</label>
		<label>
			Wert einfügen:
			<input
				type="password"
				name="wert"
				autocomplete="off"
				autocapitalize="off"
				spellcheck="false"
				required
				maxlength="1200"
			/>
		</label>
		<button type="submit" class="button primary">Speichern</button>
	</form>
{/snippet}

<style>
	.warn-box {
		background: var(--bad-soft);
		border-left: 4px solid var(--bad);
		padding: 8px 12px;
		border-radius: 10px;
	}
	.ok-box {
		background: var(--ok-soft);
		border-left: 4px solid var(--ok);
		padding: 8px 12px;
		border-radius: 10px;
		animation: pop 400ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	@keyframes pop {
		from {
			transform: scale(0.96);
			opacity: 0;
		}
	}
	.key h2 {
		font-family: var(--mono);
		font-size: var(--text-base);
	}
	.key .big {
		display: flex;
		align-items: center;
	}
	.key-form {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		align-items: end;
		margin-top: 8px;
	}
	.key-form label {
		display: grid;
		gap: 4px;
		flex: 1 1 200px;
		font-size: var(--text-md);
	}
	.key-form input[type='password'],
	.key-form input[type='text'] {
		font: inherit;
		padding: 7px 10px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--bg);
		color: var(--text);
	}
</style>
