use  common::*;

enum Morality {
    // \/ node responded to us within the last 15 mins
    // \/ /\ responded to us in the past
    //    /\ queried us in the last 15 mins
    Good,
    // inactive for 15 minutes
    Questionable,
    // "failed to respond to multiple queries in a row"
    Bad,
}

struct Node {
    node_id: [u8; 20],
    buckets: BTreeMap<u8, Vec<([u8;4], u16, u32)>>,
    announce_cache: HashMap<[u8;20], Vec<([u8;6], u32)>>,
}

impl Node {
    pub fn new() -> Self {
        Node {
            node_id: [0; 20],
            buckets: HashMap::new(),
            announce_cache: HashMap::new(),
        }
    }
}
