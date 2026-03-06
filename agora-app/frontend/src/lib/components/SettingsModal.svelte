<script lang="ts">
	import { onMount } from 'svelte';
	import { theme } from '$lib/stores/theme';
	import { themes, type ThemeId } from '$lib/themes';
	import { auth } from '$lib/stores/auth';
	import { api } from '$lib/api';
	import { getIdentityKeys, getAgentId } from '$lib/crypto';

	interface Props {
		onClose: () => void;
	}

	let { onClose }: Props = $props();

	let currentTheme: ThemeId = $state('dark');
	theme.subscribe((v) => (currentTheme = v));

	let activeTab: 'appearance' | 'encryption' | 'connection' | 'profile' = $state('appearance');
	let userId = $state('');
	let displayName = $state('');
	let avatarUrl: string | undefined = $state(undefined);
	let uploadingAvatar = $state(false);
	let deviceId = $state('');
	let fingerprint = $state('');
	let agentId = $state('');
	let homeserverUrl = $state(api.getBaseUrl());

	auth.subscribe((v) => { deviceId = v.deviceId ?? ''; userId = v.userId ?? ''; });

	onMount(async () => {
		const [keysRes, aidRes, profileRes] = await Promise.allSettled([getIdentityKeys(), getAgentId(), loadProfile()]);
		if (keysRes.status === 'fulfilled' && keysRes.value) {
			fingerprint = keysRes.value.ed25519;
		}
		if (aidRes.status === 'fulfilled' && aidRes.value) {
			agentId = aidRes.value;
		}
	});

	async function loadProfile() {
		if (!userId) return;
		try {
			const profile = await api.getProfile(userId);
			displayName = profile.displayname ?? '';
			avatarUrl = profile.avatar_url;
		} catch (e) {
			console.error('Failed to load profile:', e);
		}
	}

	async function handleAvatarUpload(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0];
		if (!file || !userId) return;

		uploadingAvatar = true;
		try {
			const mxcUri = await api.uploadFile(file);
			await api.setAvatarUrl(userId, mxcUri);
			avatarUrl = mxcUri;
		} catch (e) {
			console.error('Failed to upload avatar:', e);
			alert('Failed to upload avatar');
		} finally {
			uploadingAvatar = false;
			input.value = '';
		}
	}

	async function handleDisplayNameUpdate() {
		if (!userId) return;
		try {
			await api.setDisplayName(userId, displayName);
		} catch (e) {
			console.error('Failed to update display name:', e);
		}
	}

	function getAvatarSrc(): string | null {
		if (!avatarUrl) return null;
		return api.downloadUrl(avatarUrl);
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="overlay" onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} role="dialog" tabindex="-1">
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
	<div class="modal" onclick={(e) => e.stopPropagation()} role="document">
		<div class="modal-header">
			<h3>Settings</h3>
			<button class="close-btn" onclick={onClose} aria-label="Close settings">&times;</button>
		</div>

		<div class="modal-body">
			<nav class="tabs">
				<button
					class="tab"
					class:active={activeTab === 'profile'}
					onclick={() => (activeTab = 'profile')}
				>Profile</button>
				<button
					class="tab"
					class:active={activeTab === 'appearance'}
					onclick={() => (activeTab = 'appearance')}
				>Appearance</button>
				<button
					class="tab"
					class:active={activeTab === 'encryption'}
					onclick={() => (activeTab = 'encryption')}
				>Encryption</button>
				<button
					class="tab"
					class:active={activeTab === 'connection'}
					onclick={() => (activeTab = 'connection')}
				>Connection</button>
			</nav>

			<div class="tab-content">
				{#if activeTab === 'profile'}
					<div class="setting-group">
						<span class="setting-label">Avatar</span>
						<div class="avatar-section">
							{#if getAvatarSrc()}
								<img class="current-avatar" src={getAvatarSrc()} alt="Profile" />
							{:else}
								<div class="avatar-placeholder">{displayName?.charAt(0)?.toUpperCase() || userId?.charAt(1)?.toUpperCase() || '?'}</div>
							{/if}
							<div class="avatar-controls">
								<label class="btn btn-secondary" class:disabled={uploadingAvatar}>
									{uploadingAvatar ? 'Uploading...' : 'Upload Avatar'}
									<input type="file" accept="image/*" onchange={handleAvatarUpload} disabled={uploadingAvatar} hidden />
								</label>
								{#if avatarUrl}
									<button class="btn btn-danger" onclick={() => { avatarUrl = undefined; api.setAvatarUrl(userId, ''); }}>Remove</button>
								{/if}
							</div>
						</div>
					</div>
					<div class="setting-group">
						<span class="setting-label">Display Name</span>
						<div class="displayname-row">
							<input
								type="text"
								class="displayname-input"
								value={displayName}
								onchange={(e) => displayName = (e.target as HTMLInputElement).value}
								placeholder="Your display name"
							/>
							<button class="btn btn-primary" onclick={handleDisplayNameUpdate}>Save</button>
						</div>
					</div>
					<div class="setting-group">
						<span class="setting-label">User ID</span>
						<code class="mono-value">{userId}</code>
					</div>
				{:else if activeTab === 'encryption'}
					<div class="setting-group">
						<span class="setting-label">Device ID</span>
						<code class="mono-value">{deviceId || 'Not set'}</code>
					</div>
					<div class="setting-group">
						<span class="setting-label">Device Fingerprint (Ed25519)</span>
						<code class="mono-value fingerprint">{fingerprint || 'Initializing...'}</code>
						<p class="setting-hint">Share this with others to verify your device.</p>
					</div>
					<div class="setting-group">
						<span class="setting-label">Agent ID (Sigchain Identity)</span>
						<code class="mono-value fingerprint">{agentId || 'Initializing...'}</code>
						<p class="setting-hint">Unique cryptographic identity for your behavioral ledger. Others can verify your actions via the sigchain API.</p>
					</div>
				{:else if activeTab === 'appearance'}
					<div class="setting-group">
						<span class="setting-label">Theme</span>
						<div class="theme-grid">
							{#each themes as t (t.id)}
								<button
									class="theme-card"
									class:active={currentTheme === t.id}
									onclick={() => theme.set(t.id)}
								>
									<div class="theme-preview" data-preview={t.id}>
										<div class="preview-sidebar"></div>
										<div class="preview-content">
											<div class="preview-line short"></div>
											<div class="preview-line"></div>
											<div class="preview-line medium"></div>
										</div>
									</div>
									<span class="theme-label">{t.label}</span>
									<span class="theme-desc">{t.description}</span>
								</button>
							{/each}
						</div>
					</div>
				{:else if activeTab === 'connection'}
					<div class="setting-group">
						<span class="setting-label">Homeserver URL</span>
						<code class="mono-value">{homeserverUrl}</code>
						<p class="setting-hint">Set during login. Log out and back in to change.</p>
					</div>
				{/if}
			</div>
		</div>
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
		width: 100%;
		max-width: 480px;
		box-shadow: 0 8px 32px var(--shadow);
		max-height: 80vh;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 20px 24px 0;
	}

	.modal-header h3 {
		font-size: 1.1rem;
		font-weight: 600;
	}

	.close-btn {
		width: 28px;
		height: 28px;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		background: none;
		border: none;
		color: var(--text-secondary);
		font-size: 1.3rem;
		border-radius: 6px;
	}

	.close-btn:hover {
		background: var(--surface-hover);
		color: var(--text);
	}

	.modal-body {
		padding: 16px 24px 24px;
		overflow-y: auto;
	}

	.tabs {
		display: flex;
		gap: 2px;
		margin-bottom: 20px;
		border-bottom: 1px solid var(--border);
	}

	.tab {
		padding: 8px 16px;
		font-size: 0.8rem;
		font-weight: 500;
		background: none;
		color: var(--text-secondary);
		border: none;
		border-bottom: 2px solid transparent;
		margin-bottom: -1px;
		transition: all 0.15s;
	}

	.tab:hover {
		color: var(--text);
	}

	.tab.active {
		color: var(--accent);
		border-bottom-color: var(--accent);
	}

	.setting-group {
		margin-bottom: 20px;
	}

	.setting-label {
		display: block;
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--text-secondary);
		margin-bottom: 12px;
	}

	.theme-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 10px;
	}

	.theme-card {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		padding: 10px;
		background: var(--surface);
		border: 2px solid var(--border);
		border-radius: 10px;
		transition: all 0.15s;
		text-align: center;
	}

	.theme-card:hover {
		border-color: var(--text-muted);
	}

	.theme-card.active {
		border-color: var(--accent);
	}

	.theme-preview {
		width: 100%;
		aspect-ratio: 4 / 3;
		border-radius: 6px;
		overflow: hidden;
		display: flex;
	}

	.theme-preview .preview-sidebar {
		width: 30%;
	}

	.theme-preview .preview-content {
		flex: 1;
		padding: 12% 10%;
		display: flex;
		flex-direction: column;
		gap: 6%;
	}

	.preview-line {
		height: 8%;
		border-radius: 2px;
		width: 80%;
	}

	.preview-line.short { width: 45%; }
	.preview-line.medium { width: 65%; }

	.theme-preview[data-preview='light'] {
		background: #f5f5f5;
	}
	.theme-preview[data-preview='light'] .preview-sidebar {
		background: #ffffff;
		border-right: 1px solid #e0e0e0;
	}
	.theme-preview[data-preview='light'] .preview-line {
		background: #d0d0d0;
	}

	.theme-preview[data-preview='dark'] {
		background: #1e1e1e;
	}
	.theme-preview[data-preview='dark'] .preview-sidebar {
		background: #252525;
		border-right: 1px solid #333;
	}
	.theme-preview[data-preview='dark'] .preview-line {
		background: #444;
	}

	.theme-preview[data-preview='seraphim'] {
		background: #0a0a0a;
	}
	.theme-preview[data-preview='seraphim'] .preview-sidebar {
		background: #111;
		border-right: 1px solid #222;
	}
	.theme-preview[data-preview='seraphim'] .preview-line {
		background: #ff6600;
		opacity: 0.4;
	}

	.theme-label {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--text);
	}

	.theme-desc {
		font-size: 0.65rem;
		color: var(--text-muted);
	}

	.mono-value {
		display: block;
		padding: 8px 12px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 6px;
		font-family: monospace;
		font-size: 0.75rem;
		color: var(--text);
		word-break: break-all;
		user-select: all;
	}

	.fingerprint {
		font-size: 0.65rem;
		letter-spacing: 0.02em;
	}

	.setting-hint {
		font-size: 0.7rem;
		color: var(--text-muted);
		margin-top: 6px;
	}

	.avatar-section {
		display: flex;
		align-items: center;
		gap: 16px;
	}

	.current-avatar {
		width: 80px;
		height: 80px;
		border-radius: 50%;
		object-fit: cover;
		border: 2px solid var(--border);
	}

	.avatar-placeholder {
		width: 80px;
		height: 80px;
		border-radius: 50%;
		background: var(--surface);
		border: 2px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 2rem;
		font-weight: 600;
		color: var(--accent);
	}

	.avatar-controls {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.btn {
		padding: 8px 16px;
		border-radius: 6px;
		font-size: 0.8rem;
		font-weight: 500;
		cursor: pointer;
		border: none;
		transition: all 0.15s;
	}

	.btn-primary {
		background: var(--accent);
		color: white;
	}

	.btn-primary:hover {
		opacity: 0.9;
	}

	.btn-secondary {
		background: var(--surface);
		color: var(--text);
		border: 1px solid var(--border);
	}

	.btn-secondary:hover {
		background: var(--surface-hover);
	}

	.btn-secondary.disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.btn-danger {
		background: transparent;
		color: var(--error, #ff4444);
		border: 1px solid var(--error, #ff4444);
	}

	.btn-danger:hover {
		background: var(--error, #ff4444);
		color: white;
	}

	.displayname-row {
		display: flex;
		gap: 8px;
	}

	.displayname-input {
		flex: 1;
		padding: 8px 12px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.875rem;
	}

	.displayname-input:focus {
		outline: none;
		border-color: var(--accent);
	}
</style>
