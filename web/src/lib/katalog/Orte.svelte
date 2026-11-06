<script lang="ts">
	/**
	 * Orte aus `daten/orte.toml`: je Datei oder Ordner auf diesem PC Titel,
	 * Hinweis und ein Knopf „Öffnen“ (Entscheidung lokal-oeffnen).
	 * Der Knopf schickt nur Projekt und Kennung; den Pfad kennt nur der Server,
	 * er steht nie auf der Seite. Kein `file:`-Link, kein „Pfad kopieren“.
	 * Das Projekt kommt aus der Route wie bei `ladeDaten`.
	 */
	import type { Abgeleitet, Orte } from '$vertrag';
	import Daten from './intern/Daten.svelte';
	import type { Quelle } from './abgeleitet.ts';
	import { kontext } from '../projekt/laden.svelte.ts';
	import { fehlerSatz, oeffnen } from '../oeffnen.ts';

	interface Props {
		daten: Quelle<Orte>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
		/** Nur für Beispiele und Tests; sonst das Projekt der Seite. */
		projekt?: string | undefined;
		/** Nur für Tests. */
		fetchFn?: typeof fetch | undefined;
	}
	let { daten, teil, abgeleitet, projekt, fetchFn }: Props = $props();

	/** Ohne `projekt` gilt das Projekt der Route (Kontext, nur beim Anlegen lesbar). */
	function projektDerSeite(): { readonly slug: string } | null {
		return projekt === undefined ? kontext('Orte') : null;
	}
	const seite = projektDerSeite();
	const slug = $derived(projekt ?? seite?.slug ?? '');

	/** Je Kennung: läuft gerade, oder der letzte Satz (geöffnet oder warum nicht). */
	let laeuft = $state<Record<string, boolean>>({});
	let saetze = $state<Record<string, { text: string; gut: boolean }>>({});

	async function auf(kennung: string): Promise<void> {
		laeuft[kennung] = true;
		try {
			const a = await oeffnen(slug, kennung, fetchFn ?? fetch);
			saetze[kennung] = { text: a.satz, gut: true };
		} catch (e) {
			saetze[kennung] = { text: fehlerSatz(e), gut: false };
		} finally {
			laeuft[kennung] = false;
		}
	}
</script>

<Daten {daten} {teil} {abgeleitet}>
	{#snippet children(o: Orte)}
		{#if o.ort.length === 0}
			<p class="muted small">Noch keine Orte in <code>daten/orte.toml</code>.</p>
		{:else}
			<ul class="plain orte">
				{#each o.ort as ort (ort.kennung)}
					<li>
						<div class="text">
							<span class="titel">{ort.titel}</span>
							{#if ort.hinweis}
								<span class="muted small">{ort.hinweis}</span>
							{/if}
						</div>
						<button
							type="button"
							class="button"
							disabled={laeuft[ort.kennung] === true}
							aria-label="{ort.titel} öffnen"
							onclick={() => void auf(ort.kennung)}
						>
							Öffnen
						</button>
						{#if saetze[ort.kennung]}
							<p
								class="satz small"
								class:block-error={!saetze[ort.kennung]?.gut}
								role={saetze[ort.kennung]?.gut ? 'status' : 'alert'}
							>
								{saetze[ort.kennung]?.text}
							</p>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	{/snippet}
</Daten>

<style>
	.orte li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 6px 12px;
		padding: 8px 0;
		border-bottom: 1px solid var(--line);
	}
	.orte li:last-child {
		border-bottom: none;
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.satz {
		flex-basis: 100%;
		margin: 0;
	}
</style>
