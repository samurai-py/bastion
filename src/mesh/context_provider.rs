//! MeshSliceProvider: TurnContextProvider impl that injects remote owner's slice via SEAM #2.
//! Full implementation in Phase 6 Plan 02 (Wave 2).
//! Stub here so src/mesh/mod.rs module declaration compiles.

use crate::agent::context::{ContextBlock, TurnContextProvider};

pub struct MeshSliceProvider;

#[async_trait::async_trait]
impl TurnContextProvider for MeshSliceProvider {
    async fn context_for_turn(&self, _owner: &str, _turn_msg: &str) -> Vec<ContextBlock> {
        vec![] // stub — full impl in Plan 02
    }
}
