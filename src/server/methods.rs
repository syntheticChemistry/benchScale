// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON-RPC method implementations for benchScale.
//!
//! Method families:
//! - `health.*` — liveness, readiness, check (mandatory per IPC protocol)
//! - `lab.*`    — create, destroy, list, status
//! - `topology.*` — validate
//! - `node.*`   — health probe for individual lab nodes

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::backend::DockerBackend;
use crate::deploy;
use crate::lab::{LabMetadata, LabRegistry};
use crate::topology::Topology;
use crate::{Backend, Lab};

use super::MethodError;

/// Shared server state across all connections.
pub struct ServerState {
    registry: LabRegistry,
    backend: Arc<DockerBackend>,
    ready: Arc<RwLock<bool>>,
}

impl ServerState {
    /// Create a new server state, initializing Docker backend.
    pub async fn new() -> anyhow::Result<Self> {
        #[expect(
            deprecated,
            reason = "Server bootstrap uses legacy Config::from_env until BenchScaleConfig wiring"
        )]
        let config = crate::config_legacy::Config::from_env();
        let backend = DockerBackend::new().map_err(|e| anyhow::anyhow!("Docker init: {e}"))?;

        let available = backend.is_available().await.unwrap_or(false);

        if !available {
            warn!("Docker is not available — lab operations will fail");
        }

        Ok(Self {
            registry: LabRegistry::from_config(&config),
            backend: Arc::new(backend),
            ready: Arc::new(RwLock::new(available)),
        })
    }
}

type MethodResult = Result<Value, MethodError>;

/// Route a method name to the appropriate handler.
pub async fn dispatch(method: &str, params: Value, state: &ServerState) -> MethodResult {
    match method {
        // health.*
        "health.liveness" => health_liveness(),
        "health.readiness" => health_readiness(state).await,
        "health.check" => health_check(state).await,

        // lab.*
        "lab.create" => lab_create(params, state).await,
        "lab.destroy" => lab_destroy(params, state).await,
        "lab.list" => lab_list(state).await,
        "lab.status" => lab_status(params, state).await,

        // topology.*
        "topology.validate" => topology_validate(params).await,

        // node.*
        "node.health" => node_health(params, state).await,

        _ => Err(MethodError::NotFound),
    }
}

// ---------------------------------------------------------------------------
// health.*
// ---------------------------------------------------------------------------

#[allow(clippy::unnecessary_wraps)]
fn health_liveness() -> MethodResult {
    Ok(json!({ "status": "alive", "service": "benchscale", "version": crate::VERSION }))
}

async fn health_readiness(state: &ServerState) -> MethodResult {
    let ready = *state.ready.read().await;
    Ok(json!({ "status": if ready { "ready" } else { "not_ready" }, "docker": ready }))
}

async fn health_check(state: &ServerState) -> MethodResult {
    let docker_ok = state.backend.is_available().await.unwrap_or(false);
    {
        let mut r = state.ready.write().await;
        *r = docker_ok;
    }

    let lab_count = state
        .registry
        .list_labs()
        .await
        .map(|v| v.len())
        .unwrap_or(0);

    Ok(json!({
        "status": if docker_ok { "healthy" } else { "degraded" },
        "docker": docker_ok,
        "labs": lab_count,
        "version": crate::VERSION,
    }))
}

// ---------------------------------------------------------------------------
// lab.*
// ---------------------------------------------------------------------------

async fn lab_create(params: Value, state: &ServerState) -> MethodResult {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| MethodError::InvalidParams("missing \"name\" string".into()))?
        .to_string();

    let topology_value = params.get("topology").ok_or_else(|| {
        MethodError::InvalidParams(
            "missing \"topology\" (inline object or file path string)".into(),
        )
    })?;

    let topology: Topology = if let Some(path_str) = topology_value.as_str() {
        Topology::from_file(path_str)
            .await
            .map_err(|e| MethodError::Internal(format!("topology load: {e}")))?
    } else {
        serde_json::from_value(topology_value.clone())
            .map_err(|e| MethodError::InvalidParams(format!("topology parse: {e}")))?
    };

    info!("lab.create: creating lab '{name}'");

    let backend_clone = Arc::clone(&state.backend);
    let inner_backend: Arc<dyn Backend> = backend_clone;

    let lab = Lab::create_with_arc(name.clone(), topology.clone(), inner_backend)
        .await
        .map_err(|e| MethodError::Internal(format!("lab create: {e}")))?;

    let lab_id = lab.id().to_string();

    state
        .registry
        .register_lab(lab_id.clone(), name.clone(), topology, "docker".into())
        .await
        .map_err(|e| MethodError::Internal(format!("registry: {e}")))?;

    let nodes: Vec<Value> = lab
        .nodes()
        .await
        .iter()
        .map(|n| json!({ "name": n.name, "ip": n.ip_address, "status": format!("{:?}", n.status) }))
        .collect();

    let mut deployed_primals: Vec<Value> = Vec::new();

    if let Some(plasmid_path_str) = params.get("plasmid_bin_path").and_then(Value::as_str) {
        let plasmid_path = std::path::Path::new(plasmid_path_str);
        let arch_str = params
            .get("arch")
            .and_then(Value::as_str)
            .unwrap_or("x86_64");
        let arch = deploy::Arch::from_str_loose(arch_str).unwrap_or(deploy::Arch::X86_64);

        let available = deploy::list_available_primals(plasmid_path, arch);
        info!(
            "plasmidBin: {} binaries available for {arch}",
            available.len()
        );

        let primal_refs: Vec<&str> = available.iter().map(String::as_str).collect();

        for node_info in lab.nodes().await {
            match deploy::deploy_primals_to_node(
                state.backend.as_ref(),
                &node_info.id,
                plasmid_path,
                arch,
                &primal_refs,
            )
            .await
            {
                Ok(deployed) => {
                    for d in &deployed {
                        deployed_primals.push(json!({
                            "node": node_info.name,
                            "primal": d.name,
                            "path": d.remote_path,
                        }));
                    }
                }
                Err(e) => {
                    warn!("plasmidBin deploy to {}: {e}", node_info.name);
                }
            }
        }
    }

    Ok(json!({
        "id": lab_id,
        "name": name,
        "nodes": nodes,
        "deployed_primals": deployed_primals,
    }))
}

async fn lab_destroy(params: Value, state: &ServerState) -> MethodResult {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| MethodError::InvalidParams("missing \"name\" string".into()))?;

    info!("lab.destroy: destroying lab '{name}'");

    let metadata = state
        .registry
        .load_lab_by_name(name)
        .await
        .map_err(|e| MethodError::Internal(format!("load lab: {e}")))?;

    for node_id in &metadata.node_ids {
        if let Err(e) = state.backend.delete_node(node_id).await {
            warn!("failed to delete node {node_id}: {e}");
        }
    }

    if metadata.network_id.is_some()
        && let Err(e) = state
            .backend
            .delete_network(&metadata.topology.network.name)
            .await
    {
        warn!("failed to delete network: {e}");
    }

    state
        .registry
        .delete_lab(&metadata.id)
        .await
        .map_err(|e| MethodError::Internal(format!("delete: {e}")))?;

    Ok(json!({ "destroyed": true, "id": metadata.id }))
}

async fn lab_list(state: &ServerState) -> MethodResult {
    let labs: Vec<Value> = state
        .registry
        .list_labs()
        .await
        .map_err(|e| MethodError::Internal(format!("list: {e}")))?
        .iter()
        .map(lab_metadata_to_json)
        .collect();

    Ok(json!({ "labs": labs }))
}

async fn lab_status(params: Value, state: &ServerState) -> MethodResult {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| MethodError::InvalidParams("missing \"name\" string".into()))?;

    let metadata = state
        .registry
        .load_lab_by_name(name)
        .await
        .map_err(|e| MethodError::Internal(format!("load: {e}")))?;

    Ok(lab_metadata_to_json(&metadata))
}

fn lab_metadata_to_json(m: &LabMetadata) -> Value {
    json!({
        "id": m.id,
        "name": m.name,
        "status": format!("{:?}", m.status),
        "backend": m.backend_type,
        "nodes": m.node_ids.len(),
        "topology": m.topology.metadata.name,
        "created_at": m.created_at.to_rfc3339(),
        "updated_at": m.updated_at.to_rfc3339(),
    })
}

// ---------------------------------------------------------------------------
// topology.*
// ---------------------------------------------------------------------------

async fn topology_validate(params: Value) -> MethodResult {
    let topology_value = params.get("topology").ok_or_else(|| {
        MethodError::InvalidParams(
            "missing \"topology\" (inline object or file path string)".into(),
        )
    })?;

    let topology: Topology = if let Some(path_str) = topology_value.as_str() {
        Topology::from_file(path_str)
            .await
            .map_err(|e| MethodError::InvalidParams(format!("load: {e}")))?
    } else {
        serde_json::from_value(topology_value.clone())
            .map_err(|e| MethodError::InvalidParams(format!("parse: {e}")))?
    };

    match topology.validate() {
        Ok(()) => Ok(json!({
            "valid": true,
            "name": topology.metadata.name,
            "nodes": topology.nodes.len(),
        })),
        Err(e) => Ok(json!({
            "valid": false,
            "error": e.to_string(),
        })),
    }
}

// ---------------------------------------------------------------------------
// node.*
// ---------------------------------------------------------------------------

async fn node_health(params: Value, state: &ServerState) -> MethodResult {
    let lab_name = params
        .get("lab")
        .and_then(Value::as_str)
        .ok_or_else(|| MethodError::InvalidParams("missing \"lab\" string".into()))?;
    let node_name = params
        .get("node")
        .and_then(Value::as_str)
        .ok_or_else(|| MethodError::InvalidParams("missing \"node\" string".into()))?;

    let metadata = state
        .registry
        .load_lab_by_name(lab_name)
        .await
        .map_err(|e| MethodError::Internal(format!("load lab: {e}")))?;

    let found = metadata.node_ids.iter().any(|id| id.contains(node_name));

    if !found {
        return Ok(json!({
            "lab": lab_name,
            "node": node_name,
            "found": false,
            "status": "unknown",
        }));
    }

    let container_name = format!("benchscale-{}-{}", lab_name, node_name);
    let running = state.backend.is_available().await.unwrap_or(false);

    Ok(json!({
        "lab": lab_name,
        "node": node_name,
        "found": true,
        "container": container_name,
        "backend_available": running,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_liveness_returns_alive() {
        let result = health_liveness().expect("liveness");
        assert_eq!(result["status"], "alive");
        assert_eq!(result["service"], "benchscale");
    }

    #[test]
    fn test_lab_metadata_to_json() {
        let meta = LabMetadata {
            id: "test-id".into(),
            name: "test-lab".into(),
            status: crate::LabStatus::Running,
            topology: crate::Topology {
                metadata: crate::topology::TopologyMetadata {
                    name: "test-topo".into(),
                    description: None,
                    version: None,
                    tags: vec![],
                },
                network: crate::topology::NetworkConfig {
                    name: "test-net".into(),
                    subnet: "10.0.0.0/24".into(),
                    conditions: None,
                },
                nodes: vec![],
            },
            backend_type: "docker".into(),
            node_ids: vec!["n1".into(), "n2".into()],
            network_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let json = lab_metadata_to_json(&meta);
        assert_eq!(json["name"], "test-lab");
        assert_eq!(json["nodes"], 2);
        assert_eq!(json["topology"], "test-topo");
    }

    #[tokio::test]
    async fn test_dispatch_unknown_method_is_not_found() {
        let state = ServerState::new().await.expect("server state");
        let err = dispatch("not.a.real.method", json!({}), &state)
            .await
            .expect_err("expected NotFound");
        assert!(matches!(err, crate::server::MethodError::NotFound));
    }

    #[tokio::test]
    async fn test_dispatch_health_liveness_through_router() {
        let state = ServerState::new().await.expect("server state");
        let v = dispatch("health.liveness", json!({}), &state)
            .await
            .expect("liveness");
        assert_eq!(v["status"], "alive");
        assert_eq!(v["version"], crate::VERSION);
    }

    #[tokio::test]
    async fn test_topology_validate_inline_valid() {
        let state = ServerState::new().await.expect("server state");
        let topo = json!({
            "metadata": { "name": "t" },
            "network": { "name": "n", "subnet": "10.0.0.0/24" },
            "nodes": [{
                "name": "a1",
                "image": "alpine:latest",
            }]
        });
        let v = dispatch("topology.validate", json!({ "topology": topo }), &state)
            .await
            .expect("validate");
        assert_eq!(v["valid"], true);
        assert_eq!(v["name"], "t");
        assert_eq!(v["nodes"], 1);
    }

    #[tokio::test]
    async fn test_topology_validate_inline_invalid_subnet() {
        let state = ServerState::new().await.expect("server state");
        let topo = json!({
            "metadata": { "name": "bad" },
            "network": { "name": "n", "subnet": "10.0.0.0" },
            "nodes": []
        });
        let v = dispatch("topology.validate", json!({ "topology": topo }), &state)
            .await
            .expect("validate");
        assert_eq!(v["valid"], false);
        assert!(v["error"].as_str().unwrap_or("").contains("subnet"));
    }

    #[tokio::test]
    async fn test_lab_create_missing_name_is_invalid_params() {
        let state = ServerState::new().await.expect("server state");
        let err = dispatch("lab.create", json!({ "topology": {} }), &state)
            .await
            .expect_err("params");
        match err {
            super::MethodError::InvalidParams(s) => assert!(s.contains("name")),
            e => panic!("unexpected: {e:?}"),
        }
    }

    #[tokio::test]
    async fn test_node_health_missing_lab_is_invalid_params() {
        let state = ServerState::new().await.expect("server state");
        let err = dispatch("node.health", json!({ "node": "n1" }), &state)
            .await
            .expect_err("params");
        match err {
            crate::server::MethodError::InvalidParams(s) => assert!(s.contains("lab")),
            e => panic!("unexpected: {e:?}"),
        }
    }
}
