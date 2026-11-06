<script lang="ts">
	import type { PcBenutzer, SysAppsRechner } from '$vertrag';
	import { fmtDateTime } from '#lib/format.ts';
	import { scrollShadow } from '#lib/scrollschatten.ts';
	import type { Benutzerdaten } from '#lib/pc.ts';
	import BenutzerHinweis from './BenutzerHinweis.svelte';
	import { profilVerbergen } from '#lib/pfade.ts';

	interface Props {
		/** Rechner-Teil aus `/api/pc`: Dienste, Fehler, Stand. */
		apps: SysAppsRechner;
		/**
		 * Benutzer-Teil aus `/api/pc/benutzer` (Werkzeuge, Autostart, Programme,
		 * Daten des Boten); `null` ohne Windows-Anmeldung, dann entfällt er.
		 */
		benutzer: PcBenutzer | null;
		stand?: Benutzerdaten;
	}
	let { apps, benutzer, stand = 'da' }: Props = $props();
	const b = $derived(benutzer?.sysapps ?? null);

	const upgrades = $derived((b?.winget ?? []).filter((w) => w.available !== '').length);
	let filter = $state('');
	const winget = $derived(
		(b?.winget ?? []).filter((w) => {
			const q = filter.trim().toLowerCase();
			return q === '' || w.name.toLowerCase().includes(q) || w.id.toLowerCase().includes(q);
		})
	);
	function short(s: string, n: number): string {
		return s.length <= n ? s : `${s.slice(0, n)}…`;
	}
</script>

<BenutzerHinweis {stand} letzterStand={benutzer?.stand ?? null} />
{#if !apps.refreshed_at}
	<p class="muted">
		Wird gelesen (Dienste, Autostart, winget). Das dauert beim ersten Mal bis zu zwei Minuten.
	</p>
{:else}
	{#if b}
		<h3>Werkzeuge</h3>
		<div class="tools">
			{#each b.tools as t (t.name)}
				<div class="tool" class:have={t.version !== null} class:miss={t.version === null}>
					<div class="name">{t.name}</div>
					<div class="ver muted small">{t.version ?? 'nicht gefunden'}</div>
				</div>
			{/each}
		</div>

		<h3>Autostart ({b.autostart.length})</h3>
		{#if b.autostart.length === 0}
			<p class="muted">Keine Autostart-Einträge gefunden.</p>
		{:else}
			<ul class="plain compact">
				{#each b.autostart as a, i (i)}
					<li>
						<b>{a.name}</b> <span class="muted small">· {a.source}</span><br />
						<!-- Keine rohen Pfade mit dem Kontonamen. -->
						<code class="small">{short(profilVerbergen(a.command), 110)}</code>
					</li>
				{/each}
			</ul>
		{/if}
	{/if}

	<h3>Dienste</h3>
	<p>{apps.services_running.length} von {apps.services_total} Diensten laufen.</p>
	<details>
		<summary>Laufende Dienste anzeigen</summary>
		<ul class="plain compact cols">
			{#each apps.services_running as s, i (i)}
				<li>{s.DisplayName} <span class="muted small">({s.Name})</span></li>
			{/each}
		</ul>
	</details>

	{#if b}
		<h3>Installierte Programme (winget)</h3>
		{#if b.winget_error}
			<p class="muted">{b.winget_error}</p>
		{:else}
			<p>
				{b.winget.length} Pakete bekannt{#if upgrades > 0}, {upgrades} davon mit Aktualisierung verfügbar{/if}.
			</p>
			<details>
				<summary>Liste anzeigen</summary>
				<input
					type="search"
					placeholder="Programm suchen …"
					aria-label="Programme filtern"
					bind:value={filter}
				/>
				<div class="scroll-x" use:scrollShadow>
					<table class="small">
						<thead><tr><th>Name</th><th>Version</th><th>Neu</th></tr></thead>
						<tbody>
							{#each winget as w, i (`${w.id}-${i}`)}
								<tr class:upgrade={w.available !== ''}>
									<td>{w.name}<br /><code class="small">{w.id}</code></td>
									<td>{w.version}</td>
									<td>{w.available}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</details>
		{/if}
	{/if}
	{#if apps.errors.length > 0}
		<p class="muted small">Nicht lesbar: {apps.errors.join(' · ')}</p>
	{/if}
	<p class="muted small">
		Stand: {fmtDateTime(apps.refreshed_at)} · wird stündlich neu gelesen · nur lesen, nichts wird verändert
	</p>
{/if}

<style>
	h3 {
		margin-top: 1em;
	}
	.tools {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		gap: 8px;
	}
	.tool {
		border: 1px solid var(--line);
		border-radius: 10px;
		padding: 8px 10px;
	}
	.tool.have {
		border-left: 4px solid var(--ok);
	}
	.tool.miss {
		border-left: 4px solid var(--plan);
		opacity: 0.8;
	}
	.name {
		font-weight: 600;
	}
	.compact li {
		padding: 3px 0;
	}
	.cols {
		columns: 2;
		column-gap: 24px;
	}
	.cols li {
		break-inside: avoid;
	}
	input[type='search'] {
		margin: 8px 0;
		width: min(100%, 320px);
	}
	tr.upgrade td {
		background: color-mix(in srgb, var(--warn) 10%, transparent);
	}
	@media (max-width: 37.5rem) {
		.cols {
			columns: 1;
		}
	}
</style>
