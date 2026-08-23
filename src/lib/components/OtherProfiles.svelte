<script lang="ts">
	import { app } from "$lib/state.svelte";

	const status = $derived(app.status!);
	const others = $derived(
		status.devices.filter((d) => d.assignment && d.guid !== app.selected?.guid),
	);
</script>

{#if others.length || status.offline_assignments.length}
	<section class="card">
		<h2>Other active profiles</h2>
		<ul class="assigned">
			{#each others as d (d.guid)}
				<li>
					<div>
						<span class="profile-name">{d.assignment?.name}</span>
						<span class="dim">on {d.name}</span>
					</div>
					<button class="remove" disabled={app.busy} onclick={() => app.clear(d.guid)}>
						Remove
					</button>
				</li>
			{/each}
			{#each status.offline_assignments as o (o.device_guid)}
				<li class="offline">
					<div>
						<span class="profile-name">{o.assignment.name}</span>
						<span class="dim">on {o.device_name} (disconnected)</span>
					</div>
					<button class="remove" disabled={app.busy} onclick={() => app.clear(o.device_guid)}>
						Remove
					</button>
				</li>
			{/each}
		</ul>
	</section>
{/if}

<style>
	.assigned {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.assigned li {
		display: flex;
		justify-content: space-between;
		align-items: center;
		background: var(--elev);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 8px 14px;
	}
	.assigned li.offline {
		opacity: 0.65;
	}
</style>
