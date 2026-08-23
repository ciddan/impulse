<script lang="ts">
	import { openUrl } from "@tauri-apps/plugin-opener";
	import { app } from "$lib/state.svelte";
</script>

{#if app.pendingUpdate}
	<div class="banner update">
		<strong>Update available:</strong> Impulse {app.pendingUpdate.version}
		<button onclick={() => app.installUpdate()} disabled={app.updating}>
			{app.updating ? "Installing…" : "Install & restart"}
		</button>
	</div>
{/if}

{#if app.loadError}
	<div class="banner error">{app.loadError}</div>
{/if}

{#if app.status && !app.status.eapo.installed}
	<div class="banner warn">
		<strong>Equalizer APO is not installed.</strong>
		<p>
			Impulse requires Equalizer APO to process audio. Install it, enable it on your playback
			device, and restart — Impulse takes over from there.
		</p>
		<button onclick={() => openUrl("https://sourceforge.net/projects/equalizerapo/")}>
			Download Equalizer APO
		</button>
	</div>
{:else if app.status && !app.status.eapo.include_installed}
	<div class="banner warn">
		<strong>One step left:</strong> hook Impulse into Equalizer APO's config.
		<button onclick={() => app.setupInclude()} disabled={app.busy}>Set up now</button>
	</div>
{/if}
