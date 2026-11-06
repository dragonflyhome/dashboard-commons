<script lang="ts">
	/** Nur für Tests: wirft beim Zeichnen. */
	const kaputt = (): string => {
		throw new Error('absichtlich kaputt');
	};
</script>

<p>{kaputt()}</p>
