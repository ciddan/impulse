<script lang="ts">
	interface Props {
		label?: string;
		/** Formatted value shown between the label and the track. */
		display: string;
		value: number;
		min: number;
		max: number;
		step: number;
		disabled?: boolean;
		oninput: (value: number) => void;
	}
	let {
		label = "",
		display,
		value,
		min,
		max,
		step,
		disabled = false,
		oninput,
	}: Props = $props();

	const fill = $derived(`${(((value - min) / (max - min)) * 100).toFixed(1)}%`);
</script>

<div class="slider-row">
	<span class="label">{label}</span>
	<span class="value">{display}</span>
	<input
		type="range"
		{min}
		{max}
		{step}
		{value}
		{disabled}
		style="--fill:{fill}"
		oninput={(e) => oninput(Number((e.target as HTMLInputElement).value))}
	/>
</div>

<style>
	/* Windows Settings slider row: label left, value then a right-anchored slider */
	.slider-row {
		display: flex;
		align-items: center;
		gap: 14px;
		min-height: 30px;
	}
	.label {
		flex: 1;
		color: var(--text);
		font-size: 14px;
	}
	.value {
		font-variant-numeric: tabular-nums;
		min-width: 52px;
		text-align: right;
		color: var(--text);
	}

	/* Fluent slider: filled track + ringed thumb */
	input[type="range"] {
		flex: 0 0 52%;
		-webkit-appearance: none;
		appearance: none;
		height: 20px;
		background: transparent;
		margin: 0;
	}
	input[type="range"]::-webkit-slider-runnable-track {
		height: 3px;
		border-radius: 1.5px;
		background: linear-gradient(
			to right,
			var(--accent) 0 var(--fill, 50%),
			var(--track) var(--fill, 50%) 100%
		);
	}
	input[type="range"]::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 18px;
		height: 18px;
		margin-top: -7.5px;
		border-radius: 50%;
		background: var(--accent);
		border: 5px solid var(--thumb-ring);
		box-shadow: 0 0 0 1px var(--border);
		box-sizing: border-box;
		cursor: pointer;
	}
	input[type="range"]:disabled {
		opacity: 0.5;
	}
</style>
