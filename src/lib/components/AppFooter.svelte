<script lang="ts">
	import { openUrl } from "@tauri-apps/plugin-opener";
	import { app } from "$lib/state.svelte";

	const status = $derived(app.status!);
</script>

<footer>
	<label class="autostart">
		<input type="checkbox" checked={status.autostart} onchange={() => app.toggleAutostart()} />
		Start with Windows (minimized to tray)
	</label>
	<div class="footer-right">
		{#if app.indexAge}
			<span class="dim">Database: {new Date(app.indexAge * 1000).toLocaleDateString()}</span>
		{/if}
		<button class="link" disabled={app.indexLoading} onclick={() => app.loadIndex(true)}>
			Refresh
		</button>
		<span class="dot">·</span>
		<button class="link" onclick={() => openUrl("https://github.com/jaakkopasanen/AutoEq")}>
			AutoEq
		</button>
	</div>
</footer>
