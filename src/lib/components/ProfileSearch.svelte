<script lang="ts">
	import { app } from "$lib/state.svelte";

	const status = $derived(app.status!);
	const device = $derived(app.selected);
</script>

<h2>
	Headphone profile
	{#if device}<span class="for-device">for {device.name}</span>{/if}
</h2>

<input
	type="search"
	placeholder={app.indexLoading
		? "Loading AutoEq database…"
		: `Search ${app.groups.length.toLocaleString()} headphones…`}
	bind:value={app.query}
	disabled={app.indexLoading || !status.eapo.installed}
/>

{#if app.query && app.results.length === 0 && !app.indexLoading}
	<div class="hint">No matches. Try fewer words.</div>
{/if}

{#if !app.query}
	<div class="custom-row">
		<button
			class="secondary"
			disabled={app.busy || !device || !status.eapo.installed}
			onclick={() => app.applyCustom()}
		>
			Load custom impulse response…
		</button>
		{#if device?.assignment?.source === "custom"}
			<span class="dim">
				Loaded: <span class="profile-name">{device.assignment.name}.wav</span>
			</span>
		{/if}
	</div>
{/if}

{#if app.results.length}
	<ul class="results">
		{#each app.results as g (g.name)}
			<li>
				<div class="row">
					<div class="info">
						<span class="name">{g.name}</span>
						<span class="sub">
							{g.variants[0].source} · {g.variants[0].form}
							{#if g.variants.length > 1}
								· <button
									class="link"
									onclick={() =>
										(app.expandedResult = app.expandedResult === g.name ? null : g.name)}
									>{g.variants.length} sources</button
								>
							{/if}
						</span>
					</div>
					<button
						class="apply"
						disabled={app.busy || !device}
						onclick={() => app.apply(g.variants[0])}
					>
						Apply
					</button>
				</div>
				{#if app.expandedResult === g.name}
					<ul class="variants">
						{#each g.variants as v (v.source + v.form)}
							<li>
								<span>{v.source} · {v.form} {v.has_ir ? "" : "(no IR)"}</span>
								<button
									class="apply small"
									disabled={app.busy || !device}
									onclick={() => app.apply(v)}
								>
									Apply
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<style>
	.custom-row {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.results {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
		max-height: 300px;
		overflow-y: auto;
	}
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 7px 10px;
		border-radius: 4px;
	}
	.row:hover {
		background: var(--surface-hover);
	}
	.info {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}
	.name {
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.sub {
		color: var(--dim);
		font-size: 12px;
	}
	.variants {
		list-style: none;
		margin: 0;
		padding: 0 10px 8px 22px;
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 13px;
		color: var(--chip-text);
	}
	.variants li {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
</style>
