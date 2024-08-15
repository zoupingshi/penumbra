use {
    penumbra_proto::{
        tendermint::p2p::DefaultNodeInfo,
        util::tendermint_proxy::v1::{
            tendermint_proxy_service_server::TendermintProxyService, AbciQueryRequest,
            AbciQueryResponse, BroadcastTxAsyncRequest, BroadcastTxAsyncResponse,
            BroadcastTxSyncRequest, BroadcastTxSyncResponse, GetBlockByHeightRequest,
            GetBlockByHeightResponse, GetStatusRequest, GetStatusResponse, GetTxRequest,
            GetTxResponse, SyncInfo,
        },
    },
    std::{
        collections::BTreeMap,
        sync::{Arc, RwLock},
    },
    tap::{Tap, TapFallible, TapOptional},
    tendermint::{
        block::{Block, Height},
        Time,
    },
    tonic::Status,
    tracing::instrument,
};

/// A tendermint proxy service for use in tests.
///
/// This type implements [`TendermintProxyService`], but can be configured to report the blocks
/// generated by a [`penumbra_mock_consensus::TestNode`].
#[derive(Default)]
pub struct TestNodeProxy {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    /// A map of the [`Blocks`] that have been seen so far, keyed by [`Height`].
    blocks: RwLock<BTreeMap<Height, Block>>,
}

impl TestNodeProxy {
    /// Creates a new [`TestNodeProxy`].
    pub fn new<C>() -> Self {
        Default::default()
    }

    /// Returns a boxed function that will add [`Blocks`] to this proxy.
    pub fn on_block_callback(&self) -> penumbra_mock_consensus::OnBlockFn {
        // Create a new reference to the shared map of blocks we've seen.
        let Self { inner } = self;
        let inner = Arc::clone(inner);

        Box::new(move |block| inner.on_block(block))
    }

    /// Returns the latest block height.
    fn latest_block_height(&self) -> tendermint::block::Height {
        self.inner
            .blocks()
            .last_key_value()
            .map(|(height, _)| *height)
            .expect("blocks should not be empty")
    }

    /// Returns the latest block timestamp.
    fn timestamp(&self) -> Time {
        self.inner
            .blocks()
            .last_key_value()
            .map(|(_, block)| block)
            .expect("blocks should not be empty")
            .header
            .time
    }
}

impl Inner {
    #[instrument(level = "debug", skip_all)]
    fn on_block(&self, block: tendermint::Block) {
        // Add this block to the proxy's book-keeping.
        let height = block.header.height;
        self.blocks_mut()
            .insert(height, block)
            .map(|_overwritten| {
                // ...or panic if we have been given block with duplicate heights.
                panic!("proxy received two blocks with height {height}");
            })
            .tap_none(|| {
                tracing::debug!(?height, "received block");
            });
    }

    /// Acquires a write-lock on the map of blocks we have seen before.
    fn blocks(&self) -> std::sync::RwLockReadGuard<'_, BTreeMap<Height, Block>> {
        let Self { blocks } = self;
        blocks
            .tap(|_| tracing::trace!("acquiring read lock"))
            .read()
            .tap(|_| tracing::trace!("acquired read lock"))
            .tap_err(|_| tracing::error!("failed to acquire read lock"))
            .expect("block lock should never be poisoned")
    }

    /// Acquires a write-lock on the map of blocks we have seen before.
    fn blocks_mut(&self) -> std::sync::RwLockWriteGuard<'_, BTreeMap<Height, Block>> {
        let Self { blocks } = self;
        blocks
            .tap(|_| tracing::trace!("acquiring write lock"))
            .write()
            .tap(|_| tracing::trace!("acquired write lock"))
            .tap_err(|_| tracing::error!("failed to acquire write lock"))
            .expect("block lock should never be poisoned")
    }
}

#[tonic::async_trait]
impl TendermintProxyService for TestNodeProxy {
    async fn get_tx(
        &self,
        _req: tonic::Request<GetTxRequest>,
    ) -> Result<tonic::Response<GetTxResponse>, Status> {
        Err(Status::unimplemented("get_tx"))
    }

    /// Broadcasts a transaction asynchronously.
    #[instrument(
        level = "info",
        skip_all,
        fields(req_id = tracing::field::Empty),
    )]
    async fn broadcast_tx_async(
        &self,
        _req: tonic::Request<BroadcastTxAsyncRequest>,
    ) -> Result<tonic::Response<BroadcastTxAsyncResponse>, Status> {
        Ok(tonic::Response::new(BroadcastTxAsyncResponse {
            code: 0,
            data: Vec::default(),
            log: String::default(),
            hash: Vec::default(),
        }))
    }

    // Broadcasts a transaction synchronously.
    #[instrument(
        level = "info",
        skip_all,
        fields(req_id = tracing::field::Empty),
    )]
    async fn broadcast_tx_sync(
        &self,
        _req: tonic::Request<BroadcastTxSyncRequest>,
    ) -> Result<tonic::Response<BroadcastTxSyncResponse>, Status> {
        Ok(tonic::Response::new(BroadcastTxSyncResponse {
            code: 0,
            data: Vec::default(),
            log: String::default(),
            hash: Vec::default(),
        }))
    }

    // Queries the current status.
    #[instrument(level = "info", skip_all)]
    async fn get_status(
        &self,
        req: tonic::Request<GetStatusRequest>,
    ) -> Result<tonic::Response<GetStatusResponse>, Status> {
        let GetStatusRequest { .. } = req.into_inner();
        let latest_block_height = self.latest_block_height().into();
        let block_ts: tendermint_proto::google::protobuf::Timestamp = self.timestamp().into();
        let sync_info = SyncInfo {
            // TODO: these should get set
            latest_block_hash: vec![],
            latest_app_hash: vec![],
            latest_block_height,
            latest_block_time: Some(pbjson_types::Timestamp {
                seconds: block_ts.seconds,
                nanos: block_ts.nanos,
            }),
            // Tests run with a single node, so it is never catching up.
            catching_up: false,
        };

        Ok(GetStatusResponse {
            node_info: Some(DefaultNodeInfo::default()),
            sync_info: Some(sync_info),
            validator_info: Some(Default::default()),
        })
        .map(tonic::Response::new)
    }

    #[instrument(level = "info", skip_all)]
    async fn abci_query(
        &self,
        _req: tonic::Request<AbciQueryRequest>,
    ) -> Result<tonic::Response<AbciQueryResponse>, Status> {
        Err(Status::unimplemented("abci_query"))
    }

    #[instrument(level = "info", skip_all)]
    async fn get_block_by_height(
        &self,
        req: tonic::Request<GetBlockByHeightRequest>,
    ) -> Result<tonic::Response<GetBlockByHeightResponse>, Status> {
        // Parse the height from the inbound client request.
        let GetBlockByHeightRequest { height } = req.into_inner();
        let height =
            tendermint::block::Height::try_from(height).expect("height should be less than 2^63");

        let block = self
            .inner
            .blocks()
            .get(&height)
            .cloned()
            .map(penumbra_proto::tendermint::types::Block::try_from)
            .transpose()
            .or_else(|e| {
                tracing::warn!(?height, error = ?e, "proxy: error fetching blocks");
                Err(tonic::Status::internal("error fetching blocks"))
            })?;
        let block_id = block
            .as_ref() // is this off-by-one? should we be getting the id of the last commit?
            .and_then(|b| b.last_commit.as_ref())
            .and_then(|c| c.block_id.as_ref())
            .cloned();

        Ok(GetBlockByHeightResponse { block_id, block }).map(tonic::Response::new)
    }
}
