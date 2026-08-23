<script lang="ts">
	interface Props {
		checked: boolean;
		/** Accessible name; the visible label lives in the surrounding row. */
		label: string;
		disabled?: boolean;
		onclick: () => void;
	}
	let { checked, label, disabled = false, onclick }: Props = $props();
</script>

<button
	class="switch"
	class:on={checked}
	role="switch"
	aria-checked={checked}
	aria-label={label}
	{disabled}
	{onclick}
>
	<span class="knob"></span>
</button>

<style>
	/* Fluent toggle switch */
	.switch {
		position: relative;
		display: inline-block;
		width: 40px;
		height: 20px;
		flex-shrink: 0;
		border-radius: 10px;
		border: 1px solid var(--toggle-border);
		background: transparent;
		cursor: pointer;
		transition:
			background 0.15s ease,
			border-color 0.15s ease;
		padding: 0;
		box-sizing: border-box;
	}
	.switch.on {
		background: var(--accent);
		border-color: var(--accent);
	}
	.switch:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.knob {
		position: absolute;
		top: 3px;
		left: 3px;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: var(--knob-off);
		transition:
			transform 0.15s ease,
			background 0.15s ease;
	}
	.switch.on .knob {
		background: var(--knob);
		transform: translateX(20px);
	}
</style>
