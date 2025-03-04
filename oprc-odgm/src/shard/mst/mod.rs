mod msg;
mod sync;
// mod v1;
mod v2;

use merkle_search_tree::MerkleSearchTree;
pub use msg::NetworkPage;
pub use msg::{Key, LoadPageReq, PageRangeMessage, PagesResp};
pub use sync::PageQueryType;
pub use v2::ObjectMstShard;

use super::ObjectEntry;

type MST = MerkleSearchTree<Key, ObjectEntry>;

#[allow(dead_code)]
type MessageSerde = flare_zrpc::bincode::BincodeMsgSerde<PageRangeMessage>;
