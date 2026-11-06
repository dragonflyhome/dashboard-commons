<script lang="ts">
	/**
	 * Eine Datei aus `daten/` im Standardaufbau: lädt sie über `$projekt` und
	 * zeigt sie mit dem passenden Baustein aus `$katalog`; eigene Daten
	 * (`eigen-<name>`) als JSON zum Aufklappen.
	 */
	import { ladeDaten } from '$projekt';
	import {
		Ampel,
		Fluss,
		Fortschritt,
		Fragen,
		Fristenliste,
		Gantt,
		Kanban,
		Kennzahlen,
		Orte,
		Zeitstrahl
	} from '$katalog';
	import type {
		Ampel as AmpelDaten,
		Fluss as FlussDaten,
		Fortschritt as FortschrittDaten,
		Fragen as FragenDaten,
		Fristen as FristenDaten,
		Gantt as GanttDaten,
		Kanban as KanbanDaten,
		Kennzahlen as KennzahlenDaten,
		Orte as OrteDaten,
		Zeitstrahl as ZeitstrahlDaten
	} from '$vertrag';
	import type { Geladen } from '$projekt';

	interface Props {
		/** Dateiname ohne `.toml`. */
		name: string;
	}
	let { name }: Props = $props();

	// svelte-ignore state_referenced_locally (der Name ändert sich für eine Komponente nicht, die Liste ist nach Namen geschlüsselt)
	const daten: Geladen<unknown> = ladeDaten(name);
	/** Dieselben Daten unter dem Typ, den der Name verspricht (der Server hat sie geprüft). */
	function als<T>(): Geladen<T> {
		return daten as Geladen<T>;
	}
</script>

{#if name === 'kennzahlen'}
	<Kennzahlen daten={als<KennzahlenDaten>()} />
{:else if name === 'ampel'}
	<Ampel daten={als<AmpelDaten>()} />
{:else if name === 'fortschritt'}
	<Fortschritt daten={als<FortschrittDaten>()} />
{:else if name === 'zeitstrahl'}
	<Zeitstrahl daten={als<ZeitstrahlDaten>()} />
{:else if name === 'gantt'}
	<Gantt daten={als<GanttDaten>()} />
{:else if name === 'kanban'}
	<Kanban daten={als<KanbanDaten>()} />
{:else if name === 'fluss'}
	<Fluss daten={als<FlussDaten>()} />
{:else if name === 'fragen'}
	<Fragen daten={als<FragenDaten>()} />
{:else if name === 'orte'}
	<Orte daten={als<OrteDaten>()} />
{:else if name === 'fristen'}
	<Fristenliste daten={als<FristenDaten>()} />
{:else if daten.zustand === 'fehler'}
	<p class="block-error">{daten.satz}</p>
{:else if daten.zustand === 'laedt'}
	<p class="muted small" aria-busy="true">Daten werden geladen …</p>
{:else}
	<details>
		<summary><code>daten/{name}.toml</code></summary>
		<pre class="json">{JSON.stringify(daten.inhalt, null, 2)}</pre>
	</details>
{/if}

<style>
	summary {
		cursor: pointer;
	}
	.json {
		font-family: var(--mono);
		font-size: var(--text-sm);
		overflow: auto;
		max-height: 360px;
		padding: 8px 10px;
		border: 1px solid var(--line);
		border-radius: 8px;
	}
</style>
