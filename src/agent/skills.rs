/// SkillsLoader loads skills from filesystem (SKILL.md + Rust trait impls).
///
/// PHASE 1 SCOPE: `load_all` stub returns empty vec without FS access.
/// `rescan` is implemented here (Phase 3, D-06): parses a single SKILL.md on demand,
/// called by AgentLoop after receiving a `skill_reloaded` signal from skill-writer.
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
}

pub struct SkillsLoader;

impl SkillsLoader {
    pub fn load_all(_skills_dir: &str) -> anyhow::Result<Vec<SkillMetadata>> {
        // Phase 1 stub — no skills to load yet. Phase 4 implements real directory scan.
        Ok(vec![])
    }

    // RED: rescan not implemented yet — tests will fail to compile
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rescan_valid_skill_md_returns_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("SKILL.md");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "<name>weekly-review</name>").unwrap();
        writeln!(f, "<description>Runs a weekly review session</description>").unwrap();

        let meta = SkillsLoader::rescan(path.to_str().unwrap()).unwrap();
        assert_eq!(meta.name, "weekly-review");
        assert_eq!(meta.description, "Runs a weekly review session");
    }

    #[test]
    fn rescan_missing_file_returns_err() {
        let result = SkillsLoader::rescan("/tmp/nonexistent-skill-xyz/SKILL.md");
        assert!(result.is_err(), "should error on missing file");
    }

    #[test]
    fn rescan_skill_md_missing_name_tag_falls_back_to_dir() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("my-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        let path = skill_dir.join("SKILL.md");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "<description>some desc</description>").unwrap();

        let meta = SkillsLoader::rescan(path.to_str().unwrap()).unwrap();
        assert_eq!(meta.name, "my-skill");
        assert_eq!(meta.description, "some desc");
    }

    #[test]
    fn rescan_extracts_multiline_description() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("SKILL.md");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "<name>test-skill</name>").unwrap();
        writeln!(f, "<description>").unwrap();
        writeln!(f, "  Line one.").unwrap();
        writeln!(f, "  Line two.").unwrap();
        writeln!(f, "</description>").unwrap();

        let meta = SkillsLoader::rescan(path.to_str().unwrap()).unwrap();
        assert_eq!(meta.name, "test-skill");
        assert!(meta.description.contains("Line one."), "desc: {}", meta.description);
    }
}
