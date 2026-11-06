<script lang="ts">
	/**
	 * Systemmeldungen auf der PC-Seite (Etappe A3, Konzept [ziele]): Meldungen
	 * wie „Bote fehlt“ gehören zur PC-Seite; das Band und „Anstehend“ führen
	 * mit `/pc#meldung-<id>` hierher. Jede steht aufklappbar mit Einzelheiten
	 * da; kommt man über ihren Anker, klappt sie auf und bekommt den Fokus.
	 * Fehlt sie (archiviert), steht „nicht mehr da“.
	 *
	 * Die Liste kommt aus `/api/meldungen?projekt=dashboard` (Windows-
	 * Anmeldung; ohne sie fehlt der Kasten) und folgt dem Live-Kanal.
	 */
	import { page } from '$app/state';
	import { tick } from 'svelte';
	import { API } from '#lib/api/client.ts';
	import { live } from '#lib/live.svelte.ts';
	import { MELDUNG_WORT, SATZ_NICHT_MEHR_DA, meldungGelesen } from '#lib/anstehend.ts';
	import { kurzZeit } from '#lib/unterdashboard.ts';
	import { mitZugang } from '#lib/zugang.ts';
	import { profilVerbergen } from '#lib/pfade.ts';
	import { ankerAus, ansteuern, gleicherAnkerKlick } from '#lib/ziel.ts';
	import type { Meldung, MeldungenListe } from '$vertrag';

	interface Props {
		/** Für Tests; sonst das globale fetch. */
		fetchFn?: typeof fetch;
	}
	let { fetchFn = mitZugang((...a) => fetch(...a)) }: Props = $props();

	let meldungen = $state<Meldung[] | null>(null);
	let fehler = $state<string | null>(null);
	let anfrage = 0;

	async function laden(): Promise<void> {
		const nr = ++anfrage;
		try {
			const res = await fetchFn(`${API}/meldungen?projekt=dashboard`, {
				credentials: 'same-origin',
				headers: { accept: 'application/json' }
			});
			if (nr !== anfrage) return;
			meldungen = res.ok
				? ((await res.json()) as MeldungenListe).meldungen.filter((m) => m.system)
				: null;
		} catch {
			if (nr === anfrage) meldungen = null;
		}
	}

	$effect(() => {
		void laden();
		const off = live.onChange((c) => {
			if (c.area === 'anstehend' && (c.path === 'anstehend' || c.path === 'anstehend/dashboard'))
				void laden();
		});
		const offNachholen = live.onNachholen(() => {
			void laden();
		});
		return () => {
			off();
			offNachholen();
		};
	});

	let titel = $state<HTMLElement | null>(null);
	let laeuft = $state(false);
	async function gelesen(id: string, knopf: HTMLButtonElement): Promise<void> {
		laeuft = true;
		fehler = null;
		try {
			await meldungGelesen(id, fetchFn);
		} catch {
			fehler = 'Die Meldung ließ sich nicht als gelesen ablegen.';
		}
		try {
			await laden();
		} finally {
			laeuft = false;
		}
		await tick();
		const aktiv = document.activeElement;
		if (aktiv === null || aktiv === document.body || aktiv === knopf) titel?.focus();
	}

	// Ziel ansteuern, sobald die Liste da ist (einmal je Anker).
	const hash = $derived(ankerAus(page.url.hash));
	let angesteuert = '';
	let ankerSatz = $state<string | null>(null);
	$effect(() => {
		const h = hash;
		if (!h.startsWith('meldung-')) {
			ankerSatz = null;
			angesteuert = '';
			return;
		}
		if (meldungen === null || angesteuert === h) return;
		angesteuert = h;
		void tick().then(() => {
			ankerSatz = ansteuern(h) ? null : SATZ_NICHT_MEHR_DA;
		});
	});

	/** Etappe 3b-2: derselbe Meldungs-Anker noch einmal geklickt (wie im Unter-Dashboard). */
	function beiKlick(ev: MouseEvent): void {
		const id = gleicherAnkerKlick(ev);
		if (id?.startsWith('meldung-')) ansteuern(id);
	}
</script>

<svelte:document onclick={beiKlick} />

{#if ankerSatz}
	<p class="anker-satz" role="status">{ankerSatz}</p>
{/if}
{#if meldungen !== null && meldungen.length > 0}
	<section class="card meldungen" aria-labelledby="pc-meldungen">
		<h2 id="pc-meldungen" tabindex="-1" bind:this={titel}>Meldungen zum PC</h2>
		<ul class="plain liste">
			{#each meldungen as m (m.id)}
				<li class:offen={m.gelesen === null && m.behoben === null}>
					<details id="meldung-{m.id}">
						<summary>
							<b>{MELDUNG_WORT[m.art]}:</b>
							{profilVerbergen(m.titel)}
							{#if m.behoben !== null}<span class="muted small"> · behoben</span>{/if}
						</summary>
						<ul class="plain einzel small">
							<li>
								zuletzt {kurzZeit(m.zuletzt)}{#if m.anzahl > 1}, {m.anzahl}-mal{/if}
							</li>
							{#if m.text}<li class="text">{profilVerbergen(m.text)}</li>{/if}
						</ul>
						{#if m.gelesen === null}
							<button
								type="button"
								class="button klein"
								disabled={laeuft}
								aria-label="Gelesen: {m.titel}"
								onclick={(ev) => {
									void gelesen(m.id, ev.currentTarget);
								}}>gelesen</button
							>
						{/if}
					</details>
				</li>
			{/each}
		</ul>
		{#if fehler}<p class="block-error small" role="alert">{fehler}</p>{/if}
	</section>
{/if}

<style>
	.meldungen {
		margin: 1rem 0;
		border-left: 0.25rem solid var(--warn);
	}
	h2 {
		font-size: var(--text-base);
		margin-bottom: 0.5rem;
	}
	h2:focus {
		outline: 0.125rem solid var(--accent);
		outline-offset: 0.125rem;
	}
	.liste {
		display: grid;
		gap: 0.4rem;
	}
	.offen summary {
		font-weight: 600;
	}
	summary {
		cursor: pointer;
		overflow-wrap: anywhere;
	}
	.einzel {
		margin: 0.25rem 0 0.4rem 1.2rem;
		display: grid;
		gap: 0.1rem;
	}
	.text {
		white-space: pre-line;
		overflow-wrap: anywhere;
	}
	.klein {
		padding: 0.15rem 0.65rem;
		font-size: var(--text-sm);
		min-height: 2.25rem;
		margin-left: 1.2rem;
	}
	.anker-satz {
		margin: 0.5rem 0;
		padding: 0.4rem 0.75rem;
		border-left: 0.25rem solid var(--accent);
		background: var(--accent-soft);
		border-radius: 0.4rem;
	}
</style>
