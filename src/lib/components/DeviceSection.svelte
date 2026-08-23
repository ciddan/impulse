<script lang="ts">
	import { openConfigurator } from "$lib/api";
	import { app, convolutionActive } from "$lib/state.svelte";
	import DeviceSelect from "./DeviceSelect.svelte";

	const status = $derived(app.status!);
	const device = $derived(app.selected);
</script>

<h2>Output device</h2>

<DeviceSelect
	devices={status.devices}
	selected={device}
	onselect={(guid) => (app.selectedGuid = guid)}
/>

{#if device}
	<div class="meta">
		{#if device.is_default}<span class="chip">default</span>{/if}
		<span>{(device.sample_rate / 1000).toFixed(1)} kHz</span>
		{#if status.eapo.installed}
			{#if device.apo_enabled}
				<span class="chip ok">APO active</span>
			{:else}
				<span class="chip bad">APO not enabled</span>
			{/if}
		{/if}
		{#if device.assignment}
			<span class="profile">
				<span class="profile-name">{device.assignment.name}</span>
				<span class="dim">{device.assignment.source}</span>
				{#if !convolutionActive(device)}
					<span class="chip bad">EQ bypassed — device not at 44.1/48 kHz</span>
				{/if}
			</span>
		{:else}
			<span class="dim">no EQ profile</span>
		{/if}
	</div>
{/if}

{#if device && !device.apo_enabled && status.eapo.installed}
	<div class="hint">
		Equalizer APO isn't enabled on “{device.name}”. Enable it in its device selector
		(one-time, needs an audio restart), then profiles applied here will work.
		<button
			class="link"
			onclick={() => openConfigurator().catch((e) => (app.actionError = String(e)))}
		>
			Open device selector
		</button>
	</div>
{/if}

<style>
	.meta {
		display: flex;
		gap: 8px;
		color: var(--dim);
		font-size: 12px;
		align-items: center;
		flex-wrap: wrap;
	}
	.profile {
		display: flex;
		gap: 6px;
		align-items: baseline;
	}
</style>
