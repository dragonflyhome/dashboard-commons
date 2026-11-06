<script lang="ts">
	/**
	 * Die Glocke oben rechts (Kopfleiste, Fassung 3; ersetzt das Band mit den
	 * Meldungen). Die Zahl zählt ungelesene Meldungen aus derselben Auswertung
	 * wie „Anstehend“ (`glocke` in anstehend.ts, keine zweite Zählung);
	 * behobene zählen nie mit, sie stehen nur in der Liste unter „behoben“.
	 * Rechte- und Zugriffsmeldungen (etwa „Ordner nicht lesbar“) sind Hinweise
	 * ohne Zahl. Meldungen sind geschützt: Ohne Anmeldung keine Zahl, nur der
	 * Hinweis zur Anmeldung.
	 *
	 * Die Liste ist ein Klappfeld aus dem Katalog (Bits UI Popover) ohne
	 * Seitensperre. Live: Die Liste lädt mit „Anstehend“ neu (`change` mit
	 * Bereich `anstehend`, `speicher` mit Bereich `meldungen`); neue Meldungen
	 * sagt eine höfliche Ansage-Region an, nie die beim ersten Laden.
	 *
	 * `speicher` nur für Tests; sonst der gemeinsame Speicher.
	 */
	import { resolve } from '$app/paths';
	import { Klappfeld } from '$katalog';
	import { anstehend, type Anstehend } from '#lib/anstehend.svelte.ts';
	import { MELDUNG_WORT, SATZ_OHNE_ANMELDUNG, meldungsOrt } from '#lib/anstehend.ts';
	import { SATZ_FREMDES_KONTO } from '#lib/anmeldung.ts';
	import { zielHref } from '#lib/zieladresse.ts';
	import type { AnstehendEintrag, Meldung } from '$vertrag';

	interface Props {
		speicher?: Anstehend;
		/** Offen von außen: nur für Tests. */
		offen?: boolean;
	}
	let { speicher = anstehend, offen = $bindable(false) }: Props = $props();

	const g = $derived(speicher.stand === 'da' ? speicher.glocke : null);
	const zahl = $derived(g?.zahl ?? 0);
	const behoben = $derived(speicher.stand === 'da' ? speicher.behoben : []);
	const name = $derived(
		g === null
			? 'Meldungen: erst nach der Anmeldung'
			: zahl === 0
				? 'Meldungen: keine ungelesenen'
				: `Meldungen: ${String(zahl)} ungelesen`
	);
	const hinweis = $derived(
		speicher.stand === 'fremdes-konto'
			? SATZ_FREMDES_KONTO
			: speicher.stand === 'laedt'
				? 'Die Meldungen werden geladen.'
				: speicher.stand === 'fehler'
					? (speicher.fehler ?? 'Die Meldungen lassen sich gerade nicht laden.')
					: SATZ_OHNE_ANMELDUNG
	);

	// Neue Meldungen höflich ansagen (wie bisher das Band), nie die beim ersten Laden.
	let bekannt: Record<string, true> | null = null;
	let ansage = $state('');
	$effect(() => {
		if (g === null) return;
		const alle = g.meldungen;
		const schluessel = (e: AnstehendEintrag): string => `${e.id}:${String(e.meldung?.anzahl ?? 1)}`;
		if (bekannt === null) {
			bekannt = Object.fromEntries(alle.map((e) => [schluessel(e), true as const]));
			return;
		}
		const gesehen = bekannt;
		const neu = alle.filter((e) => !(schluessel(e) in gesehen));
		for (const e of alle) gesehen[schluessel(e)] = true;
		const erste = neu[0];
		if (erste === undefined) return;
		ansage =
			neu.length === 1
				? `Neue Meldung: ${erste.titel}`
				: `${String(neu.length)} neue Meldungen, die neueste: ${erste.titel}`;
	});

	function behobenZiel(m: Meldung): string | null {
		return m.system ? zielHref('/pc') : zielHref(`/projects/${m.projekt}`);
	}

	let fehler = $state<string | null>(null);
	async function gelesen(id: string): Promise<void> {
		const gut = await speicher.gelesen({ id });
		fehler = gut ? null : speicher.gelesenFehler;
	}
</script>

<div class="nur-vorlesen" aria-live="polite" aria-atomic="true">{ansage}</div>

<Klappfeld
	{name}
	titel="Meldungen"
	bind:offen
	onoffen={(o) => {
		if (o && speicher.stand === 'da') void speicher.meldungenLaden();
	}}
>
	{#snippet knopf()}
		<span class="glocke" data-zahl={g === null ? 'ohne' : String(zahl)}>
			<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
				<path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z" />
				<path d="M10 20.5a2 2 0 0 0 4 0" />
			</svg>
			{#if g !== null && zahl > 0}<span class="zahl" aria-hidden="true">{zahl}</span>{/if}
		</span>
	{/snippet}
	{#snippet children(schliessen)}
		{#if g === null}
			<p class="hinweis small">{hinweis}</p>
			{#if speicher.stand === 'ohne-anmeldung'}
				<p class="small">
					<a href={resolve('/anmelden')} onclick={schliessen}>Mit Passkey anmelden</a>
				</p>
			{/if}
		{:else}
			{#if g.meldungen.length === 0}
				<p class="muted small">Keine ungelesenen Meldungen.</p>
			{:else}
				<ul class="plain liste">
					{#each g.meldungen as e (e.id)}
						{@const href = e.ziel_da ? zielHref(e.ziel) : null}
						<li class="meldung art-{e.meldung?.art ?? 'info'}">
							<span class="ort"
								>{e.meldung ? MELDUNG_WORT[e.meldung.art] : 'Meldung'} · {meldungsOrt(e)}</span
							>
							{#if href !== null}
								<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- zielHref baut mit resolve() -->
								<a class="titel" {href} onclick={schliessen}>{e.titel}</a>
							{:else}
								<span class="titel">{e.titel} <span class="muted">(nicht mehr da)</span></span>
							{/if}
							{#if (e.meldung?.anzahl ?? 1) > 1}
								<span class="muted small">{e.meldung?.anzahl}-mal</span>
							{/if}
							<button
								type="button"
								class="button gelesen"
								disabled={speicher.beschaeftigt !== null}
								aria-label="Gelesen: {e.titel}"
								onclick={() => void gelesen(e.id)}>gelesen</button
							>
						</li>
					{/each}
				</ul>
			{/if}
			{#if g.hinweise.length > 0}
				<h3 class="teil">Hinweise</h3>
				<ul class="plain liste">
					{#each g.hinweise as e (e.id)}
						<li class="meldung hinweis-zeile">
							<span class="ort">Hinweis · {meldungsOrt(e)}</span>
							<span class="titel">{e.titel}</span>
							<button
								type="button"
								class="button gelesen"
								disabled={speicher.beschaeftigt !== null}
								aria-label="Gelesen: {e.titel}"
								onclick={() => void gelesen(e.id)}>gelesen</button
							>
						</li>
					{/each}
				</ul>
			{/if}
			{#if behoben.length > 0}
				<h3 class="teil">behoben</h3>
				<ul class="plain liste">
					{#each behoben as m (m.id)}
						{@const href = behobenZiel(m)}
						<li class="meldung behoben">
							<span class="ort">behoben · {m.system ? 'PC' : m.projekt}</span>
							{#if href !== null}
								<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- zielHref baut mit resolve() -->
								<a class="titel" {href} onclick={schliessen}>{m.titel}</a>
							{:else}
								<span class="titel">{m.titel}</span>
							{/if}
							<button
								type="button"
								class="button gelesen"
								disabled={speicher.beschaeftigt !== null}
								aria-label="Gelesen: {m.titel}"
								onclick={() => void gelesen(m.id)}>gelesen</button
							>
						</li>
					{/each}
				</ul>
			{/if}
			<p class="fuss small">
				<a href="{resolve('/anstehend')}?art=meldung" onclick={schliessen}>alle in Anstehend</a>
			</p>
			{#if fehler}
				<p class="block-error small" role="alert">{fehler}</p>
			{/if}
		{/if}
	{/snippet}
</Klappfeld>

<style>
	.glocke {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
	}
	svg path {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.75;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.zahl {
		position: absolute;
		top: -0.375rem;
		right: -0.625rem;
		min-width: 1.25rem;
		padding: 0 0.3em;
		border-radius: 999px;
		background: var(--warn);
		color: var(--on-warn);
		font-size: var(--text-2xs);
		font-weight: 700;
		line-height: 1.5;
		text-align: center;
	}
	.liste {
		display: grid;
		gap: 0.5rem;
		margin: 0 0 0.5rem;
	}
	.meldung {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.125rem 0.5rem;
		padding: 0.375rem 0.5rem;
		border-left: 0.1875rem solid var(--line);
		border-radius: 0.375rem;
	}
	.meldung.art-fehler,
	.meldung.art-frist {
		border-left-color: var(--warn);
		background: var(--warn-soft);
	}
	.meldung.art-fehler .titel {
		font-weight: 600;
	}
	.meldung.hinweis-zeile {
		border-left-color: var(--info);
		background: var(--info-soft);
	}
	.meldung.behoben {
		border-left-color: var(--ok);
	}
	.ort {
		flex: 1 0 100%;
		font-size: var(--text-xs);
		color: var(--muted);
	}
	.titel {
		flex: 1 1 10rem;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.gelesen {
		min-height: 2.75rem;
		padding: 0.15rem 0.65rem;
		font-size: var(--text-sm);
		font-weight: 400;
	}
	.gelesen:disabled {
		opacity: 0.6;
		cursor: not-allowed;
	}
	.teil {
		margin: 0.75rem 0 0.375rem;
		font-size: var(--text-sm);
		color: var(--muted);
	}
	.hinweis {
		margin: 0 0 0.5rem;
	}
	.fuss {
		margin: 0.25rem 0 0;
	}
	.fuss a,
	.small a {
		display: inline-flex;
		align-items: center;
		min-height: 2.75rem;
	}
</style>
