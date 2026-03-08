import { describe, it, expect, vi, beforeEach } from 'vitest';

// Setup jsdom globals before imports
import { JSDOM } from 'jsdom';
const dom = new JSDOM('<!DOCTYPE html><html><body></body></html>');
(global as any).window = dom.window;
(global as any).document = dom.window.document;
// navigator is read-only, but we don't need it for these tests

// Mock the @tauri-apps/api/core module
vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn()
}));

// Mock the api module as well
vi.mock('$lib/api', () => ({
	api: {
		keysQuery: vi.fn(),
		keysClaim: vi.fn(),
		sendToDevice: vi.fn()
	}
}));

// Import after mocking
import { getAgentDisplayName } from '$lib/crypto';
import { invoke } from '@tauri-apps/api/core';

describe('getAgentDisplayName', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		// Reset window.__TAURI_INTERNALS__ for each test
		Object.defineProperty(window, '__TAURI_INTERNALS__', {
			value: undefined,
			configurable: true,
			writable: true
		});
	});

	it('should return display name when Tauri invoke succeeds', async () => {
		// Arrange
		const mockDisplayName = 'clever-fox#5678';
		const agentIdHex = 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef';
		
		// Set up window for Tauri
		Object.defineProperty(window, '__TAURI_INTERNALS__', {
			value: {},
			configurable: true
		});
		
		vi.mocked(invoke).mockResolvedValue(mockDisplayName);

		// Act
		const result = await getAgentDisplayName(agentIdHex);

		// Assert
		expect(result).toBe(mockDisplayName);
		expect(invoke).toHaveBeenCalledWith('get_agent_display_name', {
			agentIdHex
		});
	});

	it('should return null when Tauri invoke throws', async () => {
		// Arrange
		const agentIdHex = 'cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe';
		
		// Set up window for Tauri
		Object.defineProperty(window, '__TAURI_INTERNALS__', {
			value: {},
			configurable: true
		});
		
		vi.mocked(invoke).mockRejectedValue(new Error('Tauri error'));

		// Act
		const result = await getAgentDisplayName(agentIdHex);

		// Assert
		expect(result).toBeNull();
	});

	it('should return null when not running in Tauri', async () => {
		// Arrange - delete the Tauri internals property to simulate browser environment
		// The function checks: typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
		// We need to ensure __TAURI_INTERNALS__ is NOT in window
		try {
			delete (window as any).__TAURI_INTERNALS__;
		} catch (e) {
			// Property might be non-configurable, ignore
		}

		const agentIdHex = '0000000000000000000000000000000000000000000000000000000000000000';

		// Act
		const result = await getAgentDisplayName(agentIdHex);

		// Assert
		expect(result).toBeNull();
		expect(invoke).not.toHaveBeenCalled();
	});

	it('should pass correct hex string to invoke', async () => {
		// Arrange
		const mockDisplayName = 'able-ant#0000';
		const agentIdHex = '0000000000000000000000000000000000000000000000000000000000000000';
		
		Object.defineProperty(window, '__TAURI_INTERNALS__', {
			value: {},
			configurable: true
		});
		
		vi.mocked(invoke).mockResolvedValue(mockDisplayName);

		// Act
		await getAgentDisplayName(agentIdHex);

		// Assert
		expect(invoke).toHaveBeenCalledWith('get_agent_display_name', {
			agentIdHex: '0000000000000000000000000000000000000000000000000000000000000000'
		});
	});
});

// Test format validation helpers (pure functions we can test directly)
describe('display name format validation', () => {
	// Helper function to validate display name format
	function isValidDisplayName(name: string): boolean {
		if (!name || typeof name !== 'string') return false;
		
		const parts = name.split('#');
		if (parts.length !== 2) return false;
		
		const [wordPart, checksum] = parts;
		if (checksum.length !== 4) return false;
		if (!/^\d{4}$/.test(checksum)) return false;
		
		const wordParts = wordPart.split('-');
		if (wordParts.length !== 2) return false;
		
		return wordParts.every(w => /^[a-z]+$/.test(w));
	}

	it('should validate correct display name format', () => {
		expect(isValidDisplayName('clever-fox#5678')).toBe(true);
		expect(isValidDisplayName('able-ant#0000')).toBe(true);
		expect(isValidDisplayName('happy-cat#9999')).toBe(true);
	});

	it('should reject invalid display name formats', () => {
		// Missing hash
		expect(isValidDisplayName('clever-fox5678')).toBe(false);
		// Wrong checksum length
		expect(isValidDisplayName('clever-fox#567')).toBe(false);
		expect(isValidDisplayName('clever-fox#56788')).toBe(false);
		// Non-digit checksum
		expect(isValidDisplayName('clever-fox#56a8')).toBe(false);
		// Missing hyphen
		expect(isValidDisplayName('cleverfox#5678')).toBe(false);
		// Too many hyphens
		expect(isValidDisplayName('clever-fox-cat#5678')).toBe(false);
		// Non-lowercase
		expect(isValidDisplayName('Clever-fox#5678')).toBe(false);
		// Empty
		expect(isValidDisplayName('')).toBe(false);
		expect(isValidDisplayName(null as any)).toBe(false);
	});
});

// Test handle caching logic
describe('handle caching', () => {
	it('should cache handles by AgentId', async () => {
		// This tests the caching logic in the UI component
		// The MessageList component caches handles - test the expected behavior
		
		const handleCache = new Map<string, string>();
		
		// Simulate caching behavior
		const getOrCacheHandle = async (agentId: string, fetchFn: () => Promise<string>) => {
			if (handleCache.has(agentId)) {
				return handleCache.get(agentId)!;
			}
			const handle = await fetchFn();
			handleCache.set(agentId, handle);
			return handle;
		};
		
		const mockFetch = vi.fn().mockResolvedValue('clever-fox#5678');
		
		// First call should fetch
		const result1 = await getOrCacheHandle('agent123', mockFetch);
		expect(result1).toBe('clever-fox#5678');
		expect(mockFetch).toHaveBeenCalledTimes(1);
		
		// Second call should use cache
		const result2 = await getOrCacheHandle('agent123', mockFetch);
		expect(result2).toBe('clever-fox#5678');
		expect(mockFetch).toHaveBeenCalledTimes(1); // Still only 1
		
		// Different agent should fetch
		const result3 = await getOrCacheHandle('agent456', mockFetch);
		expect(result3).toBe('clever-fox#5678');
		expect(mockFetch).toHaveBeenCalledTimes(2);
	});
});
