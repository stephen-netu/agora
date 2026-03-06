use serde::{Deserialize, Serialize};

/// Matrix m.reaction event type constant
pub const REACTION_EVENT_TYPE: &str = "m.reaction";

/// Relation type for annotations (reactions)
pub const ANNOTATION_REL_TYPE: &str = "m.annotation";

/// Content for m.reaction events.
/// Reactions are annotations attached to existing events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionContent {
    /// The relationship to the target event
    #[serde(rename = "m.relates_to")]
    pub relates_to: Relation,
}

/// Defines the relationship between this event and another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    /// The type of relationship - always "m.annotation" for reactions
    #[serde(rename = "rel_type")]
    pub rel_type: String,
    /// The event ID being reacted to
    #[serde(rename = "event_id")]
    pub event_id: String,
    /// The reaction key (emoji)
    pub key: String,
}

impl ReactionContent {
    /// Create a new reaction content targeting a specific event with a given emoji
    pub fn new(event_id: String, emoji: String) -> Self {
        Self {
            relates_to: Relation {
                rel_type: ANNOTATION_REL_TYPE.to_owned(),
                event_id,
                key: emoji,
            },
        }
    }
}

/// Aggregated reactions for a message.
/// Maps emoji -> list of user IDs who reacted with that emoji.
pub type ReactionAggregations = std::collections::HashMap<String, Vec<String>>;

/// Helper function to aggregate reactions from a list of reaction events.
/// Takes reaction events and returns aggregated counts.
pub fn aggregate_reactions(reaction_events: &[super::RoomEvent]) -> ReactionAggregations {
    let mut aggregations: ReactionAggregations = std::collections::HashMap::new();

    for event in reaction_events {
        if event.event_type != REACTION_EVENT_TYPE {
            continue;
        }

        // Parse the reaction content
        if let Ok(content) = serde_json::from_value::<ReactionContent>(event.content.clone()) {
            let emoji = content.relates_to.key;
            let sender = event.sender.as_str().to_owned();

            aggregations.entry(emoji).or_default().push(sender);
        }
    }

    aggregations
}

/// Common emoji reactions for quick selection
pub const COMMON_REACTIONS: &[&str] = &["👍", "👎", "❤️", "😂", "😮", "😢", "🎉", "🔥", "👏", "🤔"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifiers::{EventId, RoomId, UserId};
    use serde_json::json;

    fn create_test_event(
        event_id: &str,
        sender: &str,
        target_event_id: &str,
        emoji: &str,
    ) -> super::RoomEvent {
        super::RoomEvent {
            event_id: EventId::parse(event_id).unwrap(),
            room_id: RoomId::parse("!test:example.com").unwrap(),
            sender: UserId::parse(sender).unwrap(),
            event_type: REACTION_EVENT_TYPE.to_owned(),
            state_key: None,
            content: json!({
                "m.relates_to": {
                    "rel_type": "m.annotation",
                    "event_id": target_event_id,
                    "key": emoji
                }
            }),
            origin_server_ts: 1234567890,
            stream_ordering: None,
        }
    }

    #[test]
    fn test_aggregate_reactions() {
        let events = vec![
            create_test_event("$1", "@alice:example.com", "$target", "👍"),
            create_test_event("$2", "@bob:example.com", "$target", "👍"),
            create_test_event("$3", "@charlie:example.com", "$target", "❤️"),
        ];

        let aggregated = aggregate_reactions(&events);

        assert_eq!(aggregated.get("👍").unwrap().len(), 2);
        assert_eq!(aggregated.get("❤️").unwrap().len(), 1);
        assert!(aggregated
            .get("👍")
            .unwrap()
            .contains(&"@alice:example.com".to_owned()));
        assert!(aggregated
            .get("👍")
            .unwrap()
            .contains(&"@bob:example.com".to_owned()));
    }

    #[test]
    fn test_reaction_content_serialization() {
        let content = ReactionContent::new("$target:example.com".to_owned(), "👍".to_owned());
        let json = serde_json::to_value(&content).unwrap();

        assert_eq!(
            json,
            json!({
                "m.relates_to": {
                    "rel_type": "m.annotation",
                    "event_id": "$target:example.com",
                    "key": "👍"
                }
            })
        );
    }
}
