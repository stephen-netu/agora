<script lang="ts">
	import { goto } from '$app/navigation';
	import { auth } from '$lib/stores/auth';
	import { sync } from '$lib/stores/sync';
	import { api } from '$lib/api';
	import ThemeSwitcher from '$lib/components/ThemeSwitcher.svelte';

	let homeserver = $state(api.getBaseUrl());
	let username = $state('');
	let password = $state('');
	let error = $state('');
	let loading = $state(false);
	let showAdvanced = $state(false);

	async function handleLogin() {
		error = '';
		loading = true;
		api.setBaseUrl(homeserver);
		try {
			await auth.login(username, password);
			sync.start();
			goto('/rooms');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Login failed';
		} finally {
			loading = false;
		}
	}
</script>

<div class="auth-page">
	<div class="auth-card">
		<div class="auth-header">
			<h1>Agora</h1>
			<p class="subtitle">Sign in to continue</p>
		</div>

		<form onsubmit={(e) => { e.preventDefault(); handleLogin(); }}>
			<div class="field">
				<label for="username">Username</label>
				<input
					id="username"
					type="text"
					bind:value={username}
					placeholder="alice"
					autocomplete="username"
				/>
			</div>

			<div class="field">
				<label for="password">Password</label>
				<input
					id="password"
					type="password"
					bind:value={password}
					placeholder="password"
					autocomplete="current-password"
				/>
			</div>

			<button
				type="button"
				class="advanced-toggle"
				onclick={() => (showAdvanced = !showAdvanced)}
			>{showAdvanced ? 'Hide' : 'Homeserver'}</button>

			{#if showAdvanced}
				<div class="field">
					<label for="homeserver">Homeserver URL</label>
					<input
						id="homeserver"
						type="url"
						bind:value={homeserver}
						placeholder="http://localhost:8008"
					/>
				</div>
			{/if}

			{#if error}
				<p class="error">{error}</p>
			{/if}

			<button type="submit" class="btn-primary submit" disabled={loading || !username || !password}>
				{loading ? 'Signing in...' : 'Sign in'}
			</button>
		</form>

		<p class="alt-action">
			Don't have an account? <a href="/auth/register">Register</a>
		</p>

		<div class="theme-row">
			<ThemeSwitcher />
		</div>
	</div>
</div>

<style>
	.auth-page {
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 100vh;
		background: var(--bg-secondary);
	}

	.auth-card {
		width: 100%;
		max-width: 400px;
		padding: 40px;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 12px;
		box-shadow: 0 4px 24px var(--shadow);
	}

	.auth-header {
		text-align: center;
		margin-bottom: 32px;
	}

	.auth-header h1 {
		font-size: 1.75rem;
		font-weight: 700;
		color: var(--accent);
		margin-bottom: 4px;
	}

	.subtitle {
		color: var(--text-secondary);
		font-size: 0.875rem;
	}

	.field {
		margin-bottom: 16px;
	}

	.field label {
		display: block;
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--text-secondary);
		margin-bottom: 6px;
	}

	.advanced-toggle {
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 0.75rem;
		padding: 0;
		margin-bottom: 12px;
		cursor: pointer;
	}

	.advanced-toggle:hover {
		color: var(--accent);
	}

	.error {
		color: var(--danger);
		font-size: 0.8rem;
		margin-bottom: 12px;
	}

	.submit {
		width: 100%;
		padding: 12px;
		font-size: 0.9rem;
		margin-top: 8px;
	}

	.alt-action {
		text-align: center;
		margin-top: 20px;
		font-size: 0.8rem;
		color: var(--text-secondary);
	}

	.alt-action a {
		color: var(--accent);
		text-decoration: none;
		font-weight: 500;
	}

	.alt-action a:hover {
		text-decoration: underline;
	}

	.theme-row {
		display: flex;
		justify-content: center;
		margin-top: 24px;
	}
</style>
