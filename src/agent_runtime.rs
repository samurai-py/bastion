//! Shim (M2 step 5): the `AgentRuntime` contract (A-01) and its conformance
//! suite (A-02) moved to `bastion-agent-runtime`. Re-exported here so every
//! existing `bastion::agent_runtime::...` path keeps compiling unchanged.

pub use bastion_agent_runtime::*;
