<script lang="ts">
	const antwort = await fetch('/api/schluessel');
</script>

<p>{antwort.status}</p>
