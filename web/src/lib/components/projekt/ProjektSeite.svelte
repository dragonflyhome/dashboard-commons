<script lang="ts">
	/**
	 * Die Seite eines Projekts: Seite.svelte im Rahmen des Projekts. Steht
	 * unter `/projects/<slug>` und, eingebettet, als Teil auf der Gruppenseite
	 * `/gruppen/<slug>` (Entscheidung projektgruppe). Eingebettet
	 * fehlen, was die Gruppenseite selbst trägt oder nur einmal je Seite geben
	 * darf: Tab-Titel, Überschrift (der Kopf des Teils nennt Titel und Status)
	 * und das Inhaltsverzeichnis.
	 */
	import NeueFormRahmen from '#lib/components/projekt/NeueFormRahmen.svelte';
	import NeueFormSeite from '#lib/components/projekt/NeueFormSeite.svelte';
	import type { ProjectDetail, ProjektAntwort } from '$vertrag';

	interface Props {
		data: ProjectDetail & { kopf: ProjektAntwort };
		/** Als Teil der Gruppenseite. */
		eingebettet?: boolean;
	}
	let { data, eingebettet = false }: Props = $props();
</script>

<NeueFormRahmen kopf={data.kopf} aktiv={null} {eingebettet}>
	<NeueFormSeite kopf={data.kopf} />
</NeueFormRahmen>
