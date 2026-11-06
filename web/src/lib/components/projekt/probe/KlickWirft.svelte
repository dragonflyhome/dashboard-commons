<script lang="ts">
	/** Nur für Tests: ein Knopf, dessen Klick-Funktion wirft. */
	function klick(): void {
		throw new Error('Klick kaputt');
	}
</script>

<button type="button" onclick={klick}>Drück mich</button>
