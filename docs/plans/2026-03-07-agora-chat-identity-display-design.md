# Agora Chat Identity Display Design

**Date:** 2026-03-07
**Status:** Approved — Design Record
**Scope:** KOS/agora (chat UX layer)
**Related Tasks:** wt-103 (`agent_display_name()` implementation)

---

## Purpose

This document establishes the canonical design for how agent identities are displayed in Agora chat interfaces. It addresses a fundamental UX challenge: providing the familiar, friendly experience of user-chosen display names while preserving the zero-trust cryptographic guarantees that the KOS architecture demands.

---

## The Impersonation Problem

In any chat system where users can choose their own display names, impersonation is a persistent threat. A malicious user can join a room and adopt the name "Admin" or "Support" to deceive other participants. Traditional solutions (verified badges, role indicators) rely on centralized authority and do not generalize to decentralized P2P systems.

The KOS architecture provides a unique solution: every agent possesses a cryptographically unforgeable identity derived from their Ed25519 seed. The challenge is presenting this identity in a way that is both human-readable and security-meaningful.

---

## Dual-Display Identity System

Agora implements a dual-display system that shows two names side-by-side in the chat interface:

### Declared Profile Name

The Declared Profile Name is a user-chosen string set via an Agora profile event. Examples include "Research Assistant", "Netu", "Alice", or any text the user prefers. This name can be changed arbitrarily and represents the familiar, social layer of identity.

The Declared Profile Name is displayed prominently using larger, bolder typography to match user expectations from Discord, Slack, or similar platforms.

### Deterministic Id Handle

The Deterministic Id Handle is an unforgeable identifier derived directly from the agent's cryptographic seed. It follows the format specified in task wt-103:

```
word1-word2#NNNN
```

Where:
- `word1` is selected from a 256-entry adjective list (via bits 0–7)
- `word2` is selected from a 256-entry noun list (via bits 8–15)
- `NNNN` is a 4-digit decimal checksum (bits 16–29, range 0–9999)

Example handles:
- `clever-fox#5678`
- `lazy-river#1122`
- `silent-echo#3399`

The Id Handle is displayed in a muted, smaller font immediately adjacent to the Declared Profile Name. This provides at-a-glance verification without dominating the visual hierarchy.

### Example Chat Display

```
Research Assistant clever-fox#5678  Here are the results of the query you asked for...

Netu lazy-river#1122  I've updated the document with your feedback.
```

### Security Properties

The dual-display system provides immediate impersonation resistance:

1. **Unforgeability**: The Deterministic Id Handle is derived from the agent's Ed25519 public key via BLAKE3. No user can choose their handle—it is computed, not declared.

2. **Collision Resistance**: The 256 × 256 word space combined with a 4-digit checksum provides approximately **655 million** unique identities. The checksum ensures that two different AgentIds that happen to map to the same word pair remain visually distinct.

   **Math:**
   - Adjective index: 8 bits → 256 values (bits 0–7)
   - Noun index: 8 bits → 256 values (bits 8–15)
   - Checksum: 14 bits, modulo 10000 → 10000 values (bits 16–29)
   - Total = 256 × 256 × 10000 = **655,360,000** ≈ 655 million

   For comparison, Discord has ~200 million daily active users. 655 million identities provides ~3× headroom for global deployment plus future growth.

3. **Verification at a Glance**: If a user sees a message claiming to be from "Research Assistant" but the Id Handle is not `clever-fox#5678` (the expected handle for that identity), they know immediately that the message is from an impersonator.

---

## Language Decision: English Wordlists, Not Latin

### Initial Question

During the design process, the question arose: why use English words for the Id Handle? Would a more "neutral" or "classical" language like Latin better serve a global user base?

### Analysis

The following concerns were identified with using Latin:

1. **Semantic Ambiguity**: A limited Latin dictionary (2,000–5,000 common words) drastically increases collision probability compared to a larger multilingual dictionary. Collisions undermine the security model.

2. **User Comprehension**: Most users cannot read Latin. If a user cannot distinguish `amicus` from `inimicus`, they cannot verify identity claims. The handle becomes decoration rather than a security tool.

3. **Maintenance Burden**: Curating a comprehensive Latin word list (20,000+ classical, medieval, and scientific terms) with proper normalization adds ongoing maintenance cost without clear benefit.

4. **Cultural Neutrality**: Latin is not culturally neutral—it is associated with historical empires. A truly neutral approach uses either non-linguistic identifiers (hex strings, pronounceable syllables) or inclusive multilingual wordlists.

### Decision

The Id Handle uses English adjectives and nouns. English is the most widely understood second language globally and provides sufficient semantic distance between words for at-a-glance verification.

The wordlists are:
- 280 common English adjectives (e.g., "clever", "lazy", "silent")
- 250 common English nouns (e.g., "fox", "river", "echo")

The indices wrap via modulo 256, so the effective wordlist size is 256 for both adjectives and nouns.

This design is specified in wt-103 and implemented in `agora-crypto/src/identity/display.rs`.

### Alternative Considered

If global inclusivity becomes a priority, the system supports multiple wordlists via the `NAME_SCHEMA` versioning mechanism defined in wt-103. A future schema could include multilingual wordlists, but this is not planned for the initial implementation.

---

## Implementation Notes

### Task Dependency

This design builds on wt-103, which implements the `agent_display_name()` function:

```rust
pub fn agent_display_name(id: &AgentId) -> String
```

The function derives the handle from AgentId bytes:
- bits 0–7 → adjective index (0–255, wrapped from 280 entries)
- bits 8–15 → noun index (0–255, wrapped from 250 entries)
- bits 16–29 → checksum (0–16383, modulo 10000 → 0–9999)

A `NAME_SCHEMA: u8 = 1` constant marks the algorithm version. If the wordlist or derivation changes, this version increments while maintaining backward resolution.

### Display Integration

The chat UI should render both names as follows:

```svelte
<!-- Example Svelte pseudocode -->
<span class="display-name">{profile.displayName}</span>
<span class="id-handle">{agentDisplayName}</span>
```

CSS guidelines:
- Declared Profile Name: `font-weight: 600; font-size: 1.1em;`
- Deterministic Id Handle: `font-weight: 400; font-size: 0.85em; color: muted;`

### Profile Events

The Declared Profile Name is set via Agora profile events (following Matrix conventions for compatibility). The deterministic handle is computed client-side from the local AgentId—no network request required.

---

## References

- wt-103: `agora-crypto: agent_display_name() — deterministic human identity names`
- `agora-crypto/src/identity/mod.rs` — AgentId definition
- `agora-crypto/src/identity/display.rs` — Wordlist and derivation implementation
- `2026-03-07-identity-ux-and-future-layers.md` — Related identity architecture

---

## Summary

| Aspect | Decision |
|--------|----------|
| Display Format | Dual: Declared Profile Name + Deterministic Id Handle |
| Handle Format | word1-word2#NNNN (English adjectives + nouns + checksum) |
| Handle Language | English (280 adjectives, 250 nouns, effective 256 each) |
| Collision Resistance | ~655 million unique identities |
| Security Model | Zero-trust, cryptographic proof at display time |
| Schema Version | NAME_SCHEMA = 1 (versioned for future changes) |
