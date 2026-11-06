<script lang="ts">
	/**
	 * Die Seitenleiste (Kopfleiste, Fassung 3; ersetzt das Menü oben): oben
	 * „Anstehend“ (mit der Zahl der Stufe Jetzt), dann die Projekte nach
	 * Schwerpunkt wie auf der Startseite (nicht nach Status), je mit dem Zähler
	 * „wartet“ (aus „Anstehend“, ohne Anmeldung keiner). Das aktuelle Projekt
	 * ist markiert, seine Unterseiten stehen eingerückt darunter. Etappe 4c:
	 * Hat die Seite eines Projekts einen Fehler, wird sie gebaut oder wurde sie
	 * noch nie gebaut, steht eine kleine Marke hinter dem Namen (grün: keine). Abgesetzt
	 * darunter die Dashboard-Seiten; PC steht oben rechts, nicht hier.
	 *
	 * Ab 1024 px steht die Leiste fest links, darunter in der Schublade
	 * (`Schublade` aus dem Katalog). Gruppieren und Ordnen geschieht in
	 * `#lib/projekte.ts`, die Zahlen in `#lib/anstehend.ts`.
	 *
	 * Projektgruppen (Entscheidung projektgruppe): ein Eintrag mit
	 * dem Titel der Gruppe (führt zur Gruppenseite), darunter eingerückt die
	 * Teile, an der Stelle des ersten Teils.
	 */
	import { resolve } from '$app/paths';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { klasse } from '#lib/bedeutung.ts';
	import { ANSTEHEND, SEITEN, type Markiert } from '#lib/kopfleiste.ts';
	import { gruppenSchluessel } from '#lib/gruppen.ts';
	import {
		leistenGruppen,
		leistenUnterseiten,
		type BauLive,
		type LeistenProjekt,
		type LeistenUnterseite
	} from '#lib/projekte.ts';
	import type { GruppeAnzeige, ProjectSummary } from '$vertrag';

	interface Props {
		/** Die Projektliste (frei); `null`: ließ sich nicht laden. */
		projekte: readonly ProjectSummary[] | null;
		/** Projektgruppen aus `/api/projects` (ohne Datei leer). */
		gruppen?: readonly GruppeAnzeige[];
		markiert: Markiert;
		/** Daten der aktuellen Seite (für die Unterseiten des Projekts). */
		daten: unknown;
		/** Zähler „wartet“ je Slug (aus „Anstehend“). */
		wartet?: Readonly<Record<string, number>>;
		/** Etappe 4c: die letzten Ereignisse `bau` (geschützter Kanal). */
		bau?: BauLive;
		/** Einträge der Stufe Jetzt (an „Anstehend“). */
		jetztZahl?: number;
		/** Offene Passkey-Anträge (an „Schlüssel“). */
		antragZahl?: number;
		/** Ein Link wurde gewählt (die Schublade schließt dann). */
		ongewaehlt?: (() => void) | undefined;
	}
	let {
		projekte,
		gruppen: projektGruppen = [],
		markiert,
		daten,
		wartet = {},
		bau = {},
		jetztZahl = 0,
		antragZahl = 0,
		ongewaehlt
	}: Props = $props();

	const gruppen = $derived(leistenGruppen(projekte ?? [], wartet, bau, projektGruppen));
	const aktuell = $derived(
		markiert.projekt === null ? null : (projekte?.find((p) => p.slug === markiert.projekt) ?? null)
	);
	const unterseiten = $derived(markiert.projekt === null ? [] : leistenUnterseiten(daten, aktuell));

	function unterAdresse(slug: string, u: LeistenUnterseite): string {
		switch (u.ziel.art) {
			case 'unterseite':
				return resolve('/projects/[slug]/[name]', { slug, name: u.ziel.name });
			case 'vorschau':
				return resolve('/projects/[slug]/vorschau', { slug });
		}
	}

	const zahlen = $derived<Record<string, { n: number; text: string }>>({
		anstehend: { n: jetztZahl, text: 'jetzt anstehend' },
		schluessel: { n: antragZahl, text: 'offene Passkey-Anträge' }
	});
</script>

{#snippet zahl(n: number, text: string, art: 'achtung' | 'wartet')}
	{#if n > 0}<span class="zahl {art}" title="{n} {text}" aria-hidden="true">{n}</span><span
			class="nur-vorlesen">{`, ${String(n)} ${text}`}</span
		>{/if}
{/snippet}

{#snippet projektEintrag(p: LeistenProjekt)}
	{@const hier = markiert.projekt === p.slug}
	<li>
		<a
			class="eintrag projekt"
			class:hier
			href={resolve('/projects/[slug]', { slug: p.slug })}
			aria-current={hier && markiert.unterseite === null ? 'page' : undefined}
			onclick={() => ongewaehlt?.()}
			><StatusDot bedeutung={p.bedeutung} decorative /><span class="name">{p.titel}</span
			>{#if p.bau !== null && p.bau.zustand !== 'gruen'}<span
					class="bau-marke {klasse(p.bau.bedeutung)}"
					data-bau={p.bau.zustand}
					title={p.bau.zeile}
					aria-hidden="true"
				></span><span class="nur-vorlesen">{`, ${p.bau.wort}`}</span>{/if}{@render zahl(
				p.wartet,
				'warten auf dich',
				'wartet'
			)}</a
		>
		{#if hier && unterseiten.length > 0}
			<!-- eslint-disable svelte/no-navigation-without-resolve -- unterAdresse() baut mit resolve() -->
			<ul class="plain unter" aria-label="Seiten von {p.titel}">
				{#each unterseiten as u (u.schluessel)}
					<li>
						<a
							class="eintrag"
							href={unterAdresse(p.slug, u)}
							aria-current={markiert.unterseite === u.schluessel ? 'page' : undefined}
							onclick={() => ongewaehlt?.()}><span class="name">{u.titel}</span></a
						>
					</li>
				{/each}
			</ul>
			<!-- eslint-enable svelte/no-navigation-without-resolve -->
		{/if}
	</li>
{/snippet}

<nav class="leiste" aria-label="Seiten">
	<ul class="plain oben">
		<li>
			<a
				class="eintrag"
				href={resolve(ANSTEHEND.pfad)}
				aria-current={markiert.seite === ANSTEHEND.schluessel ? 'page' : undefined}
				onclick={() => ongewaehlt?.()}
				><span class="name">{ANSTEHEND.label}</span>{@render zahl(
					zahlen.anstehend?.n ?? 0,
					zahlen.anstehend?.text ?? '',
					'achtung'
				)}</a
			>
		</li>
	</ul>

	<h2 class="ueber">Projekte</h2>
	{#if projekte === null}
		<p class="muted small leer">Die Projekte lassen sich gerade nicht laden.</p>
	{/if}
	{#each gruppen as g (g.name)}
		<section class="gruppe" aria-label={g.name}>
			<h3 class="gruppe-name">{g.name}</h3>
			<ul class="plain">
				<!-- Gruppen mit eigenem Schlüssel: nie gleich einem Projekt-Slug. -->
				{#each g.projekte as p (p.teile ? gruppenSchluessel(p.slug) : p.slug)}
					{#if p.teile}
						<!-- Projektgruppe: ein Eintrag zur Gruppenseite, die Teile eingerückt darunter. -->
						<li>
							<a
								class="eintrag projekt"
								class:hier={markiert.gruppe === p.slug ||
									p.teile.some((t) => t.slug === markiert.projekt)}
								href={resolve('/gruppen/[slug]', { slug: p.slug })}
								aria-current={markiert.gruppe === p.slug ? 'page' : undefined}
								onclick={() => ongewaehlt?.()}
								><StatusDot bedeutung={p.bedeutung} decorative /><span class="name">{p.titel}</span
								>{@render zahl(p.wartet, 'warten auf dich', 'wartet')}</a
							>
							<ul class="plain unter" aria-label="Teile von {p.titel}">
								{#each p.teile as t (t.slug)}
									{@render projektEintrag(t)}
								{/each}
							</ul>
						</li>
					{:else}
						{@render projektEintrag(p)}
					{/if}
				{/each}
			</ul>
		</section>
	{/each}

	<h2 class="ueber abgesetzt">Dashboard</h2>
	<ul class="plain">
		{#each SEITEN as s (s.schluessel)}
			{@const z = zahlen[s.schluessel]}
			<li>
				<a
					class="eintrag"
					href={resolve(s.pfad)}
					aria-current={markiert.seite === s.schluessel ? 'page' : undefined}
					onclick={() => ongewaehlt?.()}
					><span class="name">{s.label}</span>{#if z}{@render zahl(z.n, z.text, 'achtung')}{/if}</a
				>
			</li>
		{/each}
	</ul>
</nav>

<style>
	.leiste {
		/* Bezug für die unsichtbaren Texte zu den Zahlen. */
		position: relative;
		padding: 0.5rem 0.5rem 1rem;
		font-size: var(--text-md);
	}
	.ueber {
		margin: 0.875rem 0.625rem 0.25rem;
		font-size: var(--text-xs);
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--muted);
	}
	.ueber.abgesetzt {
		margin-top: 1rem;
		padding-top: 0.875rem;
		border-top: 1px solid var(--line);
	}
	.gruppe-name {
		margin: 0.625rem 0.625rem 0.125rem;
		font-size: var(--text-xs);
		font-weight: 600;
		color: var(--muted);
	}
	.leer {
		margin: 0.25rem 0.625rem;
	}
	.eintrag {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-height: 2.75rem;
		padding: 0.25rem 0.625rem 0.25rem 0.4375rem;
		border-left: 0.1875rem solid transparent;
		border-radius: 0.5rem;
		color: var(--text);
		text-decoration: none;
	}
	.eintrag:hover {
		background: var(--accent-soft);
	}
	.eintrag:focus-visible {
		outline-offset: -0.1875rem;
	}
	.eintrag[aria-current='page'] {
		background: var(--accent-soft);
		/* --accent hat auf --accent-soft hell nur 4,16:1; --accent-text reicht (kontrast.spec.ts). */
		color: var(--accent-text);
		font-weight: 700;
	}
	/* Das aktuelle Projekt, auch wenn eine seiner Unterseiten offen ist. */
	.projekt.hier {
		font-weight: 700;
		border-left-color: var(--accent);
	}
	.name {
		flex: 1 1 auto;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.unter {
		margin: 0 0 0.25rem 1.5rem;
		border-left: 1px solid var(--line);
	}
	.unter .eintrag {
		min-height: 2.75rem;
		font-size: var(--text-sm);
	}
	/* Etappe 4c: Marke für den Bau-Stand der Seite (Farbe aus der Bedeutung, --ton). */
	.bau-marke {
		flex: none;
		width: 0.5rem;
		height: 0.5rem;
		margin-left: 6px;
		border-radius: 2px;
		background: var(--ton);
	}
	.bau-marke[data-bau='noch_nicht'] {
		background: transparent;
		border: 1px solid var(--ton);
	}
	.zahl {
		flex: none;
		display: inline-block;
		min-width: 1.5em;
		padding: 0 0.35em;
		border-radius: 999px;
		font-size: var(--text-2xs);
		font-weight: 700;
		line-height: 1.5;
		text-align: center;
	}
	.zahl.achtung {
		background: var(--warn);
		color: var(--on-warn);
	}
	.zahl.wartet {
		background: var(--accent-soft);
		color: var(--accent-text);
		border: 1px solid var(--accent);
	}
</style>
