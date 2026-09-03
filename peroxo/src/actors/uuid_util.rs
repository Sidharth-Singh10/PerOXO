use std::sync::LazyLock;

// Node identifier for UUID v1 generation (time-ordered message ids).
//
// Uses MAC_ADD when a valid one is provided, otherwise derives a
// process-unique node id from hostname + pid + a time-based salt with the
// multicast bit set (per IEEE 802, so it can never collide with a real MAC).
// Negative space instead of panicking: an unset or malformed MAC_ADD must
// not take the whole message path down.
pub static NODE_ID: LazyLock<[u8; 6]> = LazyLock::new(|| {
    if let Ok(mac) = std::env::var("MAC_ADD") {
        if let Some(node) = parse_node_id(&mac) {
            return node;
        }
        tracing::warn!(mac = %mac, "Invalid MAC_ADD, using generated node id");
    }

    generated_node_id()
});

fn parse_node_id(s: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return None;
    }

    let mut node = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        node[i] = u8::from_str_radix(part, 16).ok()?;
    }
    Some(node)
}

fn generated_node_id() -> [u8; 6] {
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::env::var("HOSTNAME").unwrap_or_default().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default()
        .hash(&mut hasher);

    let hash = hasher.finish();

    let mut node = [0u8; 6];
    node.copy_from_slice(&hash.to_be_bytes()[2..]);
    node[0] |= 0x01; // multicast bit: guaranteed distinct from real MAC addresses
    node
}