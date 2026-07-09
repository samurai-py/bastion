//! Automated validation of the stigmergy substrate exposed by Memory.
//!
//! These tests use a real SQLite store: pheromone deposit maps to
//! `reinforce_belief`, evaporation maps to `evaporate_beliefs`, and retrieval
//! bias is observed through `MemoryRagProvider` ranking by lexical overlap and
//! weight.

use anyhow::Result;
use bastion::agent::context::TurnContextProvider;
use bastion::agent::memory_rag::MemoryRagProvider;
use bastion::memory::sqlite::SqliteMemory;
use bastion::memory::{Memory, PrivacyTier, SharedMemory};
use bastion::session::SessionManager;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::RwLock;

async fn make_memory() -> Result<(NamedTempFile, SqliteMemory)> {
    let file = NamedTempFile::new()?;
    let path = file.path().to_str().expect("utf-8 temp path").to_owned();
    SessionManager::new(&path).init_schema().await?;
    Ok((file, SqliteMemory::new(path)))
}

async fn make_shared_memory() -> Result<(NamedTempFile, SharedMemory)> {
    let file = NamedTempFile::new()?;
    let path = file.path().to_str().expect("utf-8 temp path").to_owned();
    SessionManager::new(&path).init_schema().await?;
    Ok((
        file,
        Arc::new(RwLock::new(
            Box::new(SqliteMemory::new(path)) as Box<dyn Memory>
        )),
    ))
}

async fn store_belief(memory: &dyn Memory, owner: &str, content: &str) -> Result<i64> {
    memory
        .store_belief(
            owner,
            None,
            content,
            "stigmergy-test-session",
            "stigmergy_test",
            false,
            Some(PrivacyTier::CloudOk),
        )
        .await
}

async fn weight(memory: &dyn Memory, owner: &str, id: i64) -> Result<f64> {
    let beliefs = memory.retrieve_tagged(owner, None).await?;
    beliefs
        .into_iter()
        .find(|belief| belief.id == id)
        .map(|belief| belief.weight)
        .ok_or_else(|| anyhow::anyhow!("belief {id} not found"))
}

#[tokio::test]
async fn test_reinforce_belief_increases_weight() -> Result<()> {
    let (_file, memory) = make_memory().await?;
    let id = store_belief(&memory, "alice", "procedural trail").await?;

    let before = weight(&memory, "alice", id).await?;
    memory.reinforce_belief("alice", id, 0.75).await?;
    let after = weight(&memory, "alice", id).await?;

    assert!(
        after > before,
        "weight should increase: {before} -> {after}"
    );
    assert_eq!(after, before + 0.75);
    Ok(())
}

#[tokio::test]
async fn reinforce_belief_is_owner_scoped() -> Result<()> {
    let (_file, memory) = make_memory().await?;
    let id = store_belief(&memory, "alice", "private procedural trail").await?;

    let err = memory
        .reinforce_belief("bob", id, 1.0)
        .await
        .expect_err("wrong owner must not reinforce");
    assert!(err.to_string().contains("not found for owner"));
    assert_eq!(weight(&memory, "alice", id).await?, 1.0);
    Ok(())
}

#[tokio::test]
async fn test_evaporate_beliefs_reduces_weight_without_crossing_floor() -> Result<()> {
    let (_file, memory) = make_memory().await?;
    let first = store_belief(&memory, "alice", "first trail").await?;
    let second = store_belief(&memory, "alice", "second trail").await?;

    memory.reinforce_belief("alice", second, 1.0).await?;
    let first_before = weight(&memory, "alice", first).await?;
    let second_before = weight(&memory, "alice", second).await?;

    let affected = memory.evaporate_beliefs("alice", 0.25, 0.60).await?;

    assert_eq!(affected, 2);
    let first_after = weight(&memory, "alice", first).await?;
    let second_after = weight(&memory, "alice", second).await?;
    assert_eq!(first_after, 0.60, "floor should clamp low weights");
    assert!(
        second_after < second_before && second_after >= 0.60,
        "reinforced weight should decay but respect floor"
    );
    assert!(first_after < first_before);
    Ok(())
}

#[tokio::test]
async fn retrieval_bias_prefers_reinforced_equal_overlap() -> Result<()> {
    let (_file, memory) = make_shared_memory().await?;
    let low_id;
    let high_id;
    {
        let mem = memory.read().await;
        low_id = store_belief(mem.as_ref(), "alice", "alpha routine low priority trail").await?;
        high_id = store_belief(mem.as_ref(), "alice", "alpha routine high priority trail").await?;
        mem.reinforce_belief("alice", high_id, 3.0).await?;
    }

    let provider = MemoryRagProvider::new(memory);
    let blocks = provider.context_for_turn("alice", "alpha routine").await;
    let block = blocks
        .iter()
        .find(|block| block.max_tier == PrivacyTier::CloudOk)
        .expect("cloud memory block");

    let high_pos = block
        .content
        .find(&format!("[id {high_id}]"))
        .expect("reinforced belief rendered");
    let low_pos = block
        .content
        .find(&format!("[id {low_id}]"))
        .expect("low belief rendered");

    assert!(
        high_pos < low_pos,
        "reinforced equal-overlap belief should rank first: {}",
        block.content
    );
    Ok(())
}
