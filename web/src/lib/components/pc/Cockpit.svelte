<script lang="ts">
	/**
	 * PC-Übersicht für die Startseite: drei Ringe, zwei Live-Kurven, Ampeln.
	 * Erste Werte kommen aus /api/pc, danach alle 5 s über den Live-Kanal.
	 * Schlüssel und Benutzerordner kommen aus /api/pc/benutzer (`ladung`).
	 */
	import { resolve } from '$app/paths';
	import type { PcReport } from '$vertrag';
	import { live } from '#lib/live.svelte.ts';
	import { freierKanal } from '#lib/freikanal.svelte.ts';
	import { fmtBytes, fmtUptime } from '#lib/format.ts';
	import {
		backupVerdict,
		benutzerdaten,
		benutzerHinweis,
		cpuVerlauf,
		defenderVerdict,
		keysVerdict,
		percent,
		rebootVerdict,
		schluesselAusLadung,
		type BenutzerLadung
	} from '#lib/pc.ts';
	import Ring from './Ring.svelte';
	import Sparkline from './Sparkline.svelte';
	import StatusDot from '../StatusDot.svelte';

	interface Props {
		report: PcReport;
		/**
		 * Benutzer-Daten (`ladeBenutzer`). Fehlt sie ganz (nicht geladen),
		 * entfallen die Schlüssel- und die Ordner-Kachel.
		 */
		ladung?: BenutzerLadung;
	}
	let { report, ladung }: Props = $props();

	// Live-Werte überschreiben die Momentaufnahme, Kurven wachsen mit.
	let cpuHist = $state<number[]>([]);
	let memHist = $state<number[]>([]);
	let seeded = $state<string | null>(null);
	$effect(() => {
		const key = report.snapshot.taken_at ?? '';
		if (seeded === key) return;
		seeded = key;
		// Etappe 3a+: unbekannte Messungen (erste, GetSystemTimes scheiterte) sind eine Lücke, keine 0.
		cpuHist = cpuVerlauf(report.snapshot.history);
		memHist = report.snapshot.history.map((s) => s.mem);
	});
	// PC-Werte: aus dem geschützten Kanal, sonst aus dem freien (Kopfleiste, Fassung 2).
	const tick = $derived(live.pc ?? freierKanal.pc);
	let lastTick = $state<string | null>(null);
	$effect(() => {
		const t = tick;
		if (!t || t.at === lastTick) return;
		lastTick = t.at;
		// Die erste Messung (Belegung unbekannt) übernimmt die Kurve nicht als 0.
		if (t.cpu_belegung !== 'unbekannt') cpuHist = [...cpuHist.slice(-179), t.cpu_percent];
		memHist = [...memHist.slice(-179), percent(t.mem_used, t.mem_total)];
	});

	const cpu = $derived(tick?.cpu_percent ?? report.snapshot.cpu_percent);
	const memUsed = $derived(tick?.mem_used ?? report.snapshot.mem_used);
	const memTotal = $derived(tick?.mem_total ?? report.snapshot.mem_total);
	const disk = $derived((tick?.disks ?? report.snapshot.disks)[0] ?? null);
	const uptime = $derived(tick?.uptime_secs ?? report.snapshot.uptime_secs);
	// Die Stufe der Belegung rechnet der Server (Schwellen in belegung_schwellen).
	const cpuBelegung = $derived(tick?.cpu_belegung ?? report.snapshot.cpu_belegung);
	const memBelegung = $derived(tick?.mem_belegung ?? report.snapshot.mem_belegung);

	const backup = $derived(backupVerdict(report.backup_stand));
	const defender = $derived(defenderVerdict(report.snapshot.defender));
	const reboot = $derived(rebootVerdict(report.snapshot.neustart));
	const b = $derived(ladung?.benutzer ?? null);
	const stand = $derived(ladung ? benutzerdaten(ladung) : null);
	const schluessel = $derived(ladung ? schluesselAusLadung(ladung) : null);
	const keys = $derived(schluessel ? keysVerdict(schluessel) : null);
	const ready = $derived(report.snapshot.taken_at !== null);
</script>

<section class="cockpit card" aria-label="Wie geht es dem PC">
	<div class="head">
		<h2>Wie geht es dem PC?</h2>
		<span class="muted small">
			<!-- Ohne den Namen des Rechners: Der Server liefert dort nur einen Platzhalter. -->
			{report.snapshot.os}
			{#if ready}· läuft seit {fmtUptime(uptime)}{/if}
		</span>
	</div>
	{#if !ready}
		<p class="muted">Die ersten Messwerte kommen gleich …</p>
	{/if}
	<div class="grid">
		<div class="rings">
			<Ring
				percent={cpu}
				belegung={cpuBelegung}
				label="Prozessor"
				value="{report.snapshot.cpu_count} Kerne"
			/>
			<Ring
				percent={percent(memUsed, memTotal)}
				belegung={memBelegung}
				label="Arbeitsspeicher"
				value="{fmtBytes(memUsed)} / {fmtBytes(memTotal)}"
			/>
			{#if disk}
				<Ring
					percent={percent(disk.used, disk.total)}
					belegung={disk.belegung}
					label="Platte {disk.mount.replace('\\', '')}"
					value="{fmtBytes(disk.total - disk.used)} frei"
				/>
			{/if}
		</div>
		<div class="sparks">
			<Sparkline values={cpuHist} label="Prozessor, letzte 15 Minuten" />
			<Sparkline values={memHist} label="Arbeitsspeicher, letzte 15 Minuten" />
		</div>
	</div>
	<div class="badges">
		<a class="pill" href={resolve('/pc')} title={report.data.backup.note}>
			<StatusDot bedeutung={backup.bedeutung} decorative /> Backup: {backup.text}
		</a>
		<!-- data-*: die Fachgröße des Servers, für Tests und die Edge-Probe (pc-edge.mjs). -->
		<span
			class="pill"
			data-pruefung="defender"
			data-befund={report.snapshot.defender.befund}
			data-bedeutung={defender.bedeutung}
			><StatusDot bedeutung={defender.bedeutung} decorative /> {defender.text}</span
		>
		<span
			class="pill"
			data-pruefung="neustart"
			data-neustart={report.snapshot.neustart.neustart}
			data-bedeutung={reboot.bedeutung}
			><StatusDot bedeutung={reboot.bedeutung} decorative /> {reboot.text}</span
		>
		{#if stand && keys && schluessel}
			{#if b}
				<a
					class="pill"
					href={resolve('/schluessel')}
					data-pruefung="schluessel"
					data-vollstaendig={schluessel.vollstaendig}
					data-bedeutung={keys.bedeutung}
				>
					<StatusDot bedeutung={keys.bedeutung} decorative />
					{keys.text}
				</a>
			{:else}
				<span
					class="pill"
					title={benutzerHinweis(stand, null)}
					data-pruefung="schluessel"
					data-vollstaendig={schluessel.vollstaendig}
					data-bedeutung={keys.bedeutung}
				>
					<StatusDot bedeutung={keys.bedeutung} decorative />
					{keys.text}
				</span>
			{/if}
		{/if}
		{#if b?.inventory.computed_at && !b.inventory.hinweis}
			<span class="pill">
				📁 {fmtBytes(b.inventory.entries.reduce((n, e) => n + e.bytes, 0))} im Benutzerordner
			</span>
		{/if}
		<a class="pill more" href={resolve('/pc')}>Alles zum PC →</a>
	</div>
</section>

<style>
	.cockpit {
		background: linear-gradient(
			135deg,
			color-mix(in srgb, var(--accent-soft) 60%, var(--card)),
			var(--card) 60%
		);
	}
	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		flex-wrap: wrap;
	}
	.grid {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 18px 28px;
		align-items: center;
	}
	.rings {
		display: flex;
		gap: 18px;
		flex-wrap: wrap;
	}
	.sparks {
		display: grid;
		gap: 10px;
		min-width: 0;
	}
	.badges {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 14px;
	}
	/* Lange Sätze (Defender, Neustart) brechen am Handy um, statt die Seite
	   breiter zu machen (Edge-Probe 3a-1: 586 px bei 375 px Breite). */
	.badges .pill {
		max-width: 100%;
		white-space: normal;
	}
	a.pill {
		color: var(--text);
	}
	a.pill:hover {
		text-decoration: none;
		background: var(--accent-soft);
	}
	.more {
		margin-left: auto;
		color: var(--accent);
		font-weight: 600;
	}
	@media (max-width: 47.5rem) {
		.grid {
			grid-template-columns: 1fr;
		}
	}
</style>
