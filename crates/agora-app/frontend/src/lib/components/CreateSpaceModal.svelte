<script lang="ts">
	import { api } from '$lib/api';
	import { rooms } from '$lib/stores/rooms';
	import { goto } from '$app/navigation';

	interface Props {
		onClose: () => void;
	}

	let { onClose }: Props = $props();

	let name = $state('');
	let topic = $state('');
	let iconFile: File | null = $state(null);
	let error = $state('');
	let loading = $state(false);

	function handleIconChange(e: Event) {
		const input = e.target as HTMLInputElement;
		iconFile = input.files?.[0] ?? null;
	}

	async function handleCreate() {
		error = '';
		loading = true;
		try {
			const resp = await api.createSpace(name || undefined, topic || undefined);
			rooms.addRoom(resp.room_id, name || '(unnamed)', 'm.space');

			if (iconFile) {
				const mxcUri = await api.uploadFile(iconFile);
				await api.setState(resp.room_id, 'm.room.avatar', '', { url: mxcUri });
			}

			onClose();
			goto(`/rooms/${encodeURIComponent(resp.room_id)}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create space';
		} finally {
			loading = false;
		}
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="overlay" onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} role="dialog" tabindex="-1">
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
	<div class="modal" onclick={(e) => e.stopPropagation()} role="document">
		<h3>Create Space</h3>

		<form onsubmit={(e) => { e.preventDefault(); handleCreate(); }}>
			<div class="field">
				<label for="space-name">Name</label>
				<input id="space-name" type="text" bind:value={name} placeholder="My Space" />
			</div>

			<div class="field">
				<label for="space-topic">Topic (optional)</label>
				<input id="space-topic" type="text" bind:value={topic} placeholder="A group of rooms" />
			</div>

			<div class="field">
				<label for="space-icon">Icon (optional)</label>
				<input id="space-icon" type="file" accept="image/*" onchange={handleIconChange} />
			</div>

			{#if error}
				<p class="error">{error}</p>
			{/if}

			<div class="actions">
				<button type="button" class="btn-secondary" onclick={onClose}>Cancel</button>
				<button type="submit" class="btn-primary" disabled={loading}>
					{loading ? 'Creating...' : 'Create Space'}
				</button>
			</div>
		</form>
	</div>
</div>

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: var(--overlay);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
	}

	.modal {
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 24px;
		width: 100%;
		max-width: 400px;
		box-shadow: 0 8px 32px var(--shadow);
	}

	.modal h3 {
		font-size: 1.1rem;
		font-weight: 600;
		margin-bottom: 20px;
	}

	.field {
		margin-bottom: 14px;
	}

	.field label {
		display: block;
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--text-secondary);
		margin-bottom: 6px;
	}

	.field input[type='file'] {
		font-size: 0.8rem;
		color: var(--text-secondary);
	}

	.error {
		color: var(--danger);
		font-size: 0.8rem;
		margin-bottom: 12px;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 20px;
	}
</style>
