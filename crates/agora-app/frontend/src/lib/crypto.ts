import { api } from '$lib/api';

async function tauriInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
	if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke(cmd, args);
	}
	return null;
}

export interface DeviceInfo {
	user_id: string;
	device_id: string;
	curve25519_key: string;
	ed25519_key: string;
}

export interface EncryptedPayload {
	algorithm: string;
	sender_key: string;
	ciphertext: string;
	session_id: string;
	device_id: string;
}

export interface DecryptedPayload {
	type: string;
	content: Record<string, unknown>;
	room_id: string;
}

export async function ensureRoomKeysShared(roomId: string, memberUserIds: string[]) {
	const queryResp = await api.keysQuery(memberUserIds);
	const allDevices: DeviceInfo[] = [];

	for (const [userId, devices] of Object.entries(queryResp.device_keys)) {
		for (const [deviceId, dkPayload] of Object.entries(devices)) {
			const dk = dkPayload as Record<string, unknown>;
			const keys = dk.keys as Record<string, string> | undefined;
			if (!keys) continue;
			const curveKey = keys[`curve25519:${deviceId}`];
			const edKey = keys[`ed25519:${deviceId}`];
			if (curveKey && edKey) {
				allDevices.push({
					user_id: userId,
					device_id: deviceId,
					curve25519_key: curveKey,
					ed25519_key: edKey
				});
			}
		}
	}

	const needing = (await tauriInvoke('devices_needing_keys', {
		roomId,
		allDevices
	})) as DeviceInfo[] | null;

	if (!needing || needing.length === 0) return;

	const claimRequest: Record<string, Record<string, string>> = {};
	for (const d of needing) {
		if (!claimRequest[d.user_id]) claimRequest[d.user_id] = {};
		claimRequest[d.user_id][d.device_id] = 'signed_curve25519';
	}
	const claimResp = await api.keysClaim(claimRequest);

	for (const d of needing) {
		const userKeys = claimResp.one_time_keys?.[d.user_id] as
			| Record<string, unknown>
			| undefined;
		const deviceKeys = userKeys?.[d.device_id] as Record<string, unknown> | undefined;
		if (!deviceKeys) continue;

		const otkEntry = Object.values(deviceKeys)[0] as Record<string, string> | undefined;
		if (!otkEntry?.key) continue;

		await tauriInvoke('create_olm_session_from_otk', {
			theirCurveKey: d.curve25519_key,
			oneTimeKey: otkEntry.key
		});
	}

	const roomKeyContent = (await tauriInvoke('get_room_key_content', {
		roomId
	})) as Record<string, unknown> | null;
	if (!roomKeyContent) return;

	const toDeviceMessages: Record<string, Record<string, unknown>> = {};
	for (const d of needing) {
		const innerPayload = JSON.stringify({
			type: 'm.room_key',
			content: roomKeyContent
		});

		const encrypted = (await tauriInvoke('encrypt_olm_event', {
			recipientCurveKey: d.curve25519_key,
			recipientEdKey: d.ed25519_key,
			plaintext: innerPayload
		})) as Record<string, unknown> | null;

		if (encrypted) {
			if (!toDeviceMessages[d.user_id]) toDeviceMessages[d.user_id] = {};
			toDeviceMessages[d.user_id][d.device_id] = encrypted;

			await tauriInvoke('mark_keys_shared', {
				roomId,
				userId: d.user_id,
				deviceId: d.device_id
			});
		}
	}

	if (Object.keys(toDeviceMessages).length > 0) {
		await api.sendToDevice('m.room.encrypted', toDeviceMessages);
	}
}

export async function encryptMessage(
	roomId: string,
	eventType: string,
	content: Record<string, unknown>
): Promise<EncryptedPayload | null> {
	return (await tauriInvoke('encrypt_message', {
		roomId,
		eventType,
		content
	})) as EncryptedPayload | null;
}

export async function decryptEvent(
	roomId: string,
	senderKey: string,
	sessionId: string,
	ciphertext: string
): Promise<DecryptedPayload | null> {
	try {
		return (await tauriInvoke('decrypt_event', {
			roomId,
			senderKey,
			sessionId,
			ciphertext
		})) as DecryptedPayload | null;
	} catch {
		return null;
	}
}

export async function getIdentityKeys(): Promise<{ curve25519: string; ed25519: string } | null> {
	try {
		const keys = (await tauriInvoke('get_identity_keys')) as [string, string] | null;
		if (keys) return { curve25519: keys[0], ed25519: keys[1] };
		return null;
	} catch {
		return null;
	}
}
