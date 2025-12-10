use std::sync::LazyLock;

// Load and parse MAC only once
pub static NODE_ID: LazyLock<[u8; 6]> = LazyLock::new(|| {
    let node_str = std::env::var("MAC_ADD").expect("Environment variable MAC_ADD must be set");

    let node_vec: Vec<u8> = node_str
        .split(':')
        .map(|s| u8::from_str_radix(s, 16).expect("Invalid Hex Component"))
        .collect();

    node_vec
        .try_into()
        .expect("MAC_ADD must contain exactly 6 bytes, e.g. 00:01:02:03:04:05")
});
