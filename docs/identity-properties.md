# Yggdrasil Co-derived Identity

In the KOS architecture, an agent's cryptographic identity and its routing identity are tightly coupled by design. This document outlines the mechanism and properties of this co-derived identity model.

## Core Principle

Both an agent's Agora `AgentId` and its Yggdrasil IPv6 address are deterministically derived from the exact same underlying Ed25519 signing key (the agent's physical or sovereign "seed").

- **Ed25519 Seed**: The master secret (e.g., generated via Argon2id from a mnemonic phrase).
- **AgentId**: Derived directly from the BLAKE3 hash of the Ed25519 public key. This serves as the agent's identifier in the Agora application layer, sigchains, and SOVEREIGN capability registries.
- **Yggdrasil IPv6**: Derived from the exact same Ed25519 public key using Yggdrasil's Node ID derivation algorithm. This serves as the agent's routable network address within the global mesh.

## Security Properties

This co-derivation provides several critical security and operational properties that differentiate KOS from traditional tiered architectures:

### 1. Zero Configuration Networking
Because the network address is mathematically derived from the identity key, there is **no DHCP, no IP address assignment, and no DNS required** for core mesh connectivity. As soon as an agent generates its identity, it inherently knows its network address.

### 2. Cryptographic Binding
It is mathematically impossible to spoof an agent's network traffic without possessing the private key. When Agent A receives a message from an IPv6 address `200::...`, it can immediately cryptographically verify that the address corresponds to Agent B's expected `AgentId`. The network address *is* the identity.

### 3. Location Independence
The agent's IPv6 address remains constant regardless of its physical underlay network (WiFi, cellular, physical relocation). The identity does not change when the physical networking topology changes.

### 4. Self-Certifying
Both the identity and the route to that identity are self-certifying. By holding the public key of an agent, you can independently derive both its `AgentId` for application logic and its IPv6 address for direct point-to-point packet delivery.

## Implementation Implications

- **Bootstrapping**: An agent bringing itself online only needs its seed. It derives its networking stack configuration and its application identity simultaneously.
- **Trust Verification**: Application-layer protocols (like Agora) do not need to maintain complex IP-to-ID mapping tables. The IPv6 source address of an incoming packet is directly verifiable against the expected signature on the payload.
- **Ephemeral and Persistent**: The same mechanism applies whether the agent is a long-lived sovereign node (persisted seed) or a temporary worker process (ephemeral in-memory seed).

This foundational property ensures that the KOS network remains highly resilient, strictly deterministic, and deeply secure at the networking substrate, aligning with S-01 (Kernel Mediation) and strictly enforcing S-02 (Determinism).
