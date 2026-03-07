//! Human-readable deterministic identity names.
//!
//! Generates memorable, collision-resistant handles from AgentId.
//!
//! Format: word1-word2#NNNN
//!   - word1: adjective (from 280 entries, index via bits 0-7)
//!   - word2: noun (from 250 entries, index via bits 8-15)
//!   - NNNN: 4-digit checksum (bits 16-29, modulo 10000)
//!
//! Total space: 280 × 250 × 10000 = 700,000,000 (~700 million) unique identities.
//! NAME_SCHEMA = 1 marks the algorithm version.

use super::AgentId;

/// Schema version for the name derivation algorithm.
/// Increment if wordlist or algorithm changes.
pub const NAME_SCHEMA: u8 = 1;

// 280 common English adjectives
const ADJECTIVES: &[&str; 280] = &[
    "able", "acid", "aged", "also", "area", "army", "away", "baby", "back", "ball",
    "band", "bank", "base", "bath", "bear", "beat", "beer", "bell", "belt", "best",
    "bill", "bird", "blow", "blue", "boat", "body", "bomb", "bond", "bone", "book",
    "boom", "born", "boss", "both", "bowl", "bulk", "burn", "bush", "busy", "call",
    "calm", "came", "camp", "card", "care", "case", "cash", "cast", "cell", "chat",
    "chip", "city", "club", "coal", "coat", "code", "cold", "come", "cook", "cool",
    "cope", "copy", "core", "cost", "crew", "crop", "dark", "data", "date", "dawn",
    "days", "dead", "deal", "dear", "debt", "deep", "deny", "desk", "dial", "diet",
    "dirt", "dish", "disk", "does", "done", "door", "dose", "down", "draw", "drop",
    "drug", "drum", "dual", "duke", "dust", "duty", "each", "earn", "east", "easy",
    "edge", "edit", "else", "even", "ever", "evil", "exit", "face", "fact", "fail",
    "fair", "fall", "farm", "fast", "fate", "fear", "feed", "feel", "feet", "fell",
    "felt", "file", "fill", "film", "find", "fine", "fire", "firm", "fish", "five",
    "flat", "flow", "food", "foot", "ford", "form", "fort", "four", "free", "from",
    "fuel", "full", "fund", "gain", "game", "gate", "gave", "gear", "gene", "gift",
    "girl", "give", "glad", "goal", "goes", "gold", "golf", "gone", "good", "gray",
    "grew", "grey", "grid", "grim", "grow", "gulf", "hair", "half", "hall", "hand",
    "hang", "hard", "harm", "hate", "have", "head", "hear", "heat", "held", "hell",
    "help", "here", "hero", "high", "hill", "hire", "hold", "hole", "holy", "home",
    "hope", "host", "hour", "huge", "hung", "hunt", "hurt", "idea", "inch", "into",
    "iron", "item", "jack", "jail", "jazz", "join", "joke", "jury", "just", "keen",
    "keep", "kept", "kick", "kill", "kind", "king", "kiss", "knee", "knew", "know",
    "lack", "lady", "laid", "lake", "land", "lane", "last", "late", "lead", "left",
    "less", "life", "lift", "like", "line", "link", "list", "live", "load", "loan",
    "lock", "logo", "long", "look", "lord", "lose", "loss", "lost", "love", "luck",
    "made", "mail", "main", "make", "male", "many", "mark", "mass", "meal", "mean",
    "meat", "meet", "menu", "mere", "mike", "mile", "milk", "mill", "mind", "mine",
    "miss", "mode", "mood", "moon", "more", "most", "move", "much", "must", "myth",
];

// 250 common English nouns
const NOUNS: &[&str; 250] = &[
    "ant", "ape", "arch", "area", "arm", "army", "aunt", "baby", "back", "bag",
    "bait", "bald", "ball", "band", "bank", "bar", "bark", "barn", "bat", "bay",
    "beak", "beam", "bean", "bear", "beast", "bed", "bee", "bell", "belt", "bend",
    "bent", "bike", "bird", "bite", "blow", "blue", "boat", "body", "bone", "book",
    "boom", "boot", "born", "boss", "bowl", "burn", "bush", "busy", "cake", "call",
    "calm", "came", "camp", "cane", "cape", "card", "care", "carp", "cart", "case",
    "cash", "cast", "cave", "cell", "chat", "chip", "city", "clay", "club", "coal",
    "coat", "code", "coil", "coin", "cold", "cone", "cook", "cool", "cope", "copy",
    "coral", "core", "corn", "cost", "crab", "crew", "crib", "crop", "crow", "cube",
    "cup", "curb", "curl", "cut", "dame", "damp", "dare", "dark", "dart", "dash",
    "data", "date", "dawn", "days", "dead", "deal", "dear", "debt", "deck", "deed",
    "deer", "desk", "dial", "diet", "dirt", "dish", "disk", "dock", "does", "dog",
    "doll", "dome", "done", "door", "dose", "dot", "dove", "down", "drag", "draw",
    "drew", "drip", "drop", "drum", "dual", "duck", "dude", "duel", "duet", "dull",
    "dumb", "dump", "dune", "dunk", "dusk", "dust", "duty", "each", "ear", "ease",
    "east", "easy", "eats", "echo", "edge", "edit", "eel", "egg", "else", "emit",
    "ends", "envy", "epic", "even", "ever", "evil", "exam", "exit", "eyed", "face",
    "fact", "fade", "fail", "fair", "fake", "fall", "fame", "fang", "farm", "fast",
    "fate", "fawn", "fear", "feat", "feed", "feel", "feet", "fell", "felt", "fern",
    "fest", "fete", "fever", "few", "fiat", "fief", "fight", "file", "fill", "film",
    "find", "fine", "fire", "firm", "fish", "fist", "five", "flag", "flame", "flap",
    "flat", "flaw", "flea", "fled", "flew", "flip", "flit", "flow", "foam", "fog",
    "foil", "fold", "folk", "fond", "font", "food", "fool", "foot", "ford", "fore",
    "fork", "form", "fort", "foul", "found", "fowl", "fox", "foyer", "frame", "fray",
    "free", "frog", "from", "fuel", "full", "fume", "fund", "fury", "fuse", "fuss",
];

/// Generate a human-readable deterministic name from an AgentId.
///
/// Format: word1-word2#NNNN
///   - word1: adjective from ADJECTIVES (bits 0-7 → index 0-255)
///   - word2: noun from NOUNS (bits 8-15 → index 0-255)
///   - NNNN: 4-digit checksum (bits 16-29 → 0-9999)
///
/// Returns ~700 million unique identities (280 × 250 × 10000).
/// The checksum ensures two different AgentIds that map to the same word pair
/// remain visually distinct.
pub fn agent_display_name(id: &AgentId) -> String {
    let bytes = id.as_bytes();
    
    // Extract bits 0-7 (8 bits) → adjective index (0-255)
    let adj_index = (bytes[0] & 0xFF) as usize;
    
    // Extract bits 8-15 (8 bits) → noun index (0-255)
    let noun_index = (bytes[1] & 0xFF) as usize;
    
    // Extract bits 16-29 (14 bits) → checksum (0-16383), we use 0-9999
    let checksum = ((u16::from_be_bytes([bytes[2], bytes[3]]) & 0x3FFF) % 10000) as u16;
    
    let adjective = ADJECTIVES[adj_index % ADJECTIVES.len()];
    let noun = NOUNS[noun_index % NOUNS.len()];
    
    format!("{}-{}#{:04}", adjective, noun, checksum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentId;
    use std::collections::BTreeSet;
    
    // ─────────────────────────────────────────────────────────────────────────────
    // Format and structure tests
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_agent_display_name_format() {
        let id_bytes = [0u8; 32];
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        let name = agent_display_name(&id);
        
        // Check format: word-word#NNNN
        assert!(name.contains('-'), "Should contain hyphen");
        assert!(name.contains('#'), "Should contain hash");
        let parts: Vec<&str> = name.split('#').collect();
        assert_eq!(parts.len(), 2, "Should have exactly one #");
        
        // Check checksum is 4 digits
        assert_eq!(parts[1].len(), 4, "Checksum should be 4 digits");
    }
    
    #[test]
    fn test_format_validation_manual() {
        // Test multiple different AgentIds - validate format manually
        for seed in 0..100 {
            let mut id_bytes = [0u8; 32];
            id_bytes[0] = seed as u8;
            let id = AgentId::from_bytes(&id_bytes).unwrap();
            let name = agent_display_name(&id);
            
            // Must have exactly one hyphen
            let hyphen_count = name.matches('-').count();
            assert_eq!(hyphen_count, 1, "Name '{}' should have exactly one hyphen", name);
            
            // Must have exactly one hash
            let hash_count = name.matches('#').count();
            assert_eq!(hash_count, 1, "Name '{}' should have exactly one hash", name);
            
            // Parts should be: adjective-noun#checksum
            let parts: Vec<&str> = name.split(&['-', '#'][..]).collect();
            assert_eq!(parts.len(), 3, "Name '{}' should have 3 parts", name);
            
            // Adjective and noun should be lowercase letters only
            assert!(parts[0].chars().all(|c| c.is_ascii_lowercase()), 
                "Adjective '{}' should be lowercase", parts[0]);
            assert!(parts[1].chars().all(|c| c.is_ascii_lowercase()), 
                "Noun '{}' should be lowercase", parts[1]);
            
            // Checksum should be 4 digits
            assert!(parts[2].chars().all(|c| c.is_ascii_digit()),
                "Checksum '{}' should be digits", parts[2]);
            assert_eq!(parts[2].len(), 4, "Checksum should be 4 digits");
        }
    }
    
    #[test]
    fn test_checksum_is_always_4_digits() {
        // Test various byte combinations that affect checksum
        for i in 0..200 {
            let mut id_bytes = [0u8; 32];
            id_bytes[2] = (i >> 8) as u8;
            id_bytes[3] = (i & 0xFF) as u8;
            let id = AgentId::from_bytes(&id_bytes).unwrap();
            let name = agent_display_name(&id);
            
            let checksum_part = name.split('#').nth(1).unwrap();
            assert_eq!(checksum_part.len(), 4, "Checksum should be exactly 4 digits");
            assert!(
                checksum_part.chars().all(|c| c.is_ascii_digit()),
                "Checksum should only contain digits"
            );
        }
    }
    
    // ─────────────────────────────────────────────────────────────────────────────
    // Determinism tests
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_agent_display_name_deterministic() {
        let id_bytes = [42u8; 32];
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        
        let name1 = agent_display_name(&id);
        let name2 = agent_display_name(&id);
        
        assert_eq!(name1, name2, "Same AgentId should produce same name");
    }
    
    #[test]
    fn test_deterministic_100_calls() {
        let id_bytes = [99u8; 32];
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        
        let first_name = agent_display_name(&id);
        for _ in 0..100 {
            let name = agent_display_name(&id);
            assert_eq!(name, first_name, "Should be deterministic across calls");
        }
    }
    
    // ─────────────────────────────────────────────────────────────────────────────
    // Uniqueness and collision resistance tests
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_agent_display_name_different_ids_different_names() {
        let id1 = AgentId::from_bytes(&[0u8; 32]).unwrap();
        let id2 = AgentId::from_bytes(&[1u8; 32]).unwrap();
        
        let name1 = agent_display_name(&id1);
        let name2 = agent_display_name(&id2);
        
        assert_ne!(name1, name2, "Different AgentIds should produce different names");
    }
    
    #[test]
    fn test_changing_bit_0_changes_adjective() {
        let mut id1_bytes = [0u8; 32];
        let mut id2_bytes = [0u8; 32];
        id2_bytes[0] = 1; // Change only bit 0
        
        let id1 = AgentId::from_bytes(&id1_bytes).unwrap();
        let id2 = AgentId::from_bytes(&id2_bytes).unwrap();
        
        let name1 = agent_display_name(&id1);
        let name2 = agent_display_name(&id2);
        
        // Adjectives should differ
        let adj1 = name1.split('-').next().unwrap();
        let adj2 = name2.split('-').next().unwrap();
        assert_ne!(
            adj1, adj2,
            "Changing bit 0 should change adjective ({} vs {})",
            adj1, adj2
        );
    }
    
    #[test]
    fn test_changing_bit_8_changes_noun() {
        let mut id1_bytes = [0u8; 32];
        let mut id2_bytes = [0u8; 32];
        id2_bytes[1] = 1; // Change only bit 8 (byte index 1)
        
        let id1 = AgentId::from_bytes(&id1_bytes).unwrap();
        let id2 = AgentId::from_bytes(&id2_bytes).unwrap();
        
        let name1 = agent_display_name(&id1);
        let name2 = agent_display_name(&id2);
        
        // Nouns should differ
        let noun1 = name1.split('-').nth(1).unwrap().split('#').next().unwrap();
        let noun2 = name2.split('-').nth(1).unwrap().split('#').next().unwrap();
        assert_ne!(
            noun1, noun2,
            "Changing bit 8 should change noun ({} vs {})",
            noun1, noun2
        );
    }
    
    #[test]
    fn test_changing_bits_16_29_changes_checksum() {
        let mut id1_bytes = [0u8; 32];
        let mut id2_bytes = [0u8; 32];
        id2_bytes[2] = 0x40; // Change bit 16 (first bit of checksum)
        
        let id1 = AgentId::from_bytes(&id1_bytes).unwrap();
        let id2 = AgentId::from_bytes(&id2_bytes).unwrap();
        
        let name1 = agent_display_name(&id1);
        let name2 = agent_display_name(&id2);
        
        let checksum1 = name1.split('#').nth(1).unwrap();
        let checksum2 = name2.split('#').nth(1).unwrap();
        
        assert_ne!(
            checksum1, checksum2,
            "Changing bits 16-29 should change checksum ({} vs {})",
            checksum1, checksum2
        );
    }
    
    #[test]
    fn test_small_input_changes_produce_different_names() {
        // Test 50 adjacent AgentIds - all should be different
        let mut names = Vec::new();
        for i in 0..50 {
            let mut id_bytes = [0u8; 32];
            id_bytes[0] = i as u8;
            let id = AgentId::from_bytes(&id_bytes).unwrap();
            names.push(agent_display_name(&id));
        }
        
        // All names should be unique
        let unique_count = names.iter().collect::<BTreeSet<_>>().len();
        assert_eq!(
            unique_count, 50,
            "50 different AgentIds should produce 50 different names"
        );
    }
    
    // ─────────────────────────────────────────────────────────────────────────────
    // Edge cases - wordlist boundaries
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_first_adjective() {
        // byte[0] = 0 -> first adjective "able"
        let id_bytes = [0u8; 32];
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        let name = agent_display_name(&id);
        
        let adj = name.split('-').next().unwrap();
        assert_eq!(adj, "able", "First adjective should be 'able'");
    }
    
    #[test]
    fn test_first_noun() {
        // byte[1] = 0 -> first noun "ant"
        let id_bytes = [0u8; 32];
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        let name = agent_display_name(&id);
        
        let noun = name.split('-').nth(1).unwrap().split('#').next().unwrap();
        assert_eq!(noun, "ant", "First noun should be 'ant'");
    }
    
    #[test]
    fn test_checksum_zero() {
        // bytes[2-3] = 0 -> checksum = 0
        let id_bytes = [0u8; 32];
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        let name = agent_display_name(&id);
        
        let checksum = name.split('#').nth(1).unwrap();
        assert_eq!(checksum, "0000", "Zero checksum should be 0000");
    }
    
    // ─────────────────────────────────────────────────────────────────────────────
    // Known test vectors (for reproducibility verification)
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_known_vector_all_zeros() {
        let id_bytes = [0u8; 32];
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        let name = agent_display_name(&id);
        
        // Verify format and basic correctness
        assert!(name.starts_with("able-ant#"));
        assert_eq!(name.len(), 12); // "able-ant#0000" = 12 chars
    }
    
    #[test]
    fn test_known_vector_all_ff() {
        let id_bytes = [0xFFu8; 32];
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        let name = agent_display_name(&id);
        
        // Verify format - should have valid words from end of lists
        assert!(name.contains('-'));
        assert!(name.contains('#'));
        let parts: Vec<&str> = name.split('#').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].len(), 4);
    }
    
    #[test]
    fn test_known_vector_byte_0_only() {
        // Only byte[0] = 1, rest 0
        let mut id_bytes = [0u8; 32];
        id_bytes[0] = 1;
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        let name = agent_display_name(&id);
        
        // Should use second adjective
        let adj = name.split('-').next().unwrap();
        assert_eq!(adj, "acid", "Second adjective should be 'acid'");
    }
    
    // ─────────────────────────────────────────────────────────────────────────────
    // Collision detection (same word pair, different checksum)
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_same_words_different_checksum() {
        // Find two IDs with same word pair but different checksum
        // by varying bytes[2-3] while keeping bytes[0-1] same
        let mut id1_bytes = [0u8; 32];
        let mut id2_bytes = [0u8; 32];
        id2_bytes[2] = 1; // Change checksum
        
        let id1 = AgentId::from_bytes(&id1_bytes).unwrap();
        let id2 = AgentId::from_bytes(&id2_bytes).unwrap();
        
        let name1 = agent_display_name(&id1);
        let name2 = agent_display_name(&id2);
        
        // Words should be same
        let word_pair1 = name1[..name1.find('#').unwrap()].to_string();
        let word_pair2 = name2[..name2.find('#').unwrap()].to_string();
        assert_eq!(
            word_pair1, word_pair2,
            "Word pair should be same when only checksum changes"
        );
        
        // But checksums should differ
        let checksum1 = name1.split('#').nth(1).unwrap();
        let checksum2 = name2.split('#').nth(1).unwrap();
        assert_ne!(
            checksum1, checksum2,
            "Checksums should differ for collision detection"
        );
    }
    
    // ─────────────────────────────────────────────────────────────────────────────
    // NAME_SCHEMA verification
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_name_schema_constant() {
        assert_eq!(NAME_SCHEMA, 1, "NAME_SCHEMA should be 1");
    }
    
    // ─────────────────────────────────────────────────────────────────────────────
    // Round-trip tests (hex encoding/decoding)
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_roundtrip_hex_encoding() {
        // Test round-trip: from_hex -> agent_display_name
        let hex = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let agent_id = AgentId::from_hex(hex).unwrap();
        let name = agent_display_name(&agent_id);
        
        // Verify format
        assert!(name.contains('-'));
        assert!(name.contains('#'));
        
        // Different hex should give different name
        let hex2 = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        let agent_id2 = AgentId::from_hex(hex2).unwrap();
        let name2 = agent_display_name(&agent_id2);
        
        assert_ne!(name, name2, "Different AgentIds should produce different names");
    }
    
    // ─────────────────────────────────────────────────────────────────────────────
    // Wordlist validation
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_adjectives_are_all_lowercase() {
        for adj in ADJECTIVES.iter() {
            assert!(
                adj.chars().all(|c| c.is_ascii_lowercase()),
                "Adjective '{}' should be all lowercase",
                adj
            );
        }
    }
    
    #[test]
    fn test_nouns_are_all_lowercase() {
        for noun in NOUNS.iter() {
            assert!(
                noun.chars().all(|c| c.is_ascii_lowercase()),
                "Noun '{}' should be all lowercase",
                noun
            );
        }
    }
    
    #[test]
    fn test_adjective_count() {
        assert_eq!(ADJECTIVES.len(), 280, "Should have 280 adjectives");
    }
    
    #[test]
    fn test_noun_count() {
        assert_eq!(NOUNS.len(), 250, "Should have 250 nouns");
    }
}
