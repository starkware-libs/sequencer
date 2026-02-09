use std::sync::Arc;
use std::time::Duration;

use aerospike::{
    as_bin,
    operations,
    BatchOperation,
    BatchPolicy,
    BatchReadPolicy,
    BatchWritePolicy,
    Bin,
    Bins,
    Client,
    ClientPolicy,
    CommitLevel,
    Error as AerospikeError,
    Host,
    Key,
    ReadPolicy,
    Record,
    Result as AerospikeResult,
    ResultCode,
    Value,
    WritePolicy,
};

use crate::storage_trait::{
    AsyncStorage,
    DbHashMap,
    DbKey,
    DbOperationMap,
    DbValue,
    EmptyStorageConfig,
    NoStats,
    PatriciaStorageResult,
    Storage,
};

pub type Port = u16;

#[derive(thiserror::Error, Debug)]
pub enum AerospikeStorageError {
    #[error(transparent)]
    Aerospike(#[from] AerospikeError),
    #[error("batch read error: {0}")]
    BatchReadError(String),
    #[error("expected Blob, got {:?}", .0)]
    ExpectedBlob(Value),
}

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_FAIL_IF_NOT_CONNECTED: bool = true;
pub const DEFAULT_PORT: Port = 3000;

#[derive(Clone)]
pub struct AerospikeStorageConfig {
    pub aeroset: String,
    pub namespace: String,
    pub hosts: Vec<Host>,

    pub client_policy: ClientPolicy,
    pub read_policy: ReadPolicy,
    pub write_policy: WritePolicy,

    pub batch_policy: BatchPolicy,
    pub batch_read_policy: BatchReadPolicy,
    pub batch_write_policy: BatchWritePolicy,

    pub bin_name: String,
}

impl AerospikeStorageConfig {
    pub fn new_default(aeroset: String, namespace: String, hosts: Vec<(String, Port)>) -> Self {
        Self {
            aeroset,
            namespace,
            hosts: hosts.into_iter().map(|(host, port)| Host::new(&host, port)).collect(),
            client_policy: ClientPolicy {
                fail_if_not_connected: DEFAULT_FAIL_IF_NOT_CONNECTED,
                timeout: Some(DEFAULT_TIMEOUT),
                ..Default::default()
            },
            read_policy: ReadPolicy::default(),
            write_policy: WritePolicy {
                commit_level: CommitLevel::CommitAll,
                ..WritePolicy::default()
            },
            batch_policy: BatchPolicy::default(),
            batch_read_policy: BatchReadPolicy::default(),
            batch_write_policy: BatchWritePolicy {
                commit_level: CommitLevel::CommitAll,
                ..BatchWritePolicy::default()
            },
            bin_name: "default_bin".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct AerospikeStorage {
    config: AerospikeStorageConfig,
    client: Arc<Client>,
}

impl AerospikeStorage {
    pub async fn new(config: AerospikeStorageConfig) -> AerospikeResult<Self> {
        let client = Arc::new(Client::new(&config.client_policy, &config.hosts).await?);
        Ok(Self { config, client })
    }

    fn get_key(&self, key: DbKey) -> AerospikeResult<Key> {
        Key::new(self.config.namespace.clone(), self.config.aeroset.clone(), Value::Blob(key.0))
    }

    fn extract_value(&self, record: &Record) -> PatriciaStorageResult<Option<DbValue>> {
        match record.bins.get(&self.config.bin_name) {
            Some(Value::Blob(bytes)) => Ok(Some(DbValue(bytes.clone()))),
            Some(value) => Err(AerospikeStorageError::ExpectedBlob(value.clone()).into()),
            None => Ok(None),
        }
    }
}

impl Storage for AerospikeStorage {
    type Stats = NoStats;
    type Config = EmptyStorageConfig;

    async fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let record = self
            .client
            .get(&self.config.read_policy, &self.get_key(key.clone())?, Bins::All)
            .await?;
        self.extract_value(&record)
    }

    async fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        Ok(self
            .client
            .put(
                &self.config.write_policy,
                &self.get_key(key)?,
                &[as_bin!(&self.config.bin_name, value.0)],
            )
            .await?)
    }

    async fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut ops = Vec::new();
        for key in keys.iter() {
            ops.push(BatchOperation::read(
                &self.config.batch_read_policy,
                self.get_key((*key).clone())?,
                Bins::All,
            ));
        }
        let batch_records = self.client.batch(&self.config.batch_policy, &ops).await?;
        batch_records
            .iter()
            .map(|batch_record| match batch_record.record {
                Some(ref record) => self.extract_value(record),
                None => match batch_record.result_code {
                    None | Some(ResultCode::Ok) => Ok(None),
                    Some(result_code) => {
                        Err(AerospikeStorageError::BatchReadError(result_code.into_string()).into())
                    }
                },
            })
            .collect::<Result<_, _>>()
    }

    async fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        let keys_and_bins: Vec<(DbKey, Bin)> = key_to_value
            .into_iter()
            .map(|(key, value)| (key, as_bin!(&self.config.bin_name, value.0)))
            .collect();
        let mut ops = Vec::new();
        for (key, bin) in keys_and_bins.iter() {
            ops.push(BatchOperation::write(
                &self.config.batch_write_policy,
                self.get_key(key.clone())?,
                vec![operations::put(bin)],
            ));
        }
        self.client.batch(&self.config.batch_policy, &ops).await?;
        Ok(())
    }

    async fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        self.client.delete(&self.config.write_policy, &self.get_key(key.clone())?).await?;
        Ok(())
    }

    async fn multi_set_and_delete(
        &mut self,
        _key_to_operation: DbOperationMap,
    ) -> PatriciaStorageResult<()> {
        unimplemented!()
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }

    fn get_async_self(&self) -> Option<impl AsyncStorage> {
        Some(self.clone())
    }
}
