//! Definition of pool types that can be saved to the persistent store.

use crate::types::v0::{
    openapi::{models, models::PoolSpecEncryption},
    store::{
        definitions::{ObjectKey, StorableObject, StorableObjectType},
        AsOperationSequencer, OperationSequence, SpecStatus, SpecTransaction,
    },
    transport::{self, CreatePool, ImportPool, NodeId, PoolDeviceUri, PoolId},
};

pub const POOL_BS_CLUSTER_SIZE_DEFAULT: u32 = 4194304;

pub fn default_pool_cluster_size() -> u32 {
    POOL_BS_CLUSTER_SIZE_DEFAULT
}

/// Pool validation error types providing specific, actionable error information.
#[derive(Debug, Clone, PartialEq)]
pub enum PoolSpecValidationError {
    /// Pool has invalid disk count for its configuration.
    InvalidDiskCount {
        disk_count: usize,
        minimum_required: usize,
        reason: String,
    },
    /// RAID0 strip size is invalid.
    Raid0InvalidStripSize {
        strip_size: u64,
        minimum_size: u64,
        reason: String,
    },
}

impl std::fmt::Display for PoolSpecValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PoolSpecValidationError::InvalidDiskCount {
                disk_count,
                minimum_required,
                reason,
            } => {
                write!(
                    f,
                    "Invalid disk count: {disk_count} disks provided, {minimum_required} required. {reason}"
                )
            }
            PoolSpecValidationError::Raid0InvalidStripSize {
                strip_size,
                minimum_size,
                reason,
            } => {
                write!(
                    f,
                    "Invalid RAID0 strip size: {strip_size} bytes (minimum {minimum_size}). {reason}"
                )
            }
        }
    }
}

impl std::error::Error for PoolSpecValidationError {}

// PoolLabel is the type for the labels
pub type PoolLabel = HashMap<String, String>;

use crate::IntoOption;
use pstor::ApiVersion;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::From, fmt::Debug};

/// Pool data structure used by the persistent store.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Pool {
    /// Current state of the pool.
    pub state: Option<PoolState>,
    /// Desired pool specification.
    pub spec: Option<PoolSpec>,
}

/// Runtime state of the pool.
/// This should eventually satisfy the PoolSpec.
#[derive(Serialize, Deserialize, Debug, PartialEq, Default, Clone)]
pub struct PoolState {
    /// Pool information returned by the Io-Engine.
    pub pool: transport::PoolState,
}

impl From<transport::PoolState> for PoolState {
    fn from(pool: transport::PoolState) -> Self {
        Self { pool }
    }
}

/// Status of the Pool Spec
pub type PoolSpecStatus = SpecStatus<transport::PoolStatus>;

impl From<&CreatePool> for PoolSpec {
    fn from(request: &CreatePool) -> Self {
        Self {
            node: request.node.clone(),
            id: request.id.clone(),
            disks: request.disks.clone(),
            status: PoolSpecStatus::Creating,
            raid_config: request.raid_config.clone(),
            labels: request.labels.clone(),
            sequencer: OperationSequence::new(),
            operation: None,
            creat_tsc: None,
            encryption: request.encryption.clone(),
            cordon_drain: None,
            // Default is 4MiB today.
            cluster_size: request.cluster_size.unwrap_or(POOL_BS_CLUSTER_SIZE_DEFAULT),
            max_expansion: request.max_expansion.clone(),
        }
    }
}
impl From<&PoolSpec> for CreatePool {
    fn from(pool: &PoolSpec) -> Self {
        Self {
            node: pool.node.clone(),
            id: pool.id.clone(),
            disks: pool.disks.clone(),
            raid_config: pool.raid_config.clone(),
            labels: pool.labels.clone(),
            encryption: pool.encryption.clone(),
            cluster_size: Some(pool.cluster_size),
            max_expansion: pool.max_expansion.clone(),
        }
    }
}

impl PartialEq<CreatePool> for PoolSpec {
    fn eq(&self, other: &CreatePool) -> bool {
        let mut other = PoolSpec::from(other);
        other.status = self.status.clone();
        other.sequencer = self.sequencer.clone();
        other.creat_tsc = self.creat_tsc;
        &other == self
    }
}

/// Encryption parameters.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum Encryption {
    /// Name of the secret or file to parse the encryption parameters.
    Secret(EncryptionSecret),
}

/// Encryption secret.
#[derive(Serialize, Deserialize, Default, Debug, Clone, Eq, PartialEq)]
pub struct EncryptionSecret {
    /// Name of the secret.
    pub name: String,
}

/// RAID configuration specifying the type and parameters.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum RaidConfig {
    /// RAID0 configuration with strip size in KB.
    Raid0 { strip_size_kb: u32 },
}

impl RaidConfig {
    /// Validate pool configuration parameters.
    pub fn validate(&self, disk_count: usize) -> Result<(), PoolSpecValidationError> {
        match self {
            RaidConfig::Raid0 { strip_size_kb } => {
                if disk_count < 2 {
                    return Err(PoolSpecValidationError::InvalidDiskCount {
                        disk_count,
                        minimum_required: 2,
                        reason: "RAID0 configuration requires at least 2 disks".to_string(),
                    });
                }
                if *strip_size_kb == 0 || !strip_size_kb.is_power_of_two() || *strip_size_kb < 4 {
                    let reason = if *strip_size_kb == 0 {
                        "Strip size must be greater than 0".to_string()
                    } else {
                        "Strip size must be a power of 2 and at least 4KB".to_string()
                    };
                    return Err(PoolSpecValidationError::Raid0InvalidStripSize {
                        strip_size: (*strip_size_kb as u64) * 1024, // Convert to bytes for error message
                        minimum_size: 4096,
                        reason,
                    });
                }
                Ok(())
            }
        }
    }

    /// Check if this configuration requires multiple disks.
    pub fn requires_multiple_disks(&self) -> bool {
        match self {
            RaidConfig::Raid0 { .. } => true,
        }
    }
}

/// User specification of a pool.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PoolSpec {
    /// id of the io-engine instance
    pub node: NodeId,
    /// id of the pool
    pub id: PoolId,
    /// absolute disk paths claimed by the pool
    pub disks: Vec<PoolDeviceUri>,
    /// status of the pool
    pub status: PoolSpecStatus,
    /// pool configuration specifying the type and parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raid_config: Option<RaidConfig>,
    /// labels to be set on the pool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<PoolLabel>,
    /// Update in progress
    #[serde(skip)]
    pub sequencer: OperationSequence,
    /// Record of the operation in progress
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<PoolOperationState>,
    /// Last modification timestamp.
    #[serde(skip)]
    pub creat_tsc: Option<std::time::SystemTime>,
    /// Use to create/import encrypted pool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption: Option<Encryption>,
    /// Cordon/drain state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cordon_drain: Option<CordonDrainState>,
    /// Blobstore cluster size used for this pool.
    #[serde(default = "default_pool_cluster_size")]
    pub cluster_size: u32,
    /// Maximum expansion size for this pool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_expansion: Option<String>,
}

impl PoolSpec {
    /// Pool identification.
    pub fn id(&self) -> &PoolId {
        &self.id
    }

    /// Label pool by applying the labels.
    pub fn label(&mut self, labels: HashMap<String, String>) {
        match &mut self.labels {
            Some(existing_labels) => {
                existing_labels.extend(labels);
            }
            None => {
                self.labels = Some(labels);
            }
        }
    }

    /// Check if the pool has the given topology label key.
    pub fn has_labels_key(&self, key: &str) -> bool {
        if let Some(labels) = &self.labels {
            return labels.contains_key(key);
        }
        false
    }

    /// Remove label from pool.
    pub fn unlabel(&mut self, label_key: &str) {
        if let Some(labels) = &mut self.labels {
            labels.remove(label_key);
        }
    }

    /// Check if there are key collisions between current topology labels and the given labels.
    pub fn label_collisions<'a>(
        &'a self,
        labels: &'a HashMap<String, String>,
    ) -> (HashMap<&'a String, &'a String>, bool) {
        let mut conflict = false;
        let mut existing_conflicts = HashMap::new();

        if let Some(existing_labels) = &self.labels {
            for (key, value) in labels {
                if let Some(existing_value) = existing_labels.get(key) {
                    if existing_value != value {
                        conflict = true;
                        existing_conflicts.insert(key, existing_value);
                    }
                }
            }
        }

        (existing_conflicts, conflict)
    }

    /// Ensure the state is consistent.
    pub fn resolve(&mut self) {
        if let Some(ds) = &mut self.cordon_drain {
            match ds {
                CordonDrainState::Cordoned(state) => {
                    if !state.cordoned() {
                        self.cordon_drain = None;
                    }
                }
            }
        }
    }

    /// Validate pool specification.
    pub fn validate(&self) -> Result<(), PoolSpecValidationError> {
        // Multi-device pools require explicit pool configuration
        if self.disks.len() > 1 && self.raid_config.is_none() {
            return Err(PoolSpecValidationError::InvalidDiskCount {
                disk_count: self.disks.len(),
                minimum_required: 1,
                reason: "Multi-device pools require explicit pool configuration".to_string(),
            });
        }

        // Validate pool configuration if present
        if let Some(config) = &self.raid_config {
            config.validate(self.disks.len())?;
        }

        Ok(())
    }

    /// Cordon the pool.
    pub fn cordon(&mut self, op: PoolCordonOp) {
        match &mut self.cordon_drain {
            Some(ds) => {
                ds.add_cordon(op);
            }
            None => {
                self.cordon_drain = Some(CordonDrainState::Cordoned(CordonedState::from(op)));
            }
        }
        self.resolve();
    }

    /// Uncordon the pool.
    pub fn uncordon(&mut self, op: PoolCordonOp) {
        if let Some(ds) = &mut self.cordon_drain {
            ds.rm_cordon(op);
        }
        self.resolve();
    }

    /// Returns whether the pool is cordoned and its state.
    pub fn cordoned(&self) -> Option<&CordonedState> {
        self.cordon_drain.as_ref().map(|s| match s {
            CordonDrainState::Cordoned(cordoned) => cordoned,
        })
    }

    /// Check if the pool is cordoned for imports.
    pub fn cordoned_imports(&self) -> bool {
        self.cordoned().map(|s| s.import).unwrap_or_default()
    }

    /// Returns true if all labels are already present.
    pub fn cordon_would_modify(&self, op: &PoolCordonOp) -> bool {
        match &self.cordon_drain {
            Some(ds) => ds.would_modify(op, true),
            None => true,
        }
    }

    /// Returns true if all labels are already present.
    pub fn uncordon_would_modify(&self, op: &PoolCordonOp) -> bool {
        match &self.cordon_drain {
            Some(ds) => ds.would_modify(op, false),
            None => false,
        }
    }
}

impl From<&PoolSpec> for ImportPool {
    fn from(value: &PoolSpec) -> Self {
        Self {
            node: value.node.clone(),
            id: value.id.clone(),
            disks: value.disks.clone(),
            uuid: None,
            encryption: value.encryption.clone(),
            raid_config: value.raid_config.clone(),
        }
    }
}

impl AsOperationSequencer for PoolSpec {
    fn as_ref(&self) -> &OperationSequence {
        &self.sequencer
    }

    fn as_mut(&mut self) -> &mut OperationSequence {
        &mut self.sequencer
    }
}

impl From<PoolSpec> for models::PoolSpec {
    fn from(src: PoolSpec) -> Self {
        let encryption = match src.encryption {
            None => None,
            Some(encr) => match encr {
                Encryption::Secret(details) => Some(PoolSpecEncryption::secret(
                    openapi::models::EncryptionSecret { name: details.name },
                )),
            },
        };
        Self::new_all(
            src.disks,
            src.id,
            src.labels,
            src.node,
            src.status,
            encryption,
            src.cordon_drain.into_opt(),
            Some(src.cluster_size as i64),
            src.max_expansion,
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PoolOperationState {
    /// Record of the operation
    pub operation: PoolOperation,
    /// Result of the operation
    pub result: Option<bool>,
}

impl SpecTransaction<PoolOperation> for PoolSpec {
    fn has_pending_op(&self) -> bool {
        self.operation.is_some()
    }

    fn commit_op(&mut self) {
        if let Some(op) = self.operation.clone() {
            match op.operation {
                PoolOperation::Destroy => {
                    self.status = SpecStatus::Deleted;
                }
                PoolOperation::Create => {
                    self.status = SpecStatus::Created(transport::PoolStatus::Online);
                }
                PoolOperation::Label(PoolLabelOp { labels, .. }) => {
                    self.label(labels);
                }
                PoolOperation::Unlabel(PoolUnLabelOp { label_key }) => {
                    self.unlabel(&label_key);
                }
                PoolOperation::Cordon(op) => {
                    self.cordon(op);
                }
                PoolOperation::Uncordon(op) => {
                    self.uncordon(op);
                }
            }
        }
        self.clear_op();
    }

    fn clear_op(&mut self) {
        self.operation = None;
    }

    fn start_op(&mut self, operation: PoolOperation) {
        if matches!(operation, PoolOperation::Create) && self.creat_tsc.is_none() {
            self.creat_tsc = Some(std::time::SystemTime::now());
        }
        self.operation = Some(PoolOperationState {
            operation,
            result: None,
        })
    }

    fn set_op_result(&mut self, result: bool) {
        if let Some(op) = &mut self.operation {
            op.result = Some(result);
        }
    }

    fn pending_op(&self) -> Option<&PoolOperation> {
        self.operation.as_ref().map(|o| &o.operation)
    }

    fn log_op(&self, operation: &PoolOperation) -> (bool, bool) {
        match operation {
            PoolOperation::Create => (true, true),
            PoolOperation::Destroy => (true, true),
            PoolOperation::Label(_) => (false, true),
            PoolOperation::Unlabel(_) => (false, true),
            PoolOperation::Cordon(_) => (false, true),
            PoolOperation::Uncordon(_) => (false, true),
        }
    }
}

/// Available Pool Operations
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum PoolOperation {
    Create,
    Destroy,
    Label(PoolLabelOp),
    Unlabel(PoolUnLabelOp),
    Cordon(PoolCordonOp),
    Uncordon(PoolCordonOp),
}

/// Parameter for adding/removing pool cordons.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PoolCordonOp {
    /// No new replicas can be created on this pool
    pub replicas: bool,
    /// No new snapshots can be created on this pool
    pub snapshots: bool,
    /// No new restores can be created on this pool
    pub restores: bool,
    /// Pool cannot be imported after node/engine restart.
    pub import: bool,
}
impl PoolCordonOp {
    fn resource(yes: bool, name: &str) -> &str {
        if yes {
            name
        } else {
            ""
        }
    }
    /// Convert cordon resources to a comma separated string.
    pub fn resources(&self) -> String {
        [
            Self::resource(self.replicas, "replicas"),
            Self::resource(self.snapshots, "snapshots"),
            Self::resource(self.restores, "restores"),
            Self::resource(self.import, "import"),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join(",")
    }
}

/// Parameter for adding pool labels.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PoolLabelOp {
    pub labels: HashMap<String, String>,
    pub overwrite: bool,
}
impl From<(HashMap<String, String>, bool)> for PoolLabelOp {
    fn from((labels, overwrite): (HashMap<String, String>, bool)) -> Self {
        Self { labels, overwrite }
    }
}
/// Parameter for removing pool labels.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PoolUnLabelOp {
    pub label_key: String,
}
impl From<String> for PoolUnLabelOp {
    fn from(label_key: String) -> Self {
        Self { label_key }
    }
}

impl PartialEq<transport::PoolState> for PoolSpec {
    fn eq(&self, other: &transport::PoolState) -> bool {
        self.node == other.node
    }
}

/// Key used by the store to uniquely identify a PoolSpec structure.
pub struct PoolSpecKey(PoolId);

impl From<&PoolId> for PoolSpecKey {
    fn from(id: &PoolId) -> Self {
        Self(id.clone())
    }
}

impl ObjectKey for PoolSpecKey {
    type Kind = StorableObjectType;

    fn version(&self) -> ApiVersion {
        ApiVersion::V0
    }

    fn key_type(&self) -> StorableObjectType {
        StorableObjectType::PoolSpec
    }

    fn key_uuid(&self) -> String {
        self.0.to_string()
    }
}

impl StorableObject for PoolSpec {
    type Key = PoolSpecKey;

    fn key(&self) -> Self::Key {
        PoolSpecKey(self.id.clone())
    }
}

impl From<&PoolSpec> for transport::PoolState {
    fn from(pool: &PoolSpec) -> Self {
        Self {
            node: pool.node.clone(),
            id: pool.id.clone(),
            disks: pool.disks.clone(),
            status: transport::PoolStatus::Unknown,
            capacity: 0,
            used: 0,
            committed: None,
            encrypted: pool.encryption.is_some(),
            cluster_size: pool.cluster_size,
            disk_capacity: None,
            max_expandable_size: None,
            raid_info: pool
                .raid_config
                .as_ref()
                .map(|raid_config| match raid_config {
                    RaidConfig::Raid0 { .. } => transport::RaidInfo {
                        level: "raid0".to_string(),
                        state: "unknown".to_string(),
                    },
                }),
        }
    }
}

impl From<EncryptionSecret> for models::EncryptionSecret {
    fn from(value: EncryptionSecret) -> Self {
        Self { name: value.name }
    }
}

impl From<models::Encryption> for Encryption {
    fn from(value: models::Encryption) -> Self {
        match value {
            models::Encryption::secret(secret_name) => Self::Secret(secret_name.into()),
        }
    }
}

/// Data relating to the cordoning of a pool.
#[derive(Clone, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CordonedState {
    // todo: or should these be negated, ie: by default all blocked?
    /// No new replicas can be created on this pool.
    pub replicas: bool,
    /// No new snapshots can be created on this pool.
    pub snapshots: bool,
    /// No new restores can be created on this pool.
    pub restores: bool,
    /// Pool cannot be imported after node/engine restart.
    pub import: bool,
}
impl CordonedState {
    fn cordoned(&self) -> bool {
        self.replicas || self.snapshots || self.restores || self.import
    }
}

impl From<PoolCordonOp> for CordonedState {
    fn from(value: PoolCordonOp) -> Self {
        Self {
            replicas: value.replicas,
            snapshots: value.snapshots,
            restores: value.restores,
            import: value.import,
        }
    }
}

impl CordonedState {
    fn set_if(ifset: bool, set: &mut bool, val: bool) {
        if ifset {
            *set = val;
        }
    }
    /// Add cordon resources.
    pub fn add_cordon(&mut self, op: PoolCordonOp) {
        self.op_cordon(op, true);
    }
    /// Remove cordon resources.
    pub fn rm_cordon(&mut self, op: PoolCordonOp) {
        self.op_cordon(op, false);
    }
    fn op_cordon(&mut self, op: PoolCordonOp, cordon: bool) {
        Self::set_if(op.replicas, &mut self.replicas, cordon);
        Self::set_if(op.snapshots, &mut self.snapshots, cordon);
        Self::set_if(op.restores, &mut self.restores, cordon);
        Self::set_if(op.import, &mut self.import, cordon);
    }
    fn if_modify(current: bool, op: bool, cordon: bool) -> bool {
        if cordon {
            !current && op
        } else {
            current && op
        }
    }
    /// Returns whether the operation would yield changes.
    pub fn would_modify(&self, op: &PoolCordonOp, cordon: bool) -> bool {
        Self::if_modify(self.replicas, op.replicas, cordon)
            || Self::if_modify(self.snapshots, op.snapshots, cordon)
            || Self::if_modify(self.restores, op.restores, cordon)
            || Self::if_modify(self.import, op.import, cordon)
    }
}

/// Enum variant encompassing data related to a cordoned or draining pool.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum CordonDrainState {
    /// The pool is being cordoned.
    Cordoned(CordonedState),
}

impl CordonDrainState {
    /// Update cordon with the given options.
    pub fn add_cordon(&mut self, cordon: PoolCordonOp) {
        match self {
            CordonDrainState::Cordoned(state) => {
                state.add_cordon(cordon);
            }
        }
    }
    /// Update cordon with the given options.
    pub fn rm_cordon(&mut self, cordon: PoolCordonOp) {
        match self {
            CordonDrainState::Cordoned(state) => {
                state.rm_cordon(cordon);
            }
        }
    }
    /// Returns whether the state has all the specified cordon labels.
    pub fn would_modify(&self, op: &PoolCordonOp, cordon: bool) -> bool {
        match self {
            CordonDrainState::Cordoned(state) => state.would_modify(op, cordon),
        }
    }
}

impl From<CordonDrainState> for models::PoolCordonDrain {
    fn from(node_ds: CordonDrainState) -> Self {
        match node_ds {
            CordonDrainState::Cordoned(state) => {
                let cs = models::PoolCordon {
                    replicas: state.replicas,
                    snapshots: state.snapshots,
                    restores: state.restores,
                    import: state.import,
                };
                Self::cordoned(cs)
            }
        }
    }
}
