<script lang="ts">
	/**
	 * Das Backlog auf jeder Projektseite (Etappe B2): lädt
	 * `/api/projekte/<slug>/backlog` (geschützt, nur mit Windows-Anmeldung)
	 * und zeigt es mit dem Baustein `Backlog` aus `$katalog`. Es sitzt im
	 * Rahmen der Projektseite (alte und neue Form, auch im Standardaufbau),
	 * nicht in Seite.svelte; die Chats bauen es nicht selbst ein.
	 *
	 * Lädt neu wie das Unter-Dashboard: bei `change` für Anstehend oder die
	 * Aufgaben des Projekts, nach dem Nachholen, bei Vorgängen des Projekts
	 * und um Mitternacht. Ohne Anmeldung, ohne Daten oder wenn nichts offen
	 * und nichts fertig ist, fehlt der Abschnitt ganz.
	 */
	import type { Backlog as BacklogDaten } from '$vertrag';
	import { Backlog } from '$katalog';
	import { live } from '#lib/live.svelte.ts';
	import { beiTageswechsel } from '#lib/tageswechsel.ts';
	import { ladeBacklog } from '#lib/backlog.ts';
	import { mitZugang } from '#lib/zugang.ts';

	interface Props {
		slug: string;
		/** Für Tests; sonst das globale fetch. */
		fetchFn?: typeof fetch;
	}
	let { slug, fetchFn = (...a) => fetch(...a) }: Props = $props();

	let daten = $state<BacklogDaten | null>(null);
	/** Laufende Nummer: Eine späte Antwort überschreibt nie eine neuere. */
	let anfrage = 0;

	async function laden(fuer: string): Promise<void> {
		const nr = ++anfrage;
		const b = await ladeBacklog(fuer, mitZugang(fetchFn));
		if (fuer !== slug || nr !== anfrage) return;
		daten = b;
	}

	$effect(() => {
		const s = slug;
		daten = null;
		void laden(s);
		const aus = [
			live.onChange((c) => {
				const betrifft =
					(c.area === 'anstehend' && (c.path === 'anstehend' || c.path === `anstehend/${s}`)) ||
					(c.area === 'aufgaben' && c.path === `aufgaben/${s}`);
				if (betrifft) void laden(s);
			}),
			live.onNachholen(() => {
				void laden(s);
			}),
			live.onVorgang((v) => {
				if (v.slug === s) void laden(s);
			}),
			beiTageswechsel(() => {
				void laden(s);
			})
		];
		return () => {
			for (const ab of aus) ab();
		};
	});

	const zeigen = $derived(
		daten !== null && (daten.spalten.some((s) => s.karten.length > 0) || daten.wartend.length > 0)
	);
</script>

{#if zeigen && daten !== null}
	<section class="card backlog-rahmen" aria-labelledby="backlog-titel-{slug}">
		<h2 id="backlog-titel-{slug}">Backlog</h2>
		<p class="muted small satz">
			Aufgaben, Arbeiten und die offene Frage nach Stand; fertig heißt: in den letzten sieben Tagen.
		</p>
		<Backlog backlog={daten} />
	</section>
{/if}

<style>
	.backlog-rahmen {
		margin: 0.75rem 0 1rem;
	}
	h2 {
		margin: 0 0 0.2rem;
	}
	.satz {
		margin: 0 0 0.4rem;
	}
</style>
