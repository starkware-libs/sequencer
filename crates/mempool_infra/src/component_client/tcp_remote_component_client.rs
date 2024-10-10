use std::fmt::Debug;
use std::marker::PhantomData;
use std::net::IpAddr;
use std::sync::Arc;

use bincode::{deserialize, serialize};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::definitions::ClientResult;
use super::ClientError;
use crate::component_definitions::ServerResult;

pub struct TCPRemoteComponentClient<Request, Response>
where
    Request: Serialize + Debug,
    Response: DeserializeOwned + Debug,
{
    address: String,
    _req: PhantomData<Request>,
    _res: PhantomData<Response>,
}

impl<Request, Response> TCPRemoteComponentClient<Request, Response>
where
    Request: Serialize + Debug,
    Response: DeserializeOwned + Debug,
{
    pub fn new(ip_address: IpAddr, port: u16) -> Self {
        let address = format!("{}:{}", ip_address, port).to_string();
        Self { address, _req: PhantomData, _res: PhantomData }
    }

    pub async fn send(&self, request: Request) -> ClientResult<Response> {
        let request_bytes = serialize(&request).expect("Request serialization should succeed");
        let mut stream = TcpStream::connect(&self.address)
            .await
            .map_err(|e| ClientError::TCPCommunicationFailure(Arc::new(e)))?;
        stream
            .write_all(&request_bytes)
            .await
            .map_err(|e| ClientError::TCPCommunicationFailure(Arc::new(e)))?;

        let mut buffer = Vec::<u8>::with_capacity(size_of::<ServerResult<Response>>());
        let n_bytes = stream
            .read_buf(&mut buffer)
            .await
            .map_err(|e| ClientError::TCPCommunicationFailure(Arc::new(e)))?;
        let response: ServerResult<Response> = deserialize(&buffer[..n_bytes])
            .map_err(|e| ClientError::ResponseDeserializationFailure(Arc::new(e)))?;

        response.map_err(ClientError::TCPServerError)
    }
}

// Can't derive because derive forces the generics to also be `Clone`, which we prefer not to do
// since it'll require the generic Request and Response types to be cloneable.
impl<Request, Response> Clone for TCPRemoteComponentClient<Request, Response>
where
    Request: Serialize + Debug,
    Response: DeserializeOwned + Debug,
{
    fn clone(&self) -> Self {
        Self { address: self.address.clone(), _req: PhantomData, _res: PhantomData }
    }
}
