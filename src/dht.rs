// =====
// BEP0005
//
// /\ The DHT KRPC has three message types
//
//      msg.type ∈ { query, response, error }
//
// and four methods
//
//      msg.method ∈ { ping, find_node, get_peers, announce_peer }
//
// /\ Messages on the wire are a bencoded dict that must contains the keys:
//
//      msg['t'] ∈ transaction_id
//      msg['y'] ∈ msg.type \in N
//
// and may contain the key:
//
//      msg['v'] = client string (user agent) BEP20
//
// /\ Query messages (msg['y'] == 'q') add two additional keys:
//
//      msg['q'] ∈ msg.method
//      msg['a'] ∈ msg.method.args
//
// /\ Responses (msg['y'] == 'r') add one additional key:
//
//      'r':    the result of the query
//
// /\ Errors:
//
//      'e':    a tuple of (error code, error msg) encoded as a list

// ==========
// Queries
//
// =====
// ::ping
//
//      req['q'] = "ping"
//      req['a'] = {"id" : <Alice>}
//  ------------------------------------
//      resp['y'] = 'r'
//      resp['r'] = {'id' : <Bob>}
//
// =====
// ::find_node: find the contact information for a node with the id = target
//
//          tx['q'] = "find_node"
//          tx['a'] = {'id':<id>, 'target':<target id> }
//  DHT -------------------------------------------------- DHT
//          rx['y'] = 'r'
//          rx['r'] = {
//              'id': <sender id> ,
//              'nodes': <compact node info>
//          }
//
// =====
// ::get_peers
//
//      req['q'] = "get_peers"
//      req['a'] = {"id" : <Alice>, "info_hash": <infohash> }
//  ------------------------------------
//      resp['y'] = 'r'
//      resp['r'] = {'id' : <Bob>}
//
//  Get peers associated with a torrent infohash. "q" = "get_peers" A get_peers
//  query has two arguments, "id" containing the node ID of the querying node,
//  and "info_hash" containing the infohash of the torrent. If the queried node
//  has peers for the infohash, they are returned in a key "values" as a list of
//  strings. Each string containing "compact" format peer information for a
//  single peer. If the queried node has no peers for the infohash, a key
//  "nodes" is returned containing the K nodes in the queried nodes routing
//  table closest to the infohash supplied in the query. In either case a
//  "token" key is also included in the return value. The token value is a
//  required argument for a future announce_peer query. The token value should
//  be a short binary string.
//
//  ::announce_peer
//
//  Announce that the peer, controlling the querying node, is downloading a
//  torrent on a port.  announce_peer has four arguments: "id" containing the
//  node ID of the querying node, "info_hash" containing the infohash of the
//  torrent, "port" containing the port as an integer, and the "token" received
//  in response to a previous get_peers query. The queried node must verify that
//  the token was previously sent to the same IP address as the querying node.
//  Then the queried node should store the IP address of the querying node and
//  the supplied port number under the infohash in its store of peer contact
//  information.
//
//  There is an optional argument called implied_port which value is either 0 or
//  1. If it is present and non-zero, the port argument should be ignored and
//  the source port of the UDP packet should be used as the peer's port instead.
//  This is useful for peers behind a NAT that may not know their external port,
//  and supporting uTP, they accept incoming connections on the same port as the
//  DHT port.

//  These bootstrap nodes are probably worth hardcoding
static HARDCODED_BOOTSTRAP_NODES = [
    "router.utorrent.com:6881",
    "router.bittorrent.com:6881",
    "dht.transmissionbt.com:6881",
    "router.bitcomet.com:6881",
    "dht.aelitis.com:6881",
];
