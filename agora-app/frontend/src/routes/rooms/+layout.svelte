<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { auth } from '$lib/stores/auth';
	import { sync } from '$lib/stores/sync';
	import { rooms } from '$lib/stores/rooms';
	import RoomList from '$lib/components/RoomList.svelte';
	import CreateRoomModal from '$lib/components/CreateRoomModal.svelte';
	import CreateSpaceModal from '$lib/components/CreateSpaceModal.svelte';
	import JoinRoomModal from '$lib/components/JoinRoomModal.svelte';
	import SpaceSettingsModal from '$lib/components/SpaceSettingsModal.svelte';
	import AddRoomToSpaceModal from '$lib/components/AddRoomToSpaceModal.svelte';
	import SettingsModal from '$lib/components/SettingsModal.svelte';

	let { children } = $props();

	let showCreateModal = $state(false);
	let showCreateSpaceModal = $state(false);
	let showJoinModal = $state(false);
	let showSettings = $state(false);
	let spaceSettingsId: string | null = $state(null);
	let addRoomToSpaceId: string | null = $state(null);

	let authState = $state({ token: null as string | null, userId: null as string | null, loading: false });
	auth.subscribe((v) => (authState = v));

	onMount(() => {
		if (!authState.token) {
			goto('/auth/login');
			return;
		}
		sync.start();
		return () => sync.stop();
	});

	async function handleLogout() {
		sync.stop();
		rooms.clear();
		await auth.logout();
		goto('/auth/login');
	}
</script>

<div class="app-shell">
	<aside class="sidebar">
		<RoomList
			onCreateRoom={() => (showCreateModal = true)}
			onCreateSpace={() => (showCreateSpaceModal = true)}
			onJoinRoom={() => (showJoinModal = true)}
			onSpaceSettings={(id) => (spaceSettingsId = id)}
			onAddRoomToSpace={(id) => (addRoomToSpaceId = id)}
		/>

		<div class="sidebar-footer">
			<span class="user-id">{authState.userId ?? ''}</span>
			<div class="sidebar-actions">
				<button class="footer-btn" onclick={() => (showSettings = true)} title="Settings">&#9881;</button>
				<button class="footer-btn logout" onclick={handleLogout}>Logout</button>
			</div>
		</div>
	</aside>

	<main class="content">
		{@render children()}
	</main>
</div>

{#if showCreateModal}
	<CreateRoomModal onClose={() => (showCreateModal = false)} />
{/if}

{#if showCreateSpaceModal}
	<CreateSpaceModal onClose={() => (showCreateSpaceModal = false)} />
{/if}

{#if showJoinModal}
	<JoinRoomModal onClose={() => (showJoinModal = false)} />
{/if}

{#if spaceSettingsId}
	<SpaceSettingsModal spaceId={spaceSettingsId} onClose={() => (spaceSettingsId = null)} />
{/if}

{#if addRoomToSpaceId}
	<AddRoomToSpaceModal spaceId={addRoomToSpaceId} onClose={() => (addRoomToSpaceId = null)} />
{/if}

{#if showSettings}
	<SettingsModal onClose={() => (showSettings = false)} />
{/if}

<style>
	.app-shell {
		display: flex;
		height: 100vh;
		overflow: hidden;
	}

	.sidebar {
		width: 280px;
		min-width: 280px;
		display: flex;
		flex-direction: column;
		background: var(--bg-secondary);
		border-right: 1px solid var(--border);
	}

	.sidebar-footer {
		padding: 10px 16px;
		border-top: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.user-id {
		font-size: 0.75rem;
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 0;
	}

	.sidebar-actions {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
	}

	.footer-btn {
		padding: 5px 10px;
		font-size: 0.75rem;
		background: var(--surface);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 6px;
		white-space: nowrap;
	}

	.footer-btn:hover {
		background: var(--surface-hover);
		color: var(--text);
	}

	.footer-btn.logout:hover {
		color: var(--danger);
		border-color: var(--danger);
	}

	.content {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
</style>
