use crate::controller::io_engine::GrpcContext;
use agents::errors::{GrpcConnect, SvcError};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use tonic::transport::Channel;

pub(crate) trait JsonRpcMethod {
    type Params: serde::Serialize;
    type Response: for<'de> serde::Deserialize<'de>;

    fn method(&self) -> &str;
    fn params(&self) -> &Self::Params;
}

/// Request parameters for the SPDK `bdev_set_qos_limit` JsonRpc method.
#[derive(Serialize, Debug)]
pub(crate) struct BdevSetQosLimitRequest {
    /// Name of the block device to set QoS limits for
    pub name: String,
    /// Read/Write IOPS limit (minimum 1000)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rw_ios_per_sec: Option<u32>,
    /// Read/Write bandwidth limit in MB/s (minimum 10)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rw_mbytes_per_sec: Option<u32>,
    /// Read-only bandwidth limit in MB/s
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_mbytes_per_sec: Option<u32>,
    /// Write-only bandwidth limit in MB/s
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w_mbytes_per_sec: Option<u32>,
}

impl JsonRpcMethod for BdevSetQosLimitRequest {
    type Params = Self;
    type Response = ();

    fn method(&self) -> &str {
        "bdev_set_qos_limit"
    }
    fn params(&self) -> &Self::Params {
        self
    }
}

/// JsonGrpc client for calling SPDK JsonRpc methods via io-engine's gRPC JsonRpc service.
/// This client bridges gRPC calls to SPDK's Unix Domain Socket JsonRpc interface.
#[derive(Clone, Debug)]
pub(crate) struct JsonGrpcClient {
    client: rpc::v1::json::JsonRpcClient<Channel>,
}

impl JsonGrpcClient {
    /// Create a new JsonGrpc client from a GrpcContext.
    pub async fn new(context: &GrpcContext) -> Result<Self, SvcError> {
        let channel = context
            .tonic_endpoint()
            .connect()
            .await
            .context(GrpcConnect {
                node_id: context.node().to_owned(),
                endpoint: context.endpoint().to_string(),
            })?;

        Ok(Self {
            client: rpc::v1::json::JsonRpcClient::new(channel),
        })
    }

    /// Call a JsonRpc method with generic serializable parameters and deserializable response.
    ///
    /// # Arguments
    /// * `method` - The SPDK JsonRpc method name (e.g., "bdev_set_qos_limit")
    /// * `params` - Parameters that implement Serialize (can be serde_json::Value, custom struct, etc.)
    ///
    /// # Returns
    /// * Deserialized response of type R
    ///
    /// # Examples
    ///
    /// Generic typed call - requires explicit type annotation for response:
    /// ```
    /// let params = json!({"name": "nexus0", "rw_ios_per_sec": 1000});
    /// let result: serde_json::Value = client.call("bdev_set_qos_limit", params).await?;
    /// ```
    ///
    pub async fn call_method<P, R>(&self, method: &str, params: P) -> Result<R, SvcError>
    where
        P: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        // Serialize parameters to JSON string
        let params_str = serde_json::to_string(&params).map_err(|e| SvcError::JsonRpc {
            method: method.to_string(),
            params: "serialization failed".to_string(),
            error: e.to_string(),
        })?;

        // Create gRPC JsonRpc request
        let request = rpc::v1::json::JsonRpcRequest {
            method: method.to_string(),
            params: params_str.clone(),
        };

        // Make the gRPC call to io-engine's JsonRpc service
        let response = self
            .client
            .clone()
            .json_rpc_call(request)
            .await
            .map_err(|error| SvcError::JsonRpc {
                method: method.to_string(),
                params: params_str,
                error: error.to_string(),
            })?;

        // Deserialize the response
        let result =
            serde_json::from_str(&response.into_inner().result).map_err(|e| SvcError::JsonRpc {
                method: method.to_string(),
                params: "response deserialization failed".to_string(),
                error: e.to_string(),
            })?;

        Ok(result)
    }

    /// Call a JsonRpc method using a request object that implements JsonRpcMethod.
    /// This provides the most type-safe interface with compile-time method validation.
    ///
    /// # Arguments
    /// * `request` - A request object implementing JsonRpcMethod trait
    ///
    /// # Returns
    /// * Deserialized response of the type specified by the request
    ///
    /// # Example
    /// ```
    /// let qos_request = BdevSetQosLimitRequest {
    ///     name: "nexus0".to_string(),
    ///     rw_ios_per_sec: Some(1000),
    ///     // ... other fields
    /// };
    /// let result = client.call_method(&qos_request).await?;
    /// ```
    pub async fn call<T>(&self, request: &T) -> Result<T::Response, SvcError>
    where
        T: JsonRpcMethod,
    {
        self.call_method(request.method(), request.params()).await
    }
}
