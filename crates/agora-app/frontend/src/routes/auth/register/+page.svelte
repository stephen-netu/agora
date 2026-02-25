<script lang="ts">
	import { goto } from '$app/navigation';
	import { auth } from '$lib/stores/auth';
	import { sync } from '$lib/stores/sync';
	import ThemeSwitcher from '$lib/components/ThemeSwitcher.svelte';

	let username = $state('');
	let password = $state('');
	let confirmPassword = $state('');
	let error = $state('');
	let loading = $state(false);

	async function handleRegister() {
		error = '';

		if (password !== confirmPassword) {
			error = 'Passwords do not match';
			return;
		}

		loading = true;
		try {
			await auth.register(username, password);
			sync.start();
			goto('/rooms');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Registration failed';
		} finally {
			loading = false;
		}
	}
</script>

<div class="auth-page">
	<div class="auth-card">
		<div class="auth-header">
			<h1>Agora</h1>
			<p class="subtitle">Create a new account</p>
		</div>

		<form onsubmit={(e) => { e.preventDefault(); handleRegister(); }}>
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
					autocomplete="new-password"
				/>
			</div>

			<div class="field">
				<label for="confirm">Confirm password</label>
				<input
					id="confirm"
					type="password"
					bind:value={confirmPassword}
					placeholder="password"
					autocomplete="new-password"
				/>
			</div>

			{#if error}
				<p class="error">{error}</p>
			{/if}

			<button
				type="submit"
				class="btn-primary submit"
				disabled={loading || !username || !password || !confirmPassword}
			>
				{loading ? 'Creating account...' : 'Create account'}
			</button>
		</form>

		<p class="alt-action">
			Already have an account? <a href="/auth/login">Sign in</a>
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
