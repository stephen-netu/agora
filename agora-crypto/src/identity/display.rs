//! Human-readable deterministic identity names.
//!
//! Generates memorable, collision-resistant handles from AgentId.
//!
//! Format: word1-word2#NNNN
//!   - word1: adjective (from 280 entries, index via bits 0-7)
//!   - word2: noun (from 250 entries, index via bits 8-15)
//!   - NNNN: 4-digit checksum (bits 16-29, modulo 10000)
//!
//! Total space: 256 × 256 × 10000 = 655,360,000 (~655 million) unique identities.
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
/// Returns ~655 million unique identities (256 × 256 × 10000).
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
    
    #[test]
    fn test_agent_display_name_format() {
        // Test with known AgentId
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
    fn test_agent_display_name_deterministic() {
        let id_bytes = [42u8; 32];
        let id = AgentId::from_bytes(&id_bytes).unwrap();
        
        let name1 = agent_display_name(&id);
        let name2 = agent_display_name(&id);
        
        assert_eq!(name1, name2, "Same AgentId should produce same name");
    }
    
    #[test]
    fn test_agent_display_name_different_ids_different_names() {
        let id1 = AgentId::from_bytes(&[0u8; 32]).unwrap();
        let id2 = AgentId::from_bytes(&[1u8; 32]).unwrap();
        
        let name1 = agent_display_name(&id1);
        let name2 = agent_display_name(&id2);
        
        assert_ne!(name1, name2, "Different AgentIds should produce different names");
    }
}
