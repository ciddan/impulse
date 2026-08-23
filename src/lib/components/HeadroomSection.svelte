<script lang="ts">
	import { app } from "$lib/state.svelte";
	import SliderRow from "./SliderRow.svelte";

	const status = $derived(app.status!);
</script>

<h2>Headroom</h2>

<SliderRow
	display="{app.headroomShown.toFixed(1)} dB"
	value={app.headroomShown}
	min={-12}
	max={0}
	step={0.5}
	disabled={!status.eapo.installed}
	oninput={(v) => app.nudgeHeadroom(v)}
/>

<div class="hint">
	Extra margin for hot masters and intersample peaks. −3 dB is a comfortable default.
</div>
