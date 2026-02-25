<script lang="ts">
	import FileUploadButton from './FileUploadButton.svelte';

	interface Props {
		onSend: (text: string) => void;
		onFileUpload: (file: File) => void;
		disabled?: boolean;
	}

	let { onSend, onFileUpload, disabled = false }: Props = $props();

	let text = $state('');

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			send();
		}
	}

	function send() {
		const trimmed = text.trim();
		if (!trimmed) return;
		onSend(trimmed);
		text = '';
	}
</script>

<div class="input-bar">
	<FileUploadButton onFile={onFileUpload} {disabled} />

	<textarea
		bind:value={text}
		onkeydown={handleKeydown}
		placeholder="Type a message..."
		rows="1"
		{disabled}
	></textarea>

	<button
		class="btn-primary send-btn"
		onclick={send}
		disabled={disabled || !text.trim()}
	>
		Send
	</button>
</div>

<style>
	.input-bar {
		display: flex;
		align-items: flex-end;
		gap: 8px;
		padding: 12px 16px;
		border-top: 1px solid var(--border);
		background: var(--bg);
	}

	textarea {
		flex: 1;
		resize: none;
		min-height: 38px;
		max-height: 120px;
		font-family: inherit;
	}

	.send-btn {
		padding: 10px 20px;
		flex-shrink: 0;
	}
</style>
