<script lang="ts">
	import type { DeviceView } from "$lib/api";

	interface Props {
		devices: DeviceView[];
		selected: DeviceView | null;
		onselect: (guid: string) => void;
	}
	let { devices, selected, onselect }: Props = $props();

	let open = $state(false);

	const selectedIndex = $derived(
		Math.max(
			0,
			devices.findIndex((d) => d.guid === selected?.guid),
		),
	);

	function label(d: DeviceView): string {
		return `${d.name}${d.is_default ? " (default)" : ""}`;
	}
</script>

<svelte:window
	onclick={() => (open = false)}
	onkeydown={(e) => {
		if (e.key === "Escape") open = false;
	}}
/>

<div class="dropdown">
	<button
		class="field"
		aria-haspopup="listbox"
		aria-expanded={open}
		onclick={(e) => {
			e.stopPropagation();
			open = !open;
		}}
	>
		<span>{selected ? label(selected) : ""}</span>
		<svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
			<path
				d="M2 4.2 6 8.2 10 4.2"
				fill="none"
				stroke="currentColor"
				stroke-width="1.4"
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		</svg>
	</button>
	{#if open}
		<!-- WinUI combobox behavior: flyout overlays the field with the
		     selected item aligned over it. -->
		<div class="flyout" role="listbox" style="top: {-(4 + selectedIndex * 38)}px">
			{#each devices as d (d.guid)}
				<button
					class="item"
					class:selected={selected?.guid === d.guid}
					role="option"
					aria-selected={selected?.guid === d.guid}
					onclick={() => {
						onselect(d.guid);
						open = false;
					}}
				>
					{label(d)}
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.dropdown {
		position: relative;
	}
	.field {
		width: 100%;
		box-sizing: border-box;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		background: var(--surface);
		border: 1px solid var(--muted-border);
		border-radius: 4px;
		color: inherit;
		font-family: inherit;
		font-size: 14px;
		font-weight: 600;
		padding: 9px 12px;
		outline: none;
		cursor: pointer;
	}
	.field:focus-visible {
		border-color: var(--accent);
	}
	.field svg {
		color: var(--dim);
		flex-shrink: 0;
	}
	.flyout {
		position: absolute;
		left: 0;
		right: 0;
		background: var(--flyout-bg);
		border: 1px solid var(--flyout-border);
		border-radius: 8px;
		padding: 4px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.36);
		z-index: 50;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.item {
		position: relative;
		display: flex;
		align-items: center;
		width: 100%;
		height: 36px;
		box-sizing: border-box;
		text-align: left;
		background: none;
		border: none;
		color: var(--text);
		font-family: inherit;
		font-size: 14px;
		padding: 0 12px;
		border-radius: 4px;
		cursor: pointer;
	}
	.item:hover {
		background: var(--flyout-hover);
	}
	.item.selected {
		background: var(--flyout-hover);
	}
	.item.selected::before {
		content: "";
		position: absolute;
		left: 2px;
		top: 50%;
		transform: translateY(-50%);
		width: 3px;
		height: 16px;
		border-radius: 2px;
		background: var(--accent);
	}
</style>
