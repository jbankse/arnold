use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Syscall {
    // Conversation lifecycle
    #[serde(rename = "sys_reply")]
    Reply { text: String },
    #[serde(rename = "sys_done")]
    Done {},

    // File / project
    #[serde(rename = "sys_read_file")]
    ReadFile { path: String },
    #[serde(rename = "sys_write_file")]
    WriteFile { path: String, contents: String },
    #[serde(rename = "sys_replace_in_file")]
    ReplaceInFile { path: String, old_string: String, new_string: String },
    #[serde(rename = "sys_list_dir")]
    ListDir { path: String },
    #[serde(rename = "sys_search")]
    Search { query: String, path: Option<String>, glob: Option<String> },

    // Shell
    #[serde(rename = "sys_run_command")]
    RunCommand {
        cmd: String,
        #[serde(default)] args: Vec<String>,
        #[serde(default)] cwd: Option<String>,
        #[serde(default)] timeout_ms: Option<u64>,
        #[serde(default)] stdin: Option<String>,
    },

    // Web
    #[serde(rename = "sys_web_fetch")]
    WebFetch { url: String },

    // Memory
    #[serde(rename = "sys_memory_read")]
    MemoryRead { topic: String },
    #[serde(rename = "sys_memory_write")]
    MemoryWrite { topic: String, content: String },
    #[serde(rename = "sys_memory_list")]
    MemoryList {},
}

impl Syscall {
    pub fn method(&self) -> &'static str {
        use Syscall::*;
        match self {
            Reply { .. } => "sys_reply",
            Done {} => "sys_done",
            ReadFile { .. } => "sys_read_file",
            WriteFile { .. } => "sys_write_file",
            ReplaceInFile { .. } => "sys_replace_in_file",
            ListDir { .. } => "sys_list_dir",
            Search { .. } => "sys_search",
            RunCommand { .. } => "sys_run_command",
            WebFetch { .. } => "sys_web_fetch",
            MemoryRead { .. } => "sys_memory_read",
            MemoryWrite { .. } => "sys_memory_write",
            MemoryList {} => "sys_memory_list",
        }
    }

    pub fn all_methods() -> &'static [&'static str] {
        &[
            "sys_reply", "sys_done",
            "sys_read_file", "sys_write_file", "sys_replace_in_file", "sys_list_dir", "sys_search",
            "sys_run_command",
            "sys_web_fetch",
            "sys_memory_read", "sys_memory_write", "sys_memory_list",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_reply() {
        let s = Syscall::Reply { text: "hi".into() };
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["method"], "sys_reply");
        let back: Syscall = serde_json::from_value(j).unwrap();
        assert!(matches!(back, Syscall::Reply { .. }));
    }

    #[test]
    fn all_methods_matches_enum() {
        // Sanity: hand-maintained list matches the actual variants
        assert_eq!(Syscall::all_methods().len(), 12);
    }
}
