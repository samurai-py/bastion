/// SkillsLoader loads skills from filesystem (SKILL.md + Rust trait impls).
///
/// PHASE 1 SCOPE: Full stub — returns empty vec without FS access.
/// No skills exist yet at Phase 1; FS scanning added in Phase 2 when skills ship.
/// Phase 2 implementation: scan skills_dir, parse SKILL.md <name> and <description> fields.
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
}

pub struct SkillsLoader;

impl SkillsLoader {
    pub fn load_all(_skills_dir: &str) -> anyhow::Result<Vec<SkillMetadata>> {
        // Phase 1 stub — no skills to load yet. Phase 2 implements real directory scan.
        Ok(vec![])
    }
}
