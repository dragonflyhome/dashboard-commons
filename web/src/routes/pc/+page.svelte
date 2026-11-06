<script lang="ts">
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { resolve } from '$app/paths';
	import Cockpit from '#lib/components/pc/Cockpit.svelte';
	import Folders from '#lib/components/pc/Folders.svelte';
	import BackupCard from '#lib/components/pc/BackupCard.svelte';
	import SystemApps from '#lib/components/pc/SystemApps.svelte';
	import BenutzerHinweis from '#lib/components/pc/BenutzerHinweis.svelte';
	import SystemMeldungen from '#lib/components/pc/SystemMeldungen.svelte';
	import DashboardStand from '#lib/components/pc/DashboardStand.svelte';
	import { STAND_ANKER } from '#lib/verbindung.ts';
	import Dichtestreifen from '#lib/katalog/zeit/Dichtestreifen.svelte';
	import Verlauf from '#lib/katalog/zeit/Verlauf.svelte';
	import Kanban from '#lib/components/blocks/Kanban.svelte';
	import Confetti from '#lib/components/Confetti.svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import type { VerlaufQuelle } from '$vertrag';
	import type { Zeitraum } from '#lib/zeit/verlauf.ts';
	import { SATZ_VERLAUF_OHNE_ANMELDUNG, hinweiseFuerKasten } from '#lib/zeit/laden.ts';
	import { benutzerdaten, stepsToColumns } from '#lib/pc.ts';
	import Textfassung from '#lib/components/Textfassung.svelte';
	import { dichteText } from '#lib/textfassung.ts';
	import { LaufendesJetzt } from '#lib/zeit/jetzt.svelte.ts';
	import { tagText } from '#lib/zeit/tage.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const r = $derived(data.pc);
	// Benutzer-Daten (Ordner, Programme, Schlüssel) gibt es nur mit Windows-Anmeldung und Boten.
	const b = $derived(data.benutzer);
	const stand = $derived(benutzerdaten(data));
	// Verlauf (Etappe 3a-1): Dichtestreifen wählt Tag oder Zeitraum, der Verlauf folgt.
	const v = $derived(data.verlauf.verlauf);
	let sprung = $state<{ tag: string; nr: number } | null>(null);
	let zeitraum = $state<Zeitraum | null>(null);
	// Der Quellenfilter des Verlaufs gilt auch für die Balken im Dichtestreifen.
	const ausgeblendet = new SvelteSet<VerlaufQuelle>();
	// Heute für die Textfassung des Streifens, wie im Streifen über Mitternacht weiter.
	const uhr = new LaufendesJetzt();
	const steps = $derived(stepsToColumns(r.data.steps));
	const disk = $derived(r.snapshot.disks[0] ?? null);
	const totalBytes = $derived(
		b?.inventory.computed_at && !b.inventory.hinweis
			? b.inventory.entries.reduce((n, e) => n + e.bytes, 0)
			: null
	);

	// Erfolgsmoment: geht ein Schritt auf „fertig“, rieselt kurz Konfetti.
	const doneCount = $derived(
		r.data.steps.filter((s) => s.status.toLowerCase() === 'fertig').length
	);
	let seenDone = $state<number | null>(null);
	let party = $state(0);
	$effect(() => {
		if (seenDone === null) {
			seenDone = doneCount;
			return;
		}
		if (doneCount > seenDone) party += 1;
		seenDone = doneCount;
	});

	const toc = [
		['ressourcen', 'Ressourcen'],
		[STAND_ANKER, 'Dashboard'],
		['dateien', 'Dateien'],
		['backup', 'Backup'],
		['programme', 'Programme'],
		['historie', 'Historie'],
		['fahrplan', 'Fahrplan']
	] as const;
</script>

<svelte:head>
	<title>{seitenTitel('PC')}</title>
</svelte:head>

<Confetti trigger={party} />

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › PC</p>
<h1>Wie geht es dem PC?</h1>
<!-- Etappe A3: Systemmeldungen (Bote fehlt) gehören hierher; Ziel von /pc#meldung-<id>. -->
<SystemMeldungen />
{#if r.data_error}<p class="block-error">{r.data_error}</p>{/if}
<nav class="toc" aria-label="Abschnitte">
	{#each toc as [id, label] (id)}
		<a href="#{id}">{label}</a>
	{/each}
</nav>

<div id="ressourcen"><Cockpit report={r} ladung={data} /></div>

<!-- Kopfleiste, Fassung 2: Ziel des Live-Punkts (/pc#dashboard). -->
<section class="card" id={STAND_ANKER} aria-labelledby="h-dashboard">
	<h2 id="h-dashboard">Dashboard</h2>
	<DashboardStand />
</section>

<section class="card" id="dateien">
	<h2>Wo liegen die Dateien?</h2>
	<BenutzerHinweis {stand} letzterStand={b?.inventory.computed_at ?? null} />
	{#if b}
		<Folders inv={b.inventory} diskTotal={disk?.total ?? 0} />
	{/if}
</section>

<section class="card" id="backup">
	<h2>Backup</h2>
	<BackupCard
		backup={r.data.backup}
		stand={r.backup_stand}
		projects={r.project_backups}
		{totalBytes}
	/>
</section>

<section class="card" id="programme">
	<h2>Systemanwendungen</h2>
	<SystemApps apps={r.sysapps} benutzer={b} {stand} />
</section>

<section class="card" id="historie">
	<h2>Historie dieses PCs</h2>
	{#if data.verlauf.ohneAnmeldung && v !== null}
		<p class="muted small" role="note">{SATZ_VERLAUF_OHNE_ANMELDUNG}</p>
	{/if}
	{#if v === null}
		<p class="block-error">Der Verlauf lässt sich gerade nicht laden: {data.verlauf.fehler}</p>
	{:else}
		<Dichtestreifen
			ereignisse={v.ereignisse}
			aus={ausgeblendet}
			{zeitraum}
			onwaehle={(tag: string) => {
				zeitraum = null;
				sprung = { tag, nr: (sprung?.nr ?? 0) + 1 };
			}}
			onzeitraum={(z: Zeitraum | null) => {
				zeitraum = z;
			}}
		/>
		<!-- Etappe 3b: Textfassung des Streifens (Einträge je Tag), mit demselben Filter. -->
		<Textfassung fassung={dichteText(v.ereignisse, tagText(uhr.wert), ausgeblendet)} />
		<Verlauf
			ereignisse={v.ereignisse}
			aus={ausgeblendet}
			hinweise={hinweiseFuerKasten(v.hinweise, data.verlauf.ohneAnmeldung)}
			{sprung}
			{zeitraum}
			onzeitraumweg={() => {
				zeitraum = null;
			}}
		/>
	{/if}
	<p class="muted small">
		Einträge von Hand pflegt <code>data\pc.toml</code>; Updates, Starts und die Installation kommen
		aus dem Ereignisprotokoll von Windows, dazu Sicherungen, Meldungen und Vorgänge.
	</p>
</section>

<section class="card" id="fahrplan">
	<h2>Geplante nächste Schritte</h2>
	{#if steps.length === 0}
		<p class="muted">Nichts geplant.</p>
	{:else}
		<Kanban columns={steps} />
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
	section,
	#ressourcen {
		margin-top: 16px;
		scroll-margin-top: 70px;
	}
</style>
