<script lang="ts">
	import Curve from "$lib/Curve.svelte";
	import { app, convolutionActive } from "$lib/state.svelte";
	import Switch from "./Switch.svelte";
	import SliderRow from "./SliderRow.svelte";

	const status = $derived(app.status!);
	const device = $derived(app.selected);
	const curve = $derived(app.curveData);
	const bass = $derived(app.bassShown);

	// Load the curve whenever the selected device's profile changes.
	$effect(() => {
		app.loadCurve();
	});
</script>

{#if device?.assignment}
	<div class="head">
		<h2>
			Response
			<span class="for-device">
				{device.assignment.name}{device.assignment.source === "custom"
					? " (custom IR)"
					: ""} on {device.name}
			</span>
		</h2>
		<button class="remove" disabled={app.busy} onclick={() => app.clear(device.guid)}>
			Remove profile
		</button>
	</div>

	{#if curve}
		<Curve
			points={curve.eq}
			raw={app.rawSeries}
			target={curve.target}
			bassGain={bass.gain}
			bassFc={bass.fc}
			show={app.seriesVisible}
		/>
		<div class="legend">
			{#if app.rawSeries.length}
				<button
					class="legend-item"
					class:off={!app.seriesVisible.raw}
					onclick={() => (app.seriesVisible.raw = !app.seriesVisible.raw)}
					><i class="swatch raw"></i>Raw</button
				>
			{/if}
			{#if curve.target.length}
				<button
					class="legend-item"
					class:off={!app.seriesVisible.target}
					onclick={() => (app.seriesVisible.target = !app.seriesVisible.target)}
					><i class="swatch target"></i>Target</button
				>
			{/if}
			{#if bass.gain !== 0}
				<button
					class="legend-item"
					class:off={!app.seriesVisible.shelf}
					onclick={() => (app.seriesVisible.shelf = !app.seriesVisible.shelf)}
					><i class="swatch shelf"></i>Bass shelf</button
				>
			{/if}
			{#if curve.eq.length}
				<button
					class="legend-item"
					class:off={!app.seriesVisible.main}
					onclick={() => (app.seriesVisible.main = !app.seriesVisible.main)}
					><i class="swatch total"></i>{app.rawSeries.length ? "Corrected" : "Correction"}
					(measured IR)</button
				>
			{/if}
			<span class="spacer"></span>
			{#if curve.smoothed.length}
				<button
					class="legend-item"
					class:off={!app.smoothedView}
					onclick={() => (app.smoothedView = !app.smoothedView)}>Smoothed</button
				>
			{/if}
		</div>
		{#if curve.eq_source === "convolver"}
			<div class="hint">
				Correction measured from the active convolver IR ({curve.ir_preamp_db.toFixed(1)} dB
				built-in gain, shown level-aligned).
			</div>
		{:else if !convolutionActive(device)}
			<div class="hint">
				No convolver is active: AutoEq impulse responses exist for 44.1 and 48 kHz, and
				“{device.name}” is mixing at {(device.sample_rate / 1000).toFixed(1)} kHz. Set the
				device to 44.1 or 48 kHz in Windows sound settings to enable EQ.
			</div>
		{/if}
	{/if}

	<div class="switch-row">
		<span
			class="row-title"
			title="Automatically lowers the pre-gain by the bass boost amount so boosted bass can't clip. Off = you manage headroom yourself."
		>
			Prevent distortion from bass boost
		</span>
		{#if status.compensate_shelf && bass.gain > 0}
			<span class="dim">(volume: {(app.headroomShown - bass.gain).toFixed(1)} dB)</span>
		{/if}
		<span class="switch-spacer"></span>
		<span class="dim">{status.compensate_shelf ? "On" : "Off"}</span>
		<Switch
			checked={status.compensate_shelf}
			label="Prevent distortion from bass boost"
			onclick={() => app.toggleCompensateShelf()}
		/>
	</div>

	<div class="sliders">
		<SliderRow
			label="Bass gain"
			display="{bass.gain > 0 ? '+' : ''}{bass.gain.toFixed(1)} dB"
			value={bass.gain}
			min={-12}
			max={12}
			step={0.5}
			oninput={(v) => app.nudgeBass("gain", v)}
		/>
		<SliderRow
			label="Corner"
			display="{bass.fc.toFixed(0)} Hz"
			value={bass.fc}
			min={50}
			max={350}
			step={5}
			oninput={(v) => app.nudgeBass("fc", v)}
		/>
	</div>

	{#if bass.gain > 0 && !status.compensate_shelf}
		<div class="hint">
			Distortion prevention is off — this +{bass.gain.toFixed(1)} dB boost can distort on loud
			music. Lower the headroom slider to make room for it.
		</div>
	{/if}

	<div class="divider"></div>
{/if}

<style>
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.legend {
		display: flex;
		gap: 16px;
		font-size: 12px;
		color: var(--dim);
		padding: 0 2px;
	}
	.spacer {
		flex: 1;
	}
	.legend-item {
		display: flex;
		align-items: center;
		gap: 6px;
		background: none;
		border: none;
		color: inherit;
		font-size: inherit;
		font-family: inherit;
		padding: 2px 4px;
		cursor: pointer;
		border-radius: 4px;
		transition: opacity 0.12s ease;
	}
	.legend-item:hover {
		background: var(--surface-hover);
	}
	.legend-item.off {
		opacity: 0.38;
	}
	.legend-item.off .swatch {
		background: var(--track) !important;
	}
	.swatch {
		display: inline-block;
		width: 14px;
		height: 3px;
		border-radius: 2px;
	}
	.swatch.raw {
		background: var(--curve-raw);
	}
	.swatch.target {
		background: var(--curve-target);
		opacity: 0.55;
	}
	.swatch.shelf {
		background: var(--curve-shelf);
	}
	.swatch.total {
		background: var(--curve-total);
	}
</style>
