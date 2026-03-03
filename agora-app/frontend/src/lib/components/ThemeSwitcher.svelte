<script lang="ts">
	import { theme } from '$lib/stores/theme';
	import { themes, type ThemeId } from '$lib/themes';

	let current: ThemeId = $state('dark');

	theme.subscribe((v) => (current = v));

	function select(id: ThemeId) {
		theme.set(id);
	}
</script>

<div class="theme-switcher">
	{#each themes as t (t.id)}
		<button
			class="theme-option"
			class:active={current === t.id}
			onclick={() => select(t.id)}
			title={t.description}
		>
			<span class="swatch" data-swatch={t.id}></span>
			{t.label}
		</button>
	{/each}
</div>

<style>
	.theme-switcher {
		display: flex;
		gap: 4px;
	}

	.theme-option {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		font-size: 0.75rem;
		background: var(--surface);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 6px;
		transition: all 0.15s;
	}

	.theme-option:hover {
		background: var(--surface-hover);
		color: var(--text);
	}

	.theme-option.active {
		border-color: var(--accent);
		color: var(--accent);
	}

	.swatch {
		width: 12px;
		height: 12px;
		border-radius: 50%;
		border: 1px solid var(--border);
	}

	.swatch[data-swatch='light'] {
		background: #ffffff;
	}

	.swatch[data-swatch='dark'] {
		background: #252525;
	}

	.swatch[data-swatch='seraphim'] {
		background: linear-gradient(135deg, #0a0a0a 50%, #ff6600 50%);
	}
</style>
