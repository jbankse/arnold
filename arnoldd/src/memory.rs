use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    User,
    Feedback,
    Project,
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTopic {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub kind: MemoryType,
    pub body: String,
}

#[derive(Clone)]
pub struct MemoryStore {
    root: PathBuf,
}

impl MemoryStore {
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let index = root.join("MEMORY.md");
        if !index.exists() {
            std::fs::write(&index, "# Arnold Memory\n\n_No entries yet._\n")?;
        }
        Ok(Self { root: root.to_path_buf() })
    }

    pub fn index(&self) -> Result<String> {
        Ok(std::fs::read_to_string(self.root.join("MEMORY.md"))?)
    }

    pub fn read(&self, topic: &str) -> Result<MemoryTopic> {
        let path = self.topic_path(topic);
        let text = std::fs::read_to_string(&path)?;
        parse_topic(&text)
    }

    pub fn write(&self, topic: &str, content: &MemoryTopic) -> Result<()> {
        let path = self.topic_path(topic);
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&path, serialize_topic(content))?;
        self.rebuild_index()?;
        Ok(())
    }

    fn topic_path(&self, topic: &str) -> PathBuf {
        // Disallow path traversal at this layer; jail layer (Task 8) also checks.
        let safe: String = topic.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '/').collect();
        self.root.join(format!("{safe}.md"))
    }

    fn rebuild_index(&self) -> Result<()> {
        let mut lines = vec!["# Arnold Memory".to_string(), String::new()];
        for entry in walkdir::WalkDir::new(&self.root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.is_file() { continue; }
            if p.file_name().and_then(|n| n.to_str()) == Some("MEMORY.md") { continue; }
            if p.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
            let rel = p.strip_prefix(&self.root).unwrap_or(p);
            let text = std::fs::read_to_string(p).unwrap_or_default();
            let topic = parse_topic(&text).ok();
            let (name, desc) = match topic {
                Some(t) => (t.name, t.description),
                None => (rel.display().to_string(), "(unparsed)".to_string()),
            };
            lines.push(format!("- [{}]({}) — {}", name, rel.display(), desc));
        }
        lines.push(String::new());
        std::fs::write(self.root.join("MEMORY.md"), lines.join("\n"))?;
        Ok(())
    }
}

fn parse_topic(text: &str) -> Result<MemoryTopic> {
    let stripped = text.strip_prefix("---\n").ok_or_else(|| anyhow::anyhow!("missing frontmatter"))?;
    let end = stripped.find("\n---\n").ok_or_else(|| anyhow::anyhow!("unterminated frontmatter"))?;
    let frontmatter = &stripped[..end];
    let body = &stripped[end + 5..];

    #[derive(Deserialize)]
    struct Fm { name: String, description: String, #[serde(rename = "type")] kind: MemoryType }
    let fm: Fm = serde_yaml::from_str(frontmatter)?;

    Ok(MemoryTopic {
        name: fm.name,
        description: fm.description,
        kind: fm.kind,
        body: body.to_string(),
    })
}

fn serialize_topic(t: &MemoryTopic) -> String {
    #[derive(serde::Serialize)]
    struct Fm<'a> {
        name: &'a str,
        description: &'a str,
        #[serde(rename = "type")]
        kind: &'a MemoryType,
    }
    let fm = Fm { name: &t.name, description: &t.description, kind: &t.kind };
    let yaml = serde_yaml::to_string(&fm).expect("yaml serialize");
    format!("---\n{}---\n{}", yaml, t.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_then_read_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = MemoryStore::open(tmp.path()).unwrap();
        let topic = MemoryTopic {
            name: "Test Topic".into(),
            description: "describes the topic".into(),
            kind: MemoryType::User,
            body: "body text here\n".into(),
        };
        store.write("test_topic", &topic).unwrap();
        let read = store.read("test_topic").unwrap();
        assert_eq!(read.name, "Test Topic");
        assert_eq!(read.kind, MemoryType::User);

        let index = store.index().unwrap();
        assert!(index.contains("Test Topic"));
    }

    #[test]
    fn round_trip_with_colon_in_name() {
        let tmp = TempDir::new().unwrap();
        let store = MemoryStore::open(tmp.path()).unwrap();
        let topic = MemoryTopic {
            name: "Decision provenance: Arnold v0 choices".into(),
            description: "the why behind: load-bearing v0 picks".into(),
            kind: MemoryType::Project,
            body: "body\n".into(),
        };
        store.write("provenance", &topic).unwrap();
        let read = store.read("provenance").unwrap();
        assert_eq!(read.name, "Decision provenance: Arnold v0 choices");
        assert_eq!(read.description, "the why behind: load-bearing v0 picks");
        assert_eq!(read.kind, MemoryType::Project);
    }
}
