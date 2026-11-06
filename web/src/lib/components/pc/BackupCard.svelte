<script lang="ts">
	import { resolve } from '$app/paths';
	import type { Backup, BackupLage, ProjectBackup } from '$vertrag';
	import { backupKopien, backupVerdict } from '#lib/pc.ts';
	import { klasse } from '#lib/bedeutung.ts';
	import { fmtBytes } from '#lib/format.ts';
	import StatusDot from '../StatusDot.svelte';

	interface Props {
		backup: Backup;
		/** Kopien und 3-2-1-Regel vom Server; `null`, wenn data/pc.toml kaputt ist. */
		stand: BackupLage | null;
		projects: ProjectBackup[];
		/** Zu sichernde Datenmenge, wenn schon gezählt. */
		totalBytes: number | null;
	}
	let { backup, stand, projects, totalBytes }: Props = $props();
	const level = $derived(backupVerdict(stand));
	const kopien = $derived(stand ? backupKopien(stand) : []);
</script>

<div class="head">
	<span class="lamp {klasse(level.bedeutung)}" role="img" aria-label="Backup-Ampel: {level.text}"
	></span>
	<div>
		<p class="big">{level.text}</p>
		<p class="muted">{backup.note}</p>
	</div>
</div>
<div class="copies">
	{#each kopien as k (k.ort)}
		<div
			class="copy"
			class:have={k.stand === 'vorhanden'}
			class:part={k.stand === 'teilweise'}
			class:miss={k.stand === 'fehlt'}
			title={k.text || undefined}
		>
			<div class="icon">{k.icon}</div>
			<div class="title">{k.titel}</div>
			<div class="state">{k.stand}</div>
		</div>
	{/each}
</div>
{#if totalBytes !== null}
	<p class="muted small">
		Zu sichern wären etwa {fmtBytes(totalBytes)}. Eine 2-TB-Platte reicht dafür mehrfach.
	</p>
{/if}
{#if backup.cloud}<p class="small">☁️ {backup.cloud}</p>{/if}
{#if backup.plan.length > 0}
	<h3>Geplanter Weg</h3>
	<ol class="plan">
		{#each backup.plan as step, i (i)}
			<li>{step}</li>
		{/each}
	</ol>
{/if}
{#if projects.length > 0}
	<h3>Backup je Projekt</h3>
	<p class="muted small">
		Tragen die Projekt-Chats selbst ein (Kopffelder backup und backup_note).
	</p>
	<ul class="plain projects">
		{#each projects as p (p.slug)}
			<li>
				<StatusDot bedeutung={p.bedeutung} label="Backup {p.backup}" />
				<a href={resolve('/projects/[slug]', { slug: p.slug })}>{p.title}</a>: {p.backup}
				{#if p.backup_note}<span class="muted"> · {p.backup_note}</span>{/if}
			</li>
		{/each}
	</ul>
{/if}

<style>
	.head {
		display: flex;
		gap: 14px;
		align-items: center;
	}
	.lamp {
		width: 46px;
		height: 46px;
		border-radius: 50%;
		flex: none;
		background: var(--ton, var(--plan));
		box-shadow: var(--ring-hof);
	}
	.copies {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
		gap: 10px;
		margin: 12px 0;
	}
	.copy {
		border: 1px solid var(--line);
		border-radius: 12px;
		padding: 10px;
		text-align: center;
	}
	.copy.have {
		border-color: var(--ok);
		background: var(--ok-soft);
	}
	.copy.part {
		border-color: var(--warn);
		background: var(--warn-soft);
	}
	.copy.miss {
		border-style: dashed;
		opacity: 0.8;
	}
	.icon {
		font-size: var(--text-4xl);
	}
	.title {
		font-weight: 600;
		font-size: var(--text-md);
	}
	.state {
		color: var(--muted);
		font-size: var(--text-sm);
	}
	.copy.part .state {
		color: var(--warn-text);
	}
	.copy.miss .state {
		color: var(--bad-text);
	}
	.plan li {
		margin: 3px 0;
	}
	.projects li {
		padding: 2px 0;
	}
</style>
