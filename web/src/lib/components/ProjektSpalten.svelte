<script lang="ts">
	/**
	 * Startseite, Ansicht „Spalten“ (Etappe 3b): die Projekte nach Status
	 * nebeneinander, je Spalte eine Bedeutung (aktiv, in Arbeit, fertig …).
	 * Gruppiert und gezählt wird nur in #lib/projekte.ts (`auswerten`); hier
	 * wird nur gezeigt. Am Handy stehen die Spalten untereinander, nichts
	 * rollt seitlich.
	 */
	import { resolve } from '$app/paths';
	import type { Spalte } from '#lib/projekte.ts';
	import { klasse } from '#lib/bedeutung.ts';
	import StatusDot from './StatusDot.svelte';

	interface Props {
		spalten: Spalte[];
	}
	let { spalten }: Props = $props();
</script>

<div class="spalten">
	{#each spalten as s (s.bedeutung)}
		<section class="spalte {klasse(s.bedeutung)}" aria-labelledby="spalte-{s.bedeutung}">
			<h3 id="spalte-{s.bedeutung}">
				<StatusDot bedeutung={s.bedeutung} decorative />
				<span>{s.wort}</span>
				<span class="zahl muted small" data-anzahl={s.anzahl}
					>{s.anzahl}{#if s.gesamt !== s.anzahl}<span class="nur-vorlesen">
							von {s.gesamt}</span
						><span aria-hidden="true">/{s.gesamt}</span>{/if}</span
				>
			</h3>
			<ul class="plain karten">
				{#each s.karten as k (k.schluessel)}
					<li>
						{#if k.art === 'gruppe'}
							<!-- Projektgruppe: eine Karte, je Teil eine Zeile. -->
							<a class="karte" href={resolve('/gruppen/[slug]', { slug: k.gruppe.slug })}>
								<b class="titel">{k.gruppe.titel}</b>
								{#each k.teile as t (t.slug)}
									<span class="satz muted small">{t.title} · {t.status}</span>
								{/each}
							</a>
						{:else}
							{@const p = k.projekt}
							<a class="karte" href={resolve('/projects/[slug]', { slug: p.slug })}>
								<b class="titel">{p.title}</b>
								{#if p.summary}<span class="satz muted small">{p.summary}</span>{/if}
								<span class="meta muted small"
									>{p.status} · zuletzt {p.last_change}{#if (p.open_zahl ?? p.open.length) > 0}
										· {p.open_zahl ?? p.open.length} offen{/if}</span
								>
							</a>
						{/if}
					</li>
				{/each}
			</ul>
		</section>
	{/each}
</div>

<style>
	.spalten {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 15rem), 1fr));
		gap: 0.75rem;
		align-items: start;
		min-width: 0;
	}
	.spalte {
		min-width: 0;
		padding: 0.6rem;
		border: 1px solid var(--line);
		border-top: 0.25rem solid var(--ton, var(--plan));
		border-radius: var(--radius);
		background: color-mix(in srgb, var(--ton-soft, var(--card)) 60%, var(--bg));
	}
	h3 {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin: 0 0 0.5rem;
		font-size: var(--text-md);
	}
	.zahl {
		margin-left: auto;
		font-variant-numeric: tabular-nums;
	}
	.karten {
		display: grid;
		gap: 0.4rem;
	}
	.karte {
		display: grid;
		gap: 0.15rem;
		min-height: 2.75rem;
		padding: 0.5rem 0.6rem;
		border: 1px solid var(--line);
		border-radius: 0.6rem;
		background: var(--card);
		color: var(--text);
	}
	.karte:hover,
	.karte:focus-visible {
		text-decoration: none;
		box-shadow: var(--shadow-hover);
	}
	.titel {
		overflow-wrap: anywhere;
	}
	.satz {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
</style>
