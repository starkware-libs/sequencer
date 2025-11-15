use std::sync::Arc;

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
    Error as AerospikeError,
    Key,
    ReadPolicy,
    Record,
    Result as AerospikeResult,
    ResultCode,
    Value,
    WritePolicy,
};

use crate::storage_trait::{DbHashMap, DbKey, DbValue, NoStats, PatriciaStorageResult, Storage};

#[derive(thiserror::Error, Debug)]
pub enum AerospikeStorageError {
    #[error(transparent)]
    Aerospike(#[from] AerospikeError),
    #[error("batch read error: {0}")]
    BatchReadError(String),
    #[error("expected Blob, got {:?}", .0)]
    ExpectedBlob(Value),
}

#[derive(Clone)]
pub struct AerospikeStorageConfig {
    pub aeroset: String,
    pub namespace: String,
    pub hosts: String,

    pub client_policy: ClientPolicy,
    pub read_policy: ReadPolicy,
    pub write_policy: WritePolicy,

    pub batch_policy: BatchPolicy,
    pub batch_read_policy: BatchReadPolicy,
    pub batch_write_policy: BatchWritePolicy,

    pub bin_name: String,
}

impl AerospikeStorageConfig {
    pub fn new_default(aeroset: String, namespace: String, hosts: String) -> Self {
        Self {
            aeroset,
            namespace,
            hosts,
            client_policy: ClientPolicy::default(),
            read_policy: ReadPolicy::default(),
            write_policy: WritePolicy::default(),
            batch_policy: BatchPolicy::default(),
            batch_read_policy: BatchReadPolicy::default(),
            batch_write_policy: BatchWritePolicy::default(),
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
    pub fn new(config: AerospikeStorageConfig) -> AerospikeResult<Self> {
        let client = Arc::new(Client::new(&config.client_policy, &config.hosts)?);
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

    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let record =
            self.client.get(&self.config.read_policy, &self.get_key(key.clone())?, Bins::All)?;
        self.extract_value(&record)
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        Ok(self.client.put(
            &self.config.write_policy,
            &self.get_key(key)?,
            &[as_bin!(&self.config.bin_name, value.0)],
        )?)
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut ops = Vec::new();
        for key in keys.iter() {
            ops.push(BatchOperation::read(
                &self.config.batch_read_policy,
                self.get_key((*key).clone())?,
                Bins::All,
            ));
        }
        let batch_records = self.client.batch(&self.config.batch_policy, &ops)?;
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

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
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
        self.client.batch(&self.config.batch_policy, &ops)?;
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        self.client.delete(&self.config.write_policy, &self.get_key(key.clone())?)?;
        Ok(())
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }
}
