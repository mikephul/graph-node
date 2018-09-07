extern crate jsonrpc_http_server;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate graph;

use graph::prelude::{JsonRpcServer as JsonRpcServerTrait, *};
use graph::serde_json;
use jsonrpc_http_server::{
    jsonrpc_core::{self, Id, IoHandler, MethodCall, Params, Value, Version},
    RestApi, Server, ServerBuilder,
};

use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Debug, Serialize, Deserialize)]
struct SubgraphDeployParams {
    name: String,
    ipfs_hash: String,
}

impl fmt::Display for SubgraphDeployParams {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SubgraphRemoveParams {
    name_or_id: String,
}

impl fmt::Display for SubgraphRemoveParams {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{:?}", self)
    }
}

pub struct JsonRpcServer<T> {
    provider: Arc<T>,
    logger: Logger,
}

impl<T: SubgraphProvider> JsonRpcServer<T> {
    /// Handler for the `subgraph_deploy` endpoint.
    fn deploy_handler(
        &self,
        params: SubgraphDeployParams,
    ) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
        info!(self.logger, "Received subgraph_deploy request"; "params" => params.to_string());
        self.provider
            .deploy(params.name, format!("/ipfs/{}", params.ipfs_hash))
            .map_err(|e| json_rpc_error(0, e.to_string()))
            .map(|_| Ok(Value::Null))
            .flatten()
    }

    /// Handler for the `subgraph_remove` endpoint.
    fn remove_handler(
        &self,
        params: SubgraphRemoveParams,
    ) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
        info!(self.logger, "Received subgraph_remove request"; "params" => params.to_string());
        self.provider
            .remove(params.name_or_id)
            .map_err(|e| json_rpc_error(1, e.to_string()))
            .map(|_| Ok(Value::Null))
            .flatten()
    }
}

impl<T: SubgraphProvider> JsonRpcServerTrait<T> for JsonRpcServer<T> {
    type Server = Server;

    fn serve(port: u16, provider: Arc<T>, logger: Logger) -> Result<Self::Server, io::Error> {
        let addr = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port);

        let mut handler = IoHandler::new();

        let arc_self = Arc::new(JsonRpcServer { provider, logger });
        // `subgraph_deploy` handler.
        let me = arc_self.clone();
        handler.add_method("subgraph_deploy", move |params: Params| {
            let me = me.clone();
            params
                .parse()
                .into_future()
                .and_then(move |params| me.deploy_handler(params))
        });

        // `subgraph_remove` handler.
        let me = arc_self.clone();
        handler.add_method("subgraph_remove", move |params: Params| {
            let me = me.clone();
            params
                .parse()
                .into_future()
                .and_then(move |params| me.remove_handler(params))
        });

        ServerBuilder::new(handler)
            // Enable REST API:
            // POST /<method>/<param1>/<param2>
            .rest_api(RestApi::Secure)
            .start_http(&addr.into())
    }
}

fn json_rpc_error(code: i64, message: String) -> jsonrpc_core::Error {
    jsonrpc_core::Error {
        code: jsonrpc_core::ErrorCode::ServerError(code),
        message,
        data: None,
    }
}

pub fn subgraph_deploy_request(name: String, ipfs_hash: String, id: String) -> MethodCall {
    let params = serde_json::to_value(SubgraphDeployParams { name, ipfs_hash })
        .unwrap()
        .as_object()
        .cloned()
        .unwrap();

    MethodCall {
        jsonrpc: Some(Version::V2),
        method: "subgraph_deploy".to_owned(),
        params: Params::Map(params),
        id: Id::Str(id),
    }
}

pub fn parse_response(response: Value) -> Result<(), jsonrpc_core::Error> {
    // serde deserialization of the `id` field to an `Id` struct is somehow
    // incompatible with the `arbitrary-precision` feature which we use, so we
    // need custom parsing logic.
    let object = response.as_object().unwrap();
    if let Some(error) = object.get("error") {
        Err(serde_json::from_value(error.clone()).unwrap())
    } else {
        Ok(())
    }
}
