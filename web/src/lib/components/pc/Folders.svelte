<script lang="ts">
	import type { Inventory } from '$vertrag';
	import { fmtBytes, fmtDateTime } from '#lib/format.ts';
	import { scrollShadow } from '#lib/scrollschatten.ts';
	import { reihe } from '#lib/reihen.ts';
	import { profilVerbergen } from '#lib/pfade.ts';

	interface Props {
		inv: Inventory;
		diskTotal: number;
	}
	let { inv, diskTotal }: Props = $props();

	const total = $derived(
		Math.max(
			1,
			inv.entries.reduce((n, e) => n + e.bytes, 0)
		)
	);
	const shown = $derived(inv.entries.filter((e) => e.bytes > 0).slice(0, 10));
	const rest = $derived(inv.entries.slice(10).reduce((n, e) => n + e.bytes, 0));
	const skipped = $derived(inv.entries.filter((e) => e.skipped));
	let hover = $state<string | null>(null);

	function fmtSmall(b: number): string {
		if (b >= 1024 * 1024 * 1024) return fmtBytes(b);
		if (b >= 1024 * 1024) return `${(b / 1024 / 1024).toFixed(1).replace('.', ',')} MB`;
		return `${Math.ceil(b / 1024)} KB`;
	}
</script>

{#if inv.hinweis}
	<p class="muted">{inv.hinweis}</p>
{:else if !inv.computed_at}
	<p class="muted">
		Die Ordner werden gerade gezählt. Das dauert beim ersten Mal einige Minuten; danach steht das
		Ergebnis im Cache.
	</p>
{:else}
	<p class="muted small">
		{fmtBytes(total)} in <code>{profilVerbergen(inv.root)}</code>
		{#if diskTotal > 0}· das sind {Math.round((total * 100) / diskTotal)} % der Platte{/if}
		· gezählt {fmtDateTime(inv.computed_at)} in {inv.duration_secs} s · alle 6 Stunden neu
	</p>
	<div class="stack" aria-label="Anteile der größten Ordner">
		{#each shown as e, i (e.name)}
			{@const pct = (e.bytes * 100) / total}
			{#if pct >= 0.8}
				<div
					class="seg"
					class:dim={hover !== null && hover !== e.name}
					role="img"
					aria-label="{e.name}: {fmtBytes(e.bytes)}"
					style:width="{pct}%"
					style:background={reihe(i)}
					title="{e.name}: {fmtBytes(e.bytes)}"
					onpointerenter={() => {
						hover = e.name;
					}}
					onpointerleave={() => {
						hover = null;
					}}
				></div>
			{/if}
		{/each}
		{#if rest > 0}
			<div class="seg rest" style:width="{(rest * 100) / total}%" title="Sonstiges"></div>
		{/if}
	</div>
	<div class="scroll-x" use:scrollShadow>
		<table class="folders">
			<tbody>
				{#each shown as e, i (e.name)}
					{@const pct = (e.bytes * 100) / total}
					<tr
						class:active={hover === e.name}
						onpointerenter={() => {
							hover = e.name;
						}}
						onpointerleave={() => {
							hover = null;
						}}
					>
						<td>
							<span class="swatch" style:background={reihe(i)}></span>
							{e.is_dir ? '📁' : '📄'}
							{e.name}
						</td>
						<td class="num">{fmtBytes(e.bytes)}</td>
						<td class="num muted small"
							>{#if e.is_dir}{e.files.toLocaleString('de-DE')} Dateien{/if}</td
						>
						<td class="barcell">
							<div class="bar">
								<div class="fill" style:width="{pct}%" style:background={reihe(i)}></div>
							</div>
						</td>
					</tr>
				{/each}
				{#if rest > 0}
					<tr>
						<td
							><span class="swatch rest"></span> Sonstiges ({Math.max(inv.entries.length - 10, 0)} weitere)</td
						>
						<td class="num">{fmtBytes(rest)}</td>
						<td></td>
						<td></td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>
	{#if inv.face.length > 0}
		<h3>Fächer der Chats (<code>Projekte</code>)</h3>
		<p class="muted small">
			Jeder Chat hat ein eigenes Fach für Notizen und Entwürfe. Code bleibt in den Repos.
		</p>
		<ul class="plain faecher">
			{#each inv.face as f (f.name)}
				<li>
					🗂 <b>{f.name}</b>
					<span class="muted"
						>· {fmtSmall(f.bytes)} · {f.files} Datei{f.files === 1 ? '' : 'en'}</span
					>
				</li>
			{/each}
		</ul>
	{/if}
	{#if skipped.length > 0}
		<p class="muted small">
			🔒 Nicht angesehen (privat, Größe nicht gemessen): {skipped.map((s) => s.name).join(', ')}
		</p>
	{/if}
{/if}

<style>
	.stack {
		display: flex;
		height: 18px;
		border-radius: 999px;
		overflow: hidden;
		background: color-mix(in srgb, var(--line) 60%, transparent);
		margin: 8px 0 12px;
	}
	.seg {
		transition:
			opacity 150ms ease,
			width 600ms ease;
	}
	.seg.dim {
		opacity: 0.4;
	}
	.seg.rest,
	.swatch.rest {
		background: var(--line);
	}
	.swatch {
		display: inline-block;
		width: 10px;
		height: 10px;
		border-radius: 3px;
		margin-right: 6px;
		vertical-align: middle;
	}
	.folders td {
		padding: 4px 8px;
	}
	.folders tr.active td {
		background: var(--accent-soft);
	}
	.num {
		text-align: right;
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}
	.barcell {
		width: 30%;
	}
	.bar {
		height: 8px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--line) 60%, transparent);
		overflow: hidden;
	}
	.fill {
		height: 100%;
		border-radius: 999px;
	}
	.faecher {
		columns: 2;
		column-gap: 24px;
	}
	.faecher li {
		break-inside: avoid;
		padding: 2px 0;
	}
	@media (max-width: 37.5rem) {
		.faecher {
			columns: 1;
		}
	}
</style>
