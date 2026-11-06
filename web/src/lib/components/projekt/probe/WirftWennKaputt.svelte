<script lang="ts">
	/** Nur für Tests: wirft beim Zeichnen, solange `schalter.kaputt` gilt. */
	import { schalter } from './schalter.ts';

	const text = (): string => {
		if (schalter.kaputt) throw new Error('noch kaputt');
		return 'wieder heil';
	};
</script>

<p>{text()}</p>
