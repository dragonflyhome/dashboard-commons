<script lang="ts">
	import { resolve } from '$app/paths';
	import type { Bedeutung, ProjectSummary, SeitenBau } from '$vertrag';
	import { gitStandHinweis } from '#lib/format.ts';
	import StatusDot from './StatusDot.svelte';

	interface Props {
		project: ProjectSummary;
		/** Etappe 4c: Bau-Stand der Seite aus `auswerten` (`null`: ohne eigene Seite). */
		bau?: { zustand: SeitenBau; bedeutung: Bedeutung; wort: string; zeile: string } | null;
		/** Nur für Tests: die Uhrzeit, gegen die das Alter des Git-Stands gemessen wird. */
		jetzt?: Date;
	}
	let { project, bau = null, jetzt }: Props = $props();

	// Das Alter wächst auch ohne neue Daten: jede Minute neu messen.
	let uhr = $state(new Date());
	$effect(() => {
		const t = setInterval(() => {
			uhr = new Date();
		}, 60_000);
		return () => {
			clearInterval(t);
		};
	});

	const dirty = $derived(project.repos.reduce((n, r) => n + (r.dirty_files ?? 0), 0));
	const repoError = $derived(project.repos.find((r) => r.error !== null));
	const ermittelt = $derived(project.repos.some((r) => r.stand === null));
	/** Der älteste Git-Stand der Karte; ist er älter als fünf Minuten, steht sein Alter da. */
	const aeltester = $derived(
		project.repos.reduce<string | null>((alt, r) => {
			if (r.stand === null) return alt;
			return alt === null || Date.parse(r.stand) < Date.parse(alt) ? r.stand : alt;
		}, null)
	);
	const alter = $derived(aeltester === null ? null : gitStandHinweis(aeltester, jetzt ?? uhr));
</script>

<a class="tile card" href={resolve('/projects/[slug]', { slug: project.slug })}>
	<h2><StatusDot bedeutung={project.bedeutung} label={project.status} /> {project.title}</h2>
	<p class="meta muted small">{project.status} · zuletzt {project.last_change}</p>
	{#if project.summary}
		<p>{project.summary}</p>
	{/if}
	<div class="pills">
		{#if project.chat}
			<span class="pill" title="Welcher Chat dieses Projekt verwaltet">💬 {project.chat}</span>
		{/if}
		{#if project.backup && project.backup_bedeutung}
			<span class="pill" title={project.backup_note || 'Backup-Stand'}>
				<StatusDot bedeutung={project.backup_bedeutung} label="Backup {project.backup}" /> Backup: {project.backup}
			</span>
		{/if}
		{#if project.repos.length > 0}
			{#if repoError}
				<span class="pill" title={repoError.error ?? ''}>⚠ {repoError.label}</span>
			{:else if ermittelt}
				<span class="pill muted">Git-Stand wird ermittelt</span>
			{:else if dirty === 0}
				<span class="pill">✔ alles gesichert</span>
			{:else}
				<span class="pill">✎ {dirty} ungesicherte Datei{dirty === 1 ? '' : 'en'}</span>
			{/if}
			{#if alter && !ermittelt}
				<span class="pill muted" title="So alt ist der älteste Git-Stand dieser Karte">{alter}</span
				>
			{/if}
		{/if}
		<!-- Etappe B2: ohne Anmeldung kommt nur die Zahl der offenen Punkte (open_zahl). -->
		{#if (project.open_zahl ?? project.open.length) > 0}
			<span class="pill">{project.open_zahl ?? project.open.length} offene Punkte</span>
		{/if}
		{#if project.vorschau}
			<span class="pill" title="Die Seite zeigt einen Entwurf der Website">🖥 Vorschau</span>
		{/if}
		<!-- Etappe 4c: nur, wenn die Seite nicht in Ordnung ist (rot, im Bau, noch nie gebaut). -->
		{#if bau !== null && bau.zustand !== 'gruen'}
			<span class="pill bau" data-bau={bau.zustand}>
				<StatusDot bedeutung={bau.bedeutung} decorative />
				{bau.zustand === 'rot' ? bau.zeile : bau.wort}
			</span>
		{/if}
	</div>
</a>

<style>
	.tile {
		display: block;
		color: var(--text);
		transition:
			transform 160ms ease,
			box-shadow 160ms ease,
			border-color 160ms ease;
	}
	.tile:hover,
	.tile:focus-visible {
		text-decoration: none;
		transform: translateY(-2px);
		border-color: var(--accent);
		box-shadow: var(--shadow-hover);
	}
	h2 {
		font-size: var(--text-lg);
		display: flex;
		align-items: baseline;
	}
	.meta {
		margin-top: -4px;
	}
	.pills {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 8px;
	}
	/* Lange Pillen (etwa „⚠ beispiel-projekt (Entwurfsdateien, Rust, Markdown)“)
	   brechen am Handy um, statt die Seite breiter zu machen (Abnahme 3b:
	   13 px seitlich bei 360 px). */
	.pills .pill {
		max-width: 100%;
		white-space: normal;
	}
</style>
