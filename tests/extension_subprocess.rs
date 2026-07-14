//! Integration tests for the `Subprocess` extension mechanism
//! (`docs/revamp/C3-extension-protocol-design.md` §2) against the real
//! `reference-extension-echo` child process (`src/bin/reference_extension_echo.rs`).
//!
//! Lives here (not as a `src/extension/subprocess.rs` unit test) because
//! `CARGO_BIN_EXE_reference-extension-echo` is only defined by cargo for
//! INTEGRATION test targets of a package that defines the `[[bin]]` — never
//! for that package's own lib unit tests.

use bastion::extension::facade::{ExtensionInstance, HostFacade};
use bastion::extension::subprocess::SubprocessExtension;
use bastion_extension_protocol::{
    EgressScope, Entrypoint, ExtensionKind, ExtensionManifest, MemoryScope, PermissionSet, Provided,
};
use bastion_runtime::capability::{CapabilityRegistry, InvokeCtx};

/// Path to the `reference-extension-echo` bin target.
fn echo_bin() -> String {
    env!("CARGO_BIN_EXE_reference-extension-echo").to_string()
}

fn manifest(permissions: PermissionSet) -> ExtensionManifest {
    ExtensionManifest {
        id: "acme/echo".to_string(),
        version: semver::Version::new(1, 0, 0),
        kind: ExtensionKind::Subprocess,
        compat: semver::VersionReq::parse("*").unwrap(),
        provides: vec![Provided::Capability("acme/echo:call".to_string())],
        requires: vec![],
        permissions,
        secrets: vec![],
        entrypoint: Entrypoint::Subprocess {
            command: echo_bin(),
            args: vec![],
        },
        migrations: vec![],
        signature: None,
    }
}

fn ctx(owner: &str) -> InvokeCtx {
    InvokeCtx {
        owner: owner.to_string(),
        privacy_tier: Some(bastion_memory::PrivacyTier::CloudOk),
    }
}

async fn install_echo(permissions: PermissionSet) -> (CapabilityRegistry, ExtensionManifest) {
    let m = manifest(PermissionSet {
        capabilities: vec!["acme/echo:call".to_string()],
        ..permissions
    });
    let ext = SubprocessExtension::new(
        m.clone(),
        vec![(
            "acme/echo:call".to_string(),
            "echoes its input back".to_string(),
            serde_json::json!({}),
            echo_bin(),
            vec![],
        )],
    );
    let mut registry = CapabilityRegistry::new();
    {
        let mut facade = HostFacade::new(&m, "alice", &mut registry);
        ext.activate(&mut facade).await.expect("activate succeeds");
    }
    (registry, m)
}

#[tokio::test]
async fn plain_call_echoes_input_over_the_wire() {
    let (registry, _m) = install_echo(PermissionSet::none()).await;
    let result = registry
        .invoke(
            "acme/echo:call",
            serde_json::json!({"hello": "world"}),
            &ctx("alice"),
        )
        .await
        .expect("subprocess round-trip should succeed");
    assert_eq!(result.data["echo"], serde_json::json!({"hello": "world"}));
    assert!(!result.trusted, "subprocess output defaults to untrusted");
}

#[tokio::test]
async fn host_mediated_egress_fetch_denied_without_grant() {
    let (registry, _m) = install_echo(PermissionSet::none()).await;
    let result = registry
        .invoke(
            "acme/echo:call",
            serde_json::json!({"fetch_host": "evil.com"}),
            &ctx("alice"),
        )
        .await
        .expect("the call itself succeeds — denial is IN the response");
    let host_response = &result.data["host_response"];
    assert_eq!(host_response["ok"], serde_json::json!(false));
    assert!(host_response["error"].as_str().unwrap().contains("egress"));
}

#[tokio::test]
async fn host_mediated_egress_fetch_allowed_with_grant() {
    let (registry, _m) = install_echo(PermissionSet {
        egress: EgressScope::Hosts(vec!["api.x.com".to_string()]),
        ..PermissionSet::none()
    })
    .await;
    let result = registry
        .invoke(
            "acme/echo:call",
            serde_json::json!({"fetch_host": "api.x.com"}),
            &ctx("alice"),
        )
        .await
        .expect("call succeeds");
    let host_response = &result.data["host_response"];
    assert_eq!(host_response["ok"], serde_json::json!(true));
    assert_eq!(
        host_response["data"]["authorized_host"],
        serde_json::json!("api.x.com")
    );
}

#[tokio::test]
async fn host_mediated_memory_read_denied_cross_owner() {
    let (registry, _m) = install_echo(PermissionSet {
        memory_scope: MemoryScope::ReadOwn,
        ..PermissionSet::none()
    })
    .await;
    let result = registry
        .invoke(
            "acme/echo:call",
            serde_json::json!({"read_memory_owner": "bob"}),
            &ctx("alice"),
        )
        .await
        .expect("call succeeds");
    let host_response = &result.data["host_response"];
    assert_eq!(host_response["ok"], serde_json::json!(false));
}

#[tokio::test]
async fn host_mediated_memory_read_allowed_for_own_owner() {
    let (registry, _m) = install_echo(PermissionSet {
        memory_scope: MemoryScope::ReadOwn,
        ..PermissionSet::none()
    })
    .await;
    let result = registry
        .invoke(
            "acme/echo:call",
            serde_json::json!({"read_memory_owner": "alice"}),
            &ctx("alice"),
        )
        .await
        .expect("call succeeds");
    let host_response = &result.data["host_response"];
    assert_eq!(host_response["ok"], serde_json::json!(true));
}

#[tokio::test]
async fn host_mediated_network_bind_denied_without_grant() {
    let (registry, _m) = install_echo(PermissionSet::none()).await;
    let result = registry
        .invoke(
            "acme/echo:call",
            serde_json::json!({"bind_port": 8080}),
            &ctx("alice"),
        )
        .await
        .expect("call succeeds");
    let host_response = &result.data["host_response"];
    assert_eq!(host_response["ok"], serde_json::json!(false));
}

/// Adversarial vector (a) over the subprocess wire: even a child that ASKS
/// the host to register an undeclared capability mid-`invoke()` is denied —
/// structurally (no `CapabilityRegistry` handle reaches `invoke()` at all)
/// and by policy (the capability was never declared).
#[tokio::test]
async fn host_mediated_register_capability_always_denied() {
    let (registry, _m) = install_echo(PermissionSet::none()).await;
    let result = registry
        .invoke(
            "acme/echo:call",
            serde_json::json!({"attempt_register_capability": true}),
            &ctx("alice"),
        )
        .await
        .expect("call succeeds — denial is IN the response, not a hard invoke() failure");
    let host_response = &result.data["host_response"];
    assert_eq!(host_response["ok"], serde_json::json!(false));

    // The smuggled capability name never actually reaches the registry.
    assert!(!registry.list_names().contains(&"acme/echo:smuggled"));
}
