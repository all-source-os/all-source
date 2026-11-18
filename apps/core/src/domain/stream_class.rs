//! Stream class: the tenant-free name of what a read targets.
//!
//! Core labels metrics and logs with this name, and Longhand classifies its
//! reads with the same rule, so the vectors below are the shared contract.

/// Class for a request that names no entity id, event-type prefix or event type.
pub const UNSCOPED: &str = "unscoped";
/// Class for an entity-id request whose id has no segment that qualifies.
pub const ENTITY: &str = "entity";
/// Class for a result that fails the class pattern.
pub const UNCLASSIFIED: &str = "unclassified";

/// Classifies a read by the filters it carries. An entity id decides the class
/// on its own; otherwise the event-type prefix, then the event type with its
/// last `.`-segment dropped.
pub fn stream_class(
    entity_id: Option<&str>,
    event_type_prefix: Option<&str>,
    event_type: Option<&str>,
) -> String {
    stream_class_for_tenant(None, entity_id, event_type_prefix, event_type)
}

/// [`stream_class`] for a read by `tenant`: an entity id whose first segment
/// is exactly `tenant` skips that segment, so the requesting tenant's own id
/// never becomes a class. Another tenant's all-letter id still can.
pub fn stream_class_for_tenant(
    tenant: Option<&str>,
    entity_id: Option<&str>,
    event_type_prefix: Option<&str>,
    event_type: Option<&str>,
) -> String {
    let raw = if let Some(id) = entity_id {
        entity_segment(id, tenant).unwrap_or_else(|| ENTITY.to_owned())
    } else if let Some(prefix) = non_empty(event_type_prefix) {
        prefix.strip_suffix('.').unwrap_or(prefix).to_owned()
    } else if let Some(event_type) = non_empty(event_type) {
        event_type
            .rsplit_once('.')
            .map_or(event_type, |(head, _)| head)
            .to_owned()
    } else {
        return UNSCOPED.to_owned();
    };
    finish(&raw)
}

/// Whether `class` matches `^[a-z][a-z0-9_.]{0,63}$`.
pub fn is_class(class: &str) -> bool {
    let mut bytes = class.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && class.len() <= MAX_LEN
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.'))
}

const MAX_LEN: usize = 64;

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|text| !text.is_empty())
}

/// The first segment that starts with a letter and holds no digit, with `-`
/// read as `_`. Segments that hold a digit are skipped so an id never
/// becomes a class.
fn entity_segment(entity_id: &str, tenant: Option<&str>) -> Option<String> {
    entity_id
        .split(':')
        .enumerate()
        .find(|&(position, segment)| {
            !(position == 0 && tenant == Some(segment))
                && segment.starts_with(|c: char| c.is_ascii_alphabetic())
                && !segment.contains(|c: char| c.is_ascii_digit())
        })
        .map(|(_, segment)| segment.replace('-', "_"))
}

fn finish(raw: &str) -> String {
    let class: String = raw
        .chars()
        .take(MAX_LEN)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if is_class(&class) {
        class
    } else {
        UNCLASSIFIED.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class_of_entity(entity_id: &str) -> String {
        stream_class(Some(entity_id), None, None)
    }

    #[test]
    fn stream_class_entity_takes_the_first_letter_only_segment() {
        assert_eq!(
            stream_class(Some("tenant-1:user-2:skill_run:9f3"), None, None),
            "skill_run"
        );
    }

    #[test]
    fn stream_class_entity_maps_hyphens_to_underscores() {
        assert_eq!(class_of_entity("auth-member:abc123"), "auth_member");
    }

    #[test]
    fn stream_class_design_vectors_hold() {
        assert_eq!(
            class_of_entity("3f2a9c1e-77b0-4c1d-9e1a-0b5d2c8f6a41:user_7f:skill_run:01hx9z"),
            "skill_run"
        );
        assert_eq!(class_of_entity("auth-member:abc"), "auth_member");
        assert_eq!(class_of_entity("01abc:02def"), ENTITY);
        assert_eq!(class_of_entity("au$th:x"), UNCLASSIFIED);
    }

    #[test]
    fn stream_class_digit_free_first_segment_is_the_class() {
        assert_eq!(class_of_entity("acme-corp:4f2"), "acme_corp");
    }

    #[test]
    fn stream_class_prefix_drops_only_one_trailing_dot() {
        assert_eq!(stream_class(None, Some("auth.."), None), "auth.");
    }

    #[test]
    fn stream_class_entity_lowercases_the_segment() {
        assert_eq!(class_of_entity("Auth-Member:x"), "auth_member");
    }

    #[test]
    fn stream_class_entity_without_a_qualifying_segment_is_entity() {
        assert_eq!(class_of_entity("01abc:02def"), ENTITY);
    }

    #[test]
    fn stream_class_entity_outranks_prefix_and_event_type() {
        assert_eq!(
            stream_class(
                Some("01abc"),
                Some("auth.member."),
                Some("auth.member.updated")
            ),
            ENTITY
        );
    }

    #[test]
    fn stream_class_entity_with_a_bad_segment_is_unclassified() {
        assert_eq!(class_of_entity("au$th:x"), UNCLASSIFIED);
    }

    #[test]
    fn stream_class_system_workspace_id_is_longhand() {
        assert_eq!(class_of_entity("longhand:u1:skill_run:x"), "longhand");
    }

    fn class_in(tenant: &str, entity_id: &str) -> String {
        stream_class_for_tenant(Some(tenant), Some(entity_id), None, None)
    }

    #[test]
    fn stream_class_skips_a_first_segment_equal_to_the_requesting_tenant() {
        assert_eq!(class_in("longhand", "longhand:u1:skill_run:x"), "skill_run");
        assert_eq!(class_in("acme", "acme:auth-member:x1"), "auth_member");
        assert_eq!(class_in("acme", "acme:01abc"), ENTITY);
    }

    #[test]
    fn stream_class_keeps_a_tenant_like_segment_of_another_tenant_or_position() {
        assert_eq!(class_in("acme", "longhand:u1:skill_run:x"), "longhand");
        assert_eq!(class_in("acme", "u1:acme:skill_run"), "acme");
        assert_eq!(class_in("tenant-1", "tenant:x"), "tenant");
    }

    #[test]
    fn stream_class_skip_compares_the_raw_segment_not_the_class() {
        assert_eq!(class_in("Acme", "acme:skill_run"), "acme");
        assert_eq!(class_in("acme-corp", "acme-corp:skill_run:1"), "skill_run");
    }

    #[test]
    fn stream_class_without_an_entity_ignores_the_tenant() {
        assert_eq!(
            stream_class_for_tenant(Some("auth"), None, Some("auth.member."), None),
            "auth.member"
        );
    }

    #[test]
    fn stream_class_prefix_drops_the_trailing_dot() {
        assert_eq!(
            stream_class(None, Some("auth.member."), None),
            "auth.member"
        );
    }

    #[test]
    fn stream_class_event_type_drops_its_last_segment() {
        assert_eq!(
            stream_class(None, None, Some("auth.member.updated")),
            "auth.member"
        );
    }

    #[test]
    fn stream_class_single_segment_event_type_is_kept() {
        assert_eq!(stream_class(None, None, Some("heartbeat")), "heartbeat");
    }

    #[test]
    fn stream_class_without_filters_is_unscoped() {
        assert_eq!(stream_class(None, None, None), UNSCOPED);
    }

    #[test]
    fn stream_class_prefix_outranks_event_type() {
        assert_eq!(
            stream_class(None, Some("billing."), Some("auth.member.updated")),
            "billing"
        );
    }

    #[test]
    fn stream_class_is_cut_to_64_characters() {
        let long = "a".repeat(70);
        assert_eq!(class_of_entity(&long), "a".repeat(64));
    }

    #[test]
    fn stream_class_hyphen_outside_an_entity_id_is_unclassified() {
        assert_eq!(stream_class(None, Some("bad-name."), None), UNCLASSIFIED);
    }

    #[test]
    fn is_class_accepts_the_pattern_and_rejects_everything_else() {
        assert!(is_class("auth.member"));
        assert!(is_class("a"));
        assert!(!is_class(""));
        assert!(!is_class("1abc"));
        assert!(!is_class("Auth"));
        assert!(!is_class("a-b"));
        assert!(!is_class(&"a".repeat(65)));
    }
}
