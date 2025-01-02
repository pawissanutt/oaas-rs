use std::io::Cursor;

use openraft::{
    Entry, EntryPayload, LogId, RaftTypeConfig, SnapshotMeta, StorageIOError,
    StoredMembership,
};

pub trait AppStateMachine:
    Clone + Sized + Default + Send + Sync + 'static
{
    type Req;
    type Resp;
    fn load(data: &[u8]) -> Result<Self, openraft::AnyError>;
    fn to_vec(&self) -> Result<Vec<u8>, openraft::AnyError>;
    fn apply(&mut self, req: &Self::Req) -> Self::Resp;
    fn empty_resp(&self) -> Self::Resp;
}

#[derive(Default, Clone)]

pub struct GenericStateMachine<APP, C>
where
    APP: AppStateMachine,
    C: RaftTypeConfig,
{
    pub last_applied_log: Option<LogId<C::NodeId>>,
    pub last_membership: StoredMembership<C::NodeId, C::Node>,
    pub app_data: APP,
}

impl<APP, C> GenericStateMachine<APP, C>
where
    C: RaftTypeConfig<SnapshotData = Cursor<Vec<u8>>, NodeId: Copy>,
    APP: AppStateMachine<Req = C::D, Resp = C::R>,
{
    pub fn load(
        meta: &SnapshotMeta<C::NodeId, C::Node>,
        data: &[u8],
    ) -> Result<Self, StorageIOError<C::NodeId>> {
        let app = APP::load(data).map_err(|e| {
            StorageIOError::read_snapshot(Some(meta.signature()), e)
        })?;

        let smd = Self {
            last_applied_log: meta.last_log_id,
            last_membership: meta.last_membership.clone(),
            app_data: app,
        };
        Ok(smd)
    }

    pub fn to_vec(&self) -> Result<Vec<u8>, StorageIOError<C::NodeId>> {
        self.app_data
            .to_vec()
            .map_err(|e| StorageIOError::read_state_machine(e))
    }

    pub fn apply_entry(&mut self, entry: Entry<C>) -> C::R {
        self.last_applied_log = Some(entry.log_id);

        match entry.payload {
            EntryPayload::Blank => self.app_data.empty_resp(),
            EntryPayload::Normal(ref req) => self.app_data.apply(req),
            EntryPayload::Membership(ref mem) => {
                self.last_membership =
                    StoredMembership::new(Some(entry.log_id), mem.clone());
                self.app_data.empty_resp()
            }
        }
    }
}

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use openraft::{
    storage::RaftStateMachine, RaftSnapshotBuilder, Snapshot, StorageError,
};
use tokio::sync::RwLock;

pub struct StoredSnapshot<C: RaftTypeConfig> {
    pub meta: SnapshotMeta<C::NodeId, C::Node>,
    pub data: Vec<u8>,
}

#[derive(Default)]
pub struct StateMachineStore<APP, C>
where
    C: RaftTypeConfig<SnapshotData = Cursor<Vec<u8>>, NodeId: Copy>,
    APP: AppStateMachine<Req = C::D, Resp = C::R>,
{
    /// The Raft state machine.
    pub state_machine: RwLock<GenericStateMachine<APP, C>>,

    /// Used in identifier for snapshot.v
    ///
    /// Note that concurrently created snapshots and snapshots created on different nodes
    /// are not guaranteed to have sequential `snapshot_idx` values, but this does not matter for
    /// correctness.
    snapshot_idx: AtomicU64,

    /// The last received snapshot.
    current_snapshot: RwLock<Option<StoredSnapshot<C>>>,
}

#[derive(Default, Clone)]
pub struct LocalStateMachineStore<APP, C>(Arc<StateMachineStore<APP, C>>)
where
    C: RaftTypeConfig<SnapshotData = Cursor<Vec<u8>>, NodeId: Copy>,
    APP: AppStateMachine<Req = C::D, Resp = C::R>;

impl<APP, C> RaftSnapshotBuilder<C> for LocalStateMachineStore<APP, C>
where
    C: RaftTypeConfig<SnapshotData = Cursor<Vec<u8>>, NodeId: Copy>,
    APP: AppStateMachine<Req = C::D, Resp = C::R>,
{
    #[tracing::instrument(level = "trace", skip(self))]
    async fn build_snapshot(
        &mut self,
    ) -> Result<Snapshot<C>, StorageError<C::NodeId>> {
        // Serialize the data of the state machine.
        let state_machine = self.0.state_machine.read().await;
        let data = state_machine.to_vec()?;

        let last_applied_log = state_machine.last_applied_log.to_owned();
        let last_membership = state_machine.last_membership.clone();

        // Lock the current snapshot before releasing the lock on the state machine, to avoid a race
        // condition on the written snapshot
        let mut current_snapshot = self.0.current_snapshot.write().await;
        drop(state_machine);

        let snapshot_idx =
            self.0.snapshot_idx.fetch_add(1, Ordering::Relaxed) + 1;
        let snapshot_id = if let Some(last) = last_applied_log {
            format!("{}-{}-{}", last.leader_id, last.index, snapshot_idx)
        } else {
            format!("--{}", snapshot_idx)
        };

        let meta = SnapshotMeta {
            last_log_id: last_applied_log,
            last_membership,
            snapshot_id,
        };

        let snapshot = StoredSnapshot {
            meta: meta.clone(),
            data: data.clone(),
        };

        *current_snapshot = Some(snapshot);

        Ok(Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(data)),
        })
    }
}

impl<APP, C> RaftStateMachine<C> for LocalStateMachineStore<APP, C>
where
    APP: AppStateMachine<Req = C::D, Resp = C::R>,
    C: RaftTypeConfig<
        SnapshotData = Cursor<Vec<u8>>,
        Entry = Entry<C>,
        NodeId = u64,
    >,
{
    type SnapshotBuilder = Self;

    async fn applied_state(
        &mut self,
    ) -> Result<
        (
            Option<LogId<C::NodeId>>,
            StoredMembership<C::NodeId, C::Node>,
        ),
        StorageError<C::NodeId>,
    > {
        let state_machine = self.0.state_machine.read().await;
        Ok((
            state_machine.last_applied_log,
            state_machine.last_membership.clone(),
        ))
    }

    #[tracing::instrument(level = "trace", skip(self, entries))]
    async fn apply<I>(
        &mut self,
        entries: I,
    ) -> Result<Vec<C::R>, StorageError<C::NodeId>>
    where
        I: IntoIterator<Item = C::Entry> + Send,
    {
        let mut res: Vec<C::R> = Vec::new(); //No `with_capacity`; do not know `len` of iterator
        let mut sm = self.0.state_machine.write().await;

        for entry in entries {
            res.push(sm.apply_entry(entry));
        }
        Ok(res)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<C::SnapshotData>, StorageError<C::NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    #[tracing::instrument(level = "trace", skip(self, snapshot))]
    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<C::NodeId, C::Node>,
        snapshot: Box<C::SnapshotData>,
    ) -> Result<(), StorageError<C::NodeId>> {
        tracing::info!(
            { snapshot_size = snapshot.get_ref().len() },
            "decoding snapshot for installation"
        );

        let new_snapshot = StoredSnapshot {
            meta: meta.clone(),
            data: snapshot.into_inner(),
        };

        let updated_state_machine =
            GenericStateMachine::load(meta, &new_snapshot.data[..])?;
        let mut state_machine = self.0.state_machine.write().await;
        *state_machine = updated_state_machine;

        // Lock the current snapshot before releasing the lock on the state machine, to avoid a race
        // condition on the written snapshot
        let mut current_snapshot = self.0.current_snapshot.write().await;
        drop(state_machine);

        // Update current snapshot.
        *current_snapshot = Some(new_snapshot);
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<C>>, StorageError<C::NodeId>> {
        match &*self.0.current_snapshot.read().await {
            Some(snapshot) => {
                let data = snapshot.data.clone();
                Ok(Some(Snapshot {
                    meta: snapshot.meta.clone(),
                    snapshot: Box::new(Cursor::new(data)),
                }))
            }
            None => Ok(None),
        }
    }
}
