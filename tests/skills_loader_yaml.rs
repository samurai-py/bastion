//! Integration tests for SkillsLoader YAML frontmatter parsing (PKG-09, D-15).
//! Wave 0 stubs — all tests marked ignored until load_all is implemented (Wave 3).

#[test]
#[ignore = "Wave 3: SkillsLoader::load_all not yet implemented"]
fn skills_loader_yaml_frontmatter_name_parsed() {
    // Assert: SKILL.md with YAML frontmatter name: "weekly-review" parses correctly
    todo!()
}

#[test]
#[ignore = "Wave 3: SkillsLoader::load_all not yet implemented"]
fn skills_loader_yaml_frontmatter_description_parsed() {
    // Assert: description field extracted from YAML frontmatter
    todo!()
}

#[test]
#[ignore = "Wave 3: SkillsLoader::load_all not yet implemented"]
fn agentskills_compat_reference_skill_loads() {
    // Assert: skills/weekly-review/SKILL.md loads without modification
    // and metadata.name is non-empty, metadata.description is non-empty
    todo!()
}

#[test]
#[ignore = "Wave 3: SkillsLoader::load_all not yet implemented"]
fn skills_loader_scan_directory_returns_all_skills() {
    // Assert: load_all("skills/") returns non-empty Vec<SkillMetadata>
    todo!()
}
