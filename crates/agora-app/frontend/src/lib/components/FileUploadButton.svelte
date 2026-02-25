<script lang="ts">
	interface Props {
		onFile: (file: File) => void;
		disabled?: boolean;
	}

	let { onFile, disabled = false }: Props = $props();

	let fileInput: HTMLInputElement | undefined = $state();

	function handleClick() {
		fileInput?.click();
	}

	function handleChange(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0];
		if (file) {
			onFile(file);
			input.value = '';
		}
	}
</script>

<input
	type="file"
	bind:this={fileInput}
	onchange={handleChange}
	style="display: none"
/>

<button
	class="upload-btn"
	onclick={handleClick}
	{disabled}
	title="Upload file"
>
	<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
		<path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48" />
	</svg>
</button>

<style>
	.upload-btn {
		width: 38px;
		height: 38px;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		background: var(--surface);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 6px;
		flex-shrink: 0;
	}

	.upload-btn:hover:not(:disabled) {
		background: var(--surface-hover);
		color: var(--accent);
	}
</style>
