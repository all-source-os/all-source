use serde_json::json;

use super::{Direction, HopDirection, Prime, node_entity_id};

async fn node(prime: &Prime, ty: &str, name: &str) -> String {
    let id = prime.add_node(ty, json!({ "name": name })).await.unwrap();
    node_entity_id(ty, id.as_str())
}

/// `A -requires-> T <-requires- B`: the shape from gh#438.
async fn shared_tool() -> (Prime, String, String, String) {
    let prime = Prime::open_in_memory().await.unwrap();
    let a = node(&prime, "skill", "A").await;
    let b = node(&prime, "skill", "B").await;
    let t = node(&prime, "tool", "T").await;
    prime.add_edge(&a, &t, "requires", None).await.unwrap();
    prime.add_edge(&b, &t, "requires", None).await.unwrap();
    (prime, a, b, t)
}

#[tokio::test]
async fn outgoing_only_finds_no_path_through_a_shared_target() {
    let (prime, a, b, _) = shared_tool().await;
    assert!(prime.shortest_path(&a, &b, None).is_none());
    assert!(
        prime
            .shortest_path_directed(&a, &b, None, Direction::Outgoing)
            .is_none()
    );
}

#[tokio::test]
async fn both_directions_find_the_path_and_report_each_hop() {
    let (prime, a, b, t) = shared_tool().await;
    let path = prime
        .shortest_path_directed(&a, &b, None, Direction::Both)
        .unwrap();

    let names: Vec<&str> = path
        .nodes
        .iter()
        .map(|n| n.properties["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["A", "T", "B"]);

    assert_eq!(path.hops.len(), 2);
    assert_eq!(
        (
            path.hops[0].from.as_str(),
            path.hops[0].to.as_str(),
            path.hops[0].direction
        ),
        (a.as_str(), t.as_str(), HopDirection::Outgoing)
    );
    assert_eq!(
        (
            path.hops[1].from.as_str(),
            path.hops[1].to.as_str(),
            path.hops[1].direction
        ),
        (t.as_str(), b.as_str(), HopDirection::Incoming)
    );
    assert!(path.hops.iter().all(|h| h.relation == "requires"));
}

#[tokio::test]
async fn incoming_walks_edges_backwards() {
    let (prime, a, _, t) = shared_tool().await;
    let path = prime
        .shortest_path_directed(&t, &a, None, Direction::Incoming)
        .unwrap();
    assert_eq!(path.nodes.len(), 2);
    assert_eq!(path.hops[0].direction, HopDirection::Incoming);
    assert!(
        prime
            .shortest_path_directed(&a, &t, None, Direction::Incoming)
            .is_none()
    );
}

#[tokio::test]
async fn relation_filter_applies_in_both_directions() {
    let (prime, a, b, _) = shared_tool().await;
    assert!(
        prime
            .shortest_path_directed(&a, &b, Some("uses"), Direction::Both)
            .is_none()
    );
    assert!(
        prime
            .shortest_path_directed(&a, &b, Some("requires"), Direction::Both)
            .is_some()
    );
}

#[tokio::test]
async fn path_to_self_has_no_hops() {
    let (prime, a, _, _) = shared_tool().await;
    let path = prime
        .shortest_path_directed(&a, &a, None, Direction::Both)
        .unwrap();
    assert_eq!(path.nodes.len(), 1);
    assert!(path.hops.is_empty());
}

#[test]
fn direction_parses_the_wire_spelling_and_names_a_bad_one() {
    assert_eq!("both".parse::<Direction>(), Ok(Direction::Both));
    assert_eq!("incoming".parse::<Direction>(), Ok(Direction::Incoming));
    assert_eq!("outgoing".parse::<Direction>(), Ok(Direction::Outgoing));
    let err = "sideways".parse::<Direction>().unwrap_err();
    assert!(err.contains("sideways"), "{err}");
}
