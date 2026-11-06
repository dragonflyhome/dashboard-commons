<script lang="ts">
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { resolve } from '$app/paths';
	import { api } from '#lib/api/client.ts';
	import Kpi from '#lib/components/blocks/Kpi.svelte';
	import VorgangZeile from '#lib/components/vorgaenge/VorgangZeile.svelte';
	import { fmtZeitpunkt, zustandWort } from '#lib/vorgaenge.ts';
	import type { KpiItem } from '$vertrag';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const u = $derived(data.uebersicht);

	const zahlen = $derived<KpiItem[]>(
		u === null
			? []
			: [
					{ label: 'Offen', value: String(u.zaehler.offen), note: 'noch niemand gerufen' },
					{
						label: 'Zugestellt',
						value: String(u.zaehler.zugestellt),
						note: 'der Chat hat es, aber noch nicht bestätigt'
					},
					{
						label: 'Übernommen heute',
						value: String(u.zaehler.uebernommen_heute),
						note: 'ein Chat kümmert sich'
					}
				]
	);
</script>

<svelte:head>
	<title>{seitenTitel('Vorgänge')}</title>
</svelte:head>

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Vorgänge</p>
<h1>Was zwischen den Chats läuft</h1>
<p>
	Ein Vorgang ist eine Sache mit Nummer und Zustand, die nicht verloren gehen darf: eine Frage, eine
	Antwort, ein Wunsch, eine Abnahme. Die Chats holen sich ihre Vorgänge selbst ab.
</p>

{#if data.fehler !== null || u === null}
	<p class="block-error" role="alert">{data.fehler ?? 'Die Vorgänge sind gerade nicht lesbar.'}</p>
{:else}
	<Kpi items={zahlen} />

	<section class="card" id="merge">
		<h2>Offene Merge-Requests</h2>
		{#if u.merge_offen.length === 0}
			<p class="muted">Kein Projekt hat gerade ein Backend-Modul zur Prüfung eingereicht.</p>
		{:else}
			<p class="muted small">
				Ein Projekt-Chat reicht sein Backend-Modul ein. Der Betreuer des Dashboards prüft, die
				Architektur-Rolle nimmt ab, dann kommt es mit einem Merge-Commit nach main.
			</p>
			<ul class="plain">
				{#each u.merge_offen as v (v.id)}
					<li class="merge" id={`merge-${v.id}`}>
						<p>
							<strong>{v.titel}</strong>
							<span class="muted small">
								· Projekt <code>{v.merge?.projekt ?? '–'}</code> · {zustandWort(v.zustand)} · seit
								{fmtZeitpunkt(v.erstellt)}</span
							>
						</p>
						{#if v.merge}
							<p class="small">
								Zweig <code>{v.merge.zweig}</code>, Commit <code>{v.merge.commit}</code>
							</p>
							<details>
								<summary>Beschreibung</summary>
								<pre>{v.merge.beschreibung}</pre>
							</details>
							<details>
								<summary>Prüfbericht des Chats</summary>
								<pre>{v.merge.pruefbericht}</pre>
							</details>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="card" id="chats">
		<h2>Chats</h2>
		{#if u.chats.length === 0}
			<p class="muted">Noch kein Chat hat Vorgänge.</p>
		{:else}
			<div class="breit">
				<table>
					<thead>
						<tr>
							<th>Chat</th>
							<th>Wartet</th>
							<th>Zuletzt übernommen</th>
							<th class="num">Offen</th>
						</tr>
					</thead>
					<tbody>
						{#each u.chats as c (c.slug)}
							<tr>
								<td><code>{c.slug}</code></td>
								<td>
									{#if c.wartet_seit}
										wartet seit {fmtZeitpunkt(c.wartet_seit)}
									{:else}
										<span class="muted">wartet nicht</span>
									{/if}
								</td>
								<td>{c.zuletzt_uebernommen ? fmtZeitpunkt(c.zuletzt_uebernommen) : '–'}</td>
								<td class="num" class:fett={c.offen > 0}>{c.offen}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	</section>

	<section class="card" id="liste">
		<h2>Vorgänge</h2>
		{#if u.vorgaenge.length === 0}
			<p class="muted">Noch keine Vorgänge.</p>
		{:else}
			<p class="muted small">
				Ein Klick auf eine Zeile zeigt alle Schritte und wer sie gemacht hat.
			</p>
			<ul class="plain">
				{#each u.vorgaenge as v (v.id)}
					<VorgangZeile vorgang={v} laden={(id: string) => api.vorgang(id)} />
				{/each}
			</ul>
		{/if}
	</section>
{/if}

<style>
	section {
		margin-top: 16px;
		scroll-margin-top: 70px;
	}
	.breit {
		overflow-x: auto;
	}
	table {
		width: 100%;
	}
	.num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.merge + .merge {
		margin-top: 0.75rem;
		padding-top: 0.75rem;
		border-top: 1px solid var(--line);
	}
	pre {
		white-space: pre-wrap;
		overflow-x: auto;
	}
	.fett {
		font-weight: 700;
	}
</style>
