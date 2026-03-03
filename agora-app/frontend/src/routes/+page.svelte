<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { auth } from '$lib/stores/auth';

	onMount(() => {
		const unsub = auth.subscribe((state) => {
			if (state.token) {
				goto('/rooms');
			} else {
				goto('/auth/login');
			}
		});
		return unsub;
	});
</script>

<div class="loading">
	<p>Loading...</p>
</div>

<style>
	.loading {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100vh;
		color: var(--text-secondary);
	}
</style>
