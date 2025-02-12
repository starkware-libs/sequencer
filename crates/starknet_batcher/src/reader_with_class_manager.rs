use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_types_core::felt::Felt;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::task;

// TODO(Elin): remove once class manager is properly integrated into Papyrus reader.
pub struct ReaderWithClassManager<S: StateReader + Send + Sync> {
    state_reader: S,
    class_manager_client: SharedClassManagerClient,
}

impl<S: StateReader + Send + Sync> ReaderWithClassManager<S> {
    pub fn new(state_reader: S, class_manager_client: SharedClassManagerClient) -> Self {
        Self { state_reader, class_manager_client }
    }
}

impl<S: StateReader + Send + Sync> StateReader for ReaderWithClassManager<S> {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        self.state_reader.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.state_reader.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.state_reader.get_class_hash_at(contract_address)
    }

    // fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
    //     let client = self.class_manager_client.clone();
    //     let handle = Handle::current();
    //     let inner = handle.clone();

    //     // Spawn the async task inside a blocking thread.
    //     let join_handle = task::spawn_blocking(move || {
    //         inner.block_on(get_compiled_class_async(client, class_hash))
    //     });

    //     // Block on the join handle to get the result
    //     handle.block_on(join_handle).expect("Task panicked")
    // }

    // fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
    //     let client = self.class_manager_client.clone();

    //     // Spawn the async task inside a blocking thread.
    //     tokio::runtime::Handle::current().block_on(get_compiled_class_async(client, class_hash))
    // }

    // fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
    //     let client = self.class_manager_client.clone();

    //     tokio::task::block_in_place(|| {
    //         // Now it’s safe to block on the async function.
    //         tokio::runtime::Handle::current()
    //             .block_on(async { get_compiled_class_async(client, class_hash).await })
    //     })
    // }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        let client = self.class_manager_client.clone();
        let handle = std::thread::spawn(move || {
            // Create a multi-threaded runtime in this thread.
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async { get_compiled_class_async(client, class_hash).await })
        });
        // Wait for the async work to complete.
        handle.join().unwrap()
    }

    // fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
    //     let client = self.class_manager_client.clone();

    //     // Spawn the async task inside a blocking thread.
    //     let join_handle = task::spawn_blocking(move || {
    //         // Directly call the async function in the worker thread.
    //         tokio::runtime::Runtime::new()
    //             .expect("Failed to create runtime")
    //             .block_on(get_compiled_class_async(client, class_hash))
    //     });

    //     // Block on the join handle to get the result
    //     join_handle..expect("Task panicked")
    // }

    // fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
    //     let client = self.class_manager_client.clone();
    //     let (tx, rx) = oneshot::channel();

    //     println!("class_hash: {:?} inside get_compiled_class", class_hash);
    //     tokio::task::spawn_blocking(move || {
    //         let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    //         let result = rt.block_on(get_compiled_class_async(client, class_hash));
    //         let _ = tx.send(result);
    //     });
    //     println!("class_hash: {:?} inside get_compiled_class after spawning", class_hash);

    //     let result =
    //         Handle::current().block_on(rx).expect("Task panicked").expect("Channel closed");
    //     println!("class_hash: {:?} inside get_compiled_class got result", class_hash);
    //     Ok(result)
    // }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.state_reader.get_compiled_class_hash(class_hash)
    }
}

async fn get_compiled_class_async(
    class_manager_client: SharedClassManagerClient,
    class_hash: ClassHash,
) -> StateResult<RunnableCompiledClass> {
    println!("class_hash: {:?} inside get_compiled_class_async", class_hash);

    let contract_class = class_manager_client
        .get_executable(class_hash)
        .await
        .map_err(|err| StateError::StateReadError(err.to_string()))?
        .ok_or(StateError::UndeclaredClassHash(class_hash))?;

    println!("class_hash: {:?} inside get_compiled_class_async 2", class_hash);

    match contract_class {
        ContractClass::V0(ref inner) if inner == &Default::default() => {
            Err(StateError::UndeclaredClassHash(class_hash)) // Adjust fallback logic
        }
        ContractClass::V1(casm_contract_class) => {
            Ok(RunnableCompiledClass::V1(casm_contract_class.try_into()?))
        }
        ContractClass::V0(deprecated_contract_class) => {
            Ok(RunnableCompiledClass::V0(deprecated_contract_class.try_into()?))
        }
    }
}
