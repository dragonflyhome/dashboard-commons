<script lang="ts" module>
	import { api, ApiError } from '#lib/api/client.ts';
	import { betrifftProjekt, live, type InhaltAenderung } from '#lib/live.svelte.ts';
	import { freierKanal } from '#lib/freikanal.svelte.ts';
	import type { ProjectDetail, ProjectSummary, ProjektAntwort } from '$vertrag';

	export type Daten = ProjectDetail & { kopf: ProjektAntwort };

	/** Wovon das Laden abhängt; in Tests ersetzt. */
	export interface TeilLader {
		laden(slug: string): Promise<Daten>;
		onInhalt(l: (a: InhaltAenderung) => void): () => void;
	}

	export const standard: TeilLader = {
		async laden(slug) {
			const [projekt, kopf] = await Promise.all([api.project(slug), api.projekt(slug)]);
			return { ...projekt, kopf };
		},
		onInhalt(l) {
			const aus = live.onInhalt(l);
			// Ohne geschützten Kanal kommen die Änderungen über den freien.
			const ausFrei = freierKanal.onInhalt((a) => {
				if (!live.connected) l(a);
			});
			const ausNachholen = live.onNachholen(() => {
				l({ projekte: [], daten: [], version: 0, sammel: true });
			});
			return () => {
				aus();
				ausFrei();
				ausNachholen();
			};
		}
	};
</script>

<script lang="ts">
	/**
	 * Ein Teil der Gruppenseite (Entscheidung projektgruppe): ein
	 * kleiner Kopf (Titel des Projekts, Status, Link „eigene Seite“), darunter
	 * die Seite des Projekts, wie unter `/projects/<slug>`. Fragen und
	 * Aufgaben stehen darin wie auf der eigenen Seite; Antworten gehen
	 * dieselben Wege an den Chat dieses Projekts.
	 *
	 * Der Teil lädt selbst und hört selbst auf den Live-Kanal: Ändert sich
	 * dieses Projekt (oder viele auf einmal), lädt nur dieser Abschnitt neu.
	 * Er steht in einer eigenen Fehlergrenze; ist er rot (die Seite lässt sich
	 * nicht laden oder wirft beim Zeichnen), bleiben die anderen Teile stehen.
	 */
	import { resolve } from '$app/paths';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import Fehlerbild from './Fehlerbild.svelte';
	import ProjektSeite from './ProjektSeite.svelte';

	interface Props {
		slug: string;
		/** Der Eintrag aus der Projektliste (Titel und Status für den Kopf, solange geladen wird). */
		projekt: ProjectSummary | null;
		/** Sprungmarke des Abschnitts. */
		anker: string;
		lader?: TeilLader;
	}
	let { slug, projekt, anker, lader = standard }: Props = $props();

	let daten = $state.raw<Daten | null>(null);
	let fehler = $state<string | null>(null);
	/** „neu versuchen“: Grenze neu zeichnen. */
	let runde = $state(0);
	let nr = 0;

	async function laden(s: string): Promise<void> {
		nr += 1;
		const meine = nr;
		try {
			const d = await lader.laden(s);
			if (meine !== nr) return;
			daten = d;
			fehler = null;
		} catch (e) {
			if (meine !== nr) return;
			fehler =
				e instanceof ApiError || e instanceof Error
					? e.message
					: 'Die Seite ließ sich nicht laden.';
		}
	}

	$effect(() => {
		const s = slug;
		void laden(s);
		return lader.onInhalt((a) => {
			if (betrifftProjekt(a, s)) void laden(s);
		});
	});

	const titel = $derived(daten?.title ?? projekt?.title ?? slug);
	const status = $derived(daten?.status ?? projekt?.status ?? '');
	const bedeutung = $derived(daten?.bedeutung ?? projekt?.bedeutung ?? 'neutral');
</script>

<section class="teil" id={anker} aria-labelledby="{anker}-titel" data-teil={slug}>
	<header class="teil-kopf">
		<h2 id="{anker}-titel"><StatusDot {bedeutung} label={status} /> {titel}</h2>
		<span class="muted small">{status}</span>
		<a class="small" href={resolve('/projects/[slug]', { slug })}>eigene Seite</a>
	</header>
	{#if fehler !== null && daten === null}
		<Fehlerbild
			satz="Dieser Teil lässt sich gerade nicht laden: {fehler}"
			{fehler}
			datei={null}
			neu={() => void laden(slug)}
		/>
	{:else if daten === null}
		<p class="muted">Wird geladen …</p>
	{:else}
		{#key runde}
			<svelte:boundary
				onerror={(e: unknown) => {
					console.error(`Fehler im Teil ${slug}:`, e);
				}}
			>
				<ProjektSeite data={daten} eingebettet />
				{#snippet failed(e: unknown)}
					<Fehlerbild
						satz="Dieser Teil hat beim Zeichnen einen Fehler; der Chat des Projekts kann es beheben."
						fehler={e}
						datei={null}
						neu={() => {
							runde += 1;
						}}
					/>
				{/snippet}
			</svelte:boundary>
		{/key}
	{/if}
</section>

<style>
	.teil {
		margin-top: 1.5rem;
		padding-top: 0.75rem;
		border-top: 1px solid var(--line);
		min-width: 0;
		/* Platz unter der festen Kopfzeile beim Sprung zur Marke. */
		scroll-margin-top: 72px;
	}
	.teil-kopf {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.25rem 0.75rem;
	}
	.teil-kopf h2 {
		display: flex;
		align-items: baseline;
		margin: 0;
		font-size: var(--text-lg);
		overflow-wrap: anywhere;
	}
</style>
