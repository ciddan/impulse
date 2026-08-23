<script lang="ts">
	import { onMount } from "svelte";
	import { app } from "$lib/state.svelte";
	import Banners from "$lib/components/Banners.svelte";
	import Switch from "$lib/components/Switch.svelte";
	import DeviceSection from "$lib/components/DeviceSection.svelte";
	import ProfileSearch from "$lib/components/ProfileSearch.svelte";
	import ResponseSection from "$lib/components/ResponseSection.svelte";
	import HeadroomSection from "$lib/components/HeadroomSection.svelte";
	import OtherProfiles from "$lib/components/OtherProfiles.svelte";
	import AppFooter from "$lib/components/AppFooter.svelte";

	onMount(() => app.init());

	// Solid background when the Mica backdrop isn't rendering (old Windows or
	// transparency effects disabled).
	$effect(() => {
		if (app.status) document.body.classList.toggle("solid", !app.status.mica);
	});

	// Adopt the Windows accent color (light/dark variants drive fills per theme).
	$effect(() => {
		const status = app.status;
		if (!status?.accent) return;
		const root = document.documentElement.style;
		if (status.accent_light) root.setProperty("--sys-accent-light", status.accent_light);
		if (status.accent_dark) root.setProperty("--sys-accent-dark", status.accent_dark);
	});
</script>

<main>
	<Banners />

	{#if app.status}
		<section class="card">
			<div class="switch-row">
				<span class="row-title">Equalization</span>
				<span class="switch-spacer"></span>
				<span class="dim">{app.status.master_enabled ? "On" : "Off"}</span>
				<Switch
					checked={app.status.master_enabled}
					label="Enable equalization"
					disabled={app.busy || !app.status.eapo.installed}
					onclick={() => app.toggleMaster()}
				/>
			</div>

			<div class="divider"></div>

			<DeviceSection />

			<div class="divider"></div>

			<ProfileSearch />
		</section>

		<section class="card">
			<ResponseSection />
			<HeadroomSection />
		</section>

		<OtherProfiles />

		{#if app.actionError}
			<div class="banner error">{app.actionError}</div>
		{/if}

		<AppFooter />
	{:else if !app.loadError}
		<div class="loading">Loading…</div>
	{/if}
</main>
