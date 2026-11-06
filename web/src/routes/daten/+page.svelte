<script lang="ts">
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { resolve } from '$app/paths';
	import { invalidate } from '$app/navigation';
	import { api, exportUrl } from '#lib/api/client.ts';
	import { fmtBytes, istZahl } from '#lib/format.ts';
	import { scrollShadow } from '#lib/scrollschatten.ts';
	import Sicherung from '#lib/components/daten/Sicherung.svelte';
	import Verlauf from '#lib/components/daten/Verlauf.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const b = $derived(data.bericht);
	const werte = $derived(data.verlauf.werte);
	/** Satz des Servers, wenn der Datenspeicher nicht offen ist; sonst null. */
	const fehler = $derived(b.fehler ?? null);
	const ohneSpeicher = 'Ohne Datenspeicher nicht verfügbar.';

	const toc = [
		['datei', 'Die Datei'],
		['sicherung', 'Sicherung'],
		['tabellen', 'Tabellen'],
		['verlauf', 'PC-Verlauf'],
		['export', 'Export']
	] as const;

	function fmtZeit(iso: string | null): string {
		if (iso === null) return '–';
		return new Date(iso).toLocaleString('de-DE', {
			day: '2-digit',
			month: '2-digit',
			hour: '2-digit',
			minute: '2-digit'
		});
	}
	function zahl(n: number): string {
		return istZahl(n) ? n.toLocaleString('de-DE') : '–';
	}
</script>

<svelte:head>
	<title>{seitenTitel('Daten')}</title>
</svelte:head>

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Daten</p>
<h1>Was der Server sich merkt</h1>
<nav class="toc" aria-label="Abschnitte">
	{#each toc as [id, label] (id)}
		<a href="#{id}">{label}</a>
	{/each}
</nav>

{#if fehler}
	<p class="block-error" role="alert">{fehler}</p>
{/if}

<section class="card" id="datei">
	<h2>Die Datei</h2>
	<p>
		Hier merkt sich der Server, was zur Laufzeit entsteht: Messwerte, später Antworten und Vorgänge.
		Passwörter liegen nie hier.
	</p>
	<dl class="eck">
		<dt>Ort</dt>
		<dd><code class="pfad">{b.pfad}</code></dd>
		<dt>Größe</dt>
		<dd>{fmtBytes(b.groesse_bytes)}</dd>
		<dt>Schema</dt>
		<dd>{b.schema > 0 ? `Nummer ${b.schema}` : 'unbekannt'}</dd>
	</dl>
</section>

<section class="card" id="sicherung">
	<h2>Sicherung</h2>
	<Sicherung bericht={b} sichern={() => api.sichern()} nachher={() => invalidate('api:daten')} />
</section>

<section class="card" id="tabellen">
	<h2>Tabellen</h2>
	{#if fehler}
		<p class="muted">{ohneSpeicher}</p>
	{:else if b.tabellen.length === 0}
		<p class="muted">Noch keine Tabellen.</p>
	{:else}
		<div class="scroll-x" use:scrollShadow>
			<table>
				<thead>
					<tr><th>Name</th><th>Beschreibung</th><th class="num">Einträge</th></tr>
				</thead>
				<tbody>
					{#each b.tabellen as t (t.name)}
						<tr>
							<td><code>{t.name}</code></td>
							<td>{t.beschreibung}</td>
							<td class="num">{zahl(t.eintraege)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</section>

<section class="card" id="verlauf">
	<h2>PC-Verlauf (24 Stunden)</h2>
	{#if fehler}
		<p class="muted">{ohneSpeicher}</p>
	{:else}
		<Verlauf {werte} />
		<dl class="eck">
			<dt>Erster Messwert</dt>
			<dd>{fmtZeit(b.pc_verlauf.erster)}</dd>
			<dt>Letzter Messwert</dt>
			<dd>{fmtZeit(b.pc_verlauf.letzter)}</dd>
			<dt>Anzahl</dt>
			<dd>{zahl(b.pc_verlauf.eintraege)}</dd>
		</dl>
	{/if}
</section>

<section class="card" id="export">
	<h2>Export</h2>
	{#if fehler}
		<p class="muted">{ohneSpeicher}</p>
	{:else}
		<p>Alles, was im Datenspeicher steht, als lesbare Datei.</p>
		<p class="links">
			<!-- eslint-disable svelte/no-navigation-without-resolve -- Export kommt vom Rust-Server, keine Seite der App -->
			<a class="button" href={exportUrl('toml')} target="_blank" rel="noopener">Als TOML ansehen</a>
			<a class="button" href={exportUrl('json')} download>Als JSON herunterladen</a>
			<!-- eslint-enable svelte/no-navigation-without-resolve -->
		</p>
	{/if}
</section>

<style>
	.toc {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin: 0 0 14px;
	}
	.toc a {
		padding: 4px 12px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--line) 50%, transparent);
		color: var(--text);
		font-size: var(--text-md);
	}
	.toc a:hover {
		background: var(--accent-soft);
		text-decoration: none;
	}
	section {
		margin-top: 16px;
		scroll-margin-top: 70px;
	}
	.eck {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 4px 14px;
		margin: 8px 0 0;
	}
	.eck dt {
		color: var(--muted);
	}
	.eck dd {
		margin: 0;
		min-width: 0;
	}
	.pfad {
		overflow-wrap: anywhere;
	}
	.num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.links {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
</style>
