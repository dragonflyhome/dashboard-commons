<script lang="ts">
	/**
	 * Abschnitt „Dashboard“ auf /pc (Kopfleiste, Fassung 2, Ziel des
	 * Live-Punkts, `/pc#dashboard`): wie es dem Dashboard selbst geht. Der
	 * Live-Kanal (freier und geschützter), der Bote, die Fassung von Server
	 * und Oberfläche und ob sich die Ordner lesen lassen (Startprüfung aus
	 * Hotfix 4d). Alles aus `/api/betrieb` (frei, ohne Pfade) und dem Kanal;
	 * neu geladen bei einer neuen Fassung und jede Minute. Angemeldet kommt
	 * der volle Bericht `/api/betrieb/voll` (geschützt, Abnahme): dazu
	 * die Einstellungsdatei, seit wann der Bote verbunden ist und ob der Inhaber an
	 * der Konsole angemeldet ist. Ohne Anmeldung steht davon nichts da.
	 *
	 * `laden` nur für Tests; sonst `/api/betrieb` oder `/api/betrieb/voll`.
	 */
	import { onMount } from 'svelte';
	import { fehlerText } from '#lib/api/client.ts';
	import { fmtDateTime } from '#lib/format.ts';
	import { freierKanal } from '#lib/freikanal.svelte.ts';
	import { live } from '#lib/live.svelte.ts';
	import { LIVE_TEXT, liveFarbe, type LiveZustand } from '#lib/verbindung.ts';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { profilVerbergen } from '#lib/pfade.ts';
	import type { Bedeutung, Betrieb, OrdnerStand } from '$vertrag';

	interface Props {
		laden?: () => Promise<Betrieb>;
		/** Für Tests: der Zustand des Live-Punkts. */
		zustand?: LiveZustand | null;
	}
	let { laden = holen, zustand: vorgegeben }: Props = $props();

	async function holen(): Promise<Betrieb> {
		const voll = freierKanal.geschuetztOk;
		const res = await fetch(voll ? '/api/betrieb/voll' : '/api/betrieb', {
			headers: { accept: 'application/json' }
		});
		if (!res.ok) throw new Error(`Der Server antwortet mit ${res.status}.`);
		return (await res.json()) as Betrieb;
	}

	let b = $state<Betrieb | null>(null);
	let fehler = $state<string | null>(null);
	const zustand = $derived(vorgegeben === undefined ? freierKanal.zustand : vorgegeben);

	async function neu(): Promise<void> {
		try {
			b = await laden();
			fehler = null;
		} catch (e) {
			fehler = fehlerText(e);
		}
	}

	const FARBE: Record<'ok' | 'wartet' | 'problem', Bedeutung> = {
		ok: 'ok',
		wartet: 'wartet',
		problem: 'problem'
	};

	function boteBedeutung(bote: Betrieb['bote']): Bedeutung {
		return bote === 'verbunden' ? 'ok' : bote === 'getrennt' ? 'problem' : 'neutral';
	}

	function boteText(x: Betrieb): string {
		if (x.bote === 'verbunden')
			return x.bote_seit ? `verbunden seit ${fmtDateTime(x.bote_seit)}` : 'verbunden';
		if (x.bote === 'getrennt')
			return x.bote_seit ? `getrennt, zuletzt erreichbar ${fmtDateTime(x.bote_seit)}` : 'getrennt';
		// Frei nur „vorgesehen“: ob er läuft, verriete die Anmeldung des Inhabers (nur angemeldet sichtbar).
		if (x.bote === 'vorgesehen') return 'vorgesehen; ob er läuft, steht nur angemeldet hier';
		return 'nicht vorgesehen (ohne Einstellungsdatei)';
	}

	function ordnerBedeutung(o: OrdnerStand): Bedeutung {
		return o.stand === 'lesbar' ? 'ok' : o.stand === 'fehlt' ? 'neutral' : 'problem';
	}

	/** Angemeldet: ob der Inhaber an der Konsole angemeldet ist (nur im vollen Bericht). */
	function konsoleText(x: Betrieb): string | null {
		const l = x.sitzung?.inhaber_angemeldet;
		if (l === undefined || l === null) return null;
		return l ? 'Du bist an diesem PC angemeldet' : 'Du bist an diesem PC nicht angemeldet';
	}

	// Beim Wechsel zwischen angemeldet und nicht angemeldet neu laden (voll oder frei).
	let warVoll: boolean | null = null;
	$effect(() => {
		const voll = freierKanal.geschuetztOk;
		if (warVoll !== null && warVoll !== voll) void neu();
		warVoll = voll;
	});

	onMount(() => {
		void neu();
		const offFassung = freierKanal.onFassung(() => void neu());
		const takt = setInterval(() => void neu(), 60_000);
		return () => {
			offFassung();
			clearInterval(takt);
		};
	});
</script>

<dl class="stand">
	<dt>Live-Kanal</dt>
	<dd>
		{#if zustand === null}
			<StatusDot bedeutung="neutral" decorative /> wird geprüft
		{:else}
			<StatusDot bedeutung={FARBE[liveFarbe(zustand)]} decorative />
			{LIVE_TEXT[zustand]}{live.connected ? '; Vorgänge und Antworten kommen live' : ''}
		{/if}
	</dd>
	{#if b}
		<dt>Bote</dt>
		<dd><StatusDot bedeutung={boteBedeutung(b.bote)} decorative /> {boteText(b)}</dd>
		{#if konsoleText(b)}
			<dt>Sitzung</dt>
			<dd>{konsoleText(b)}</dd>
		{/if}
		{#if b.einstellungen}
			<dt>Einstellungen</dt>
			<dd class="pfad">{profilVerbergen(b.einstellungen)}</dd>
		{/if}
		<dt>Fassung</dt>
		<dd>
			Server {b.version}{b.commit ? ` (${b.commit.slice(0, 7)})` : ''}, gestartet
			{fmtDateTime(b.gestartet)}
		</dd>
		<dt>Letzter Bau</dt>
		<dd>
			Oberfläche {b.fassung === 'build' ? 'aus dem Grundbau' : b.fassung}{b.fassung_gebaut
				? `, gebaut ${fmtDateTime(b.fassung_gebaut)}`
				: ''}
		</dd>
		<dt>Ordner</dt>
		<dd>
			<ul class="plain ordner">
				{#each b.ordner as o (o.name)}
					<li>
						<StatusDot bedeutung={ordnerBedeutung(o)} decorative />
						{o.name}: {profilVerbergen(o.stand)}
					</li>
				{/each}
			</ul>
		</dd>
	{/if}
</dl>
{#if fehler}<p class="block-error">{fehler}</p>{:else if !b}<p class="muted">Lädt …</p>{/if}

<style>
	.stand {
		display: grid;
		grid-template-columns: minmax(6rem, max-content) 1fr;
		gap: 0.5rem 1rem;
		margin: 0;
	}
	dt {
		color: var(--muted);
	}
	dd {
		margin: 0;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.ordner li {
		padding: 0.125rem 0;
	}
	@media (max-width: 37.5rem) {
		.stand {
			grid-template-columns: 1fr;
			gap: 0.125rem;
		}
		dd {
			margin-bottom: 0.5rem;
		}
	}
</style>
