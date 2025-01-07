use flare_zrpc::{ZrpcClient, ZrpcTypeConfig};

use super::msg::{ShardReq, ShardResp};

struct OperationProxy<T>
where
    T: ZrpcTypeConfig<In = ShardReq, Out = ShardResp>,
{
    rpc: ZrpcClient<T>,
}
