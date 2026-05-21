use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use crate::syscall::Syscall;
use crate::schema::schema_for_methods;

const ROLE_MD: &str = include_str!("../../role/ROLE.md");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallSpec {
    pub method: String,
    pub description: String,
    pub example: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolePayload {
    pub name: String,
    pub path: String,
    pub body: String,
    pub allowed_syscalls: Vec<String>,
    pub examples: String,
}

pub struct PlanFrameBuilder {
    pub task_id: Uuid,
    pub context: String,
    pub model_provider: String,
    pub model: String,
}

impl PlanFrameBuilder {
    pub fn build(&self) -> Result<Value> {
        let methods: Vec<&str> = Syscall::all_methods().to_vec();
        let methods_owned: Vec<String> = methods.iter().map(|s| s.to_string()).collect();
        let syscalls: Vec<SyscallSpec> = arnold_syscall_menu();
        let role_payload = RolePayload {
            name: "assistant".into(),
            path: "role/ROLE.md".into(),
            body: ROLE_MD.to_string(),
            allowed_syscalls: methods_owned.clone(),
            examples: String::new(), // No worked examples for v0a
        };
        let constraint_schema = schema_for_methods(&methods)?;
        Ok(json!({
            "type": "plan",
            "task_id": self.task_id.to_string(),
            "task_type": "chat",
            "role": "task",
            "context": self.context,
            "syscalls": syscalls,
            "role_payloads": [role_payload],
            "constraint_schema": constraint_schema,
            "reasoning_controls": {},
            "worker_id": "self",
            "parent_worker_id": null,
            "parent_task_id": null,
            "l2_controls": {},
            "live_tokens_estimate_first_turn": 0,
            "kernel_prompt": "",
            "tool_priority": { "order": [], "low_priority": [] }
        }))
    }
}

fn arnold_syscall_menu() -> Vec<SyscallSpec> {
    let mk = |m: &str, d: &str, e: Value| SyscallSpec {
        method: m.into(), description: d.into(),
        example: serde_json::to_string(&e).unwrap(),
    };
    vec![
        mk("sys_reply", "Send a user-facing text reply.",
            json!({"method":"sys_reply","params":{"text":"Hi"}})),
        mk("sys_done", "End the current turn and wait for the next inbox event.",
            json!({"method":"sys_done","params":{}})),
        mk("sys_read_file", "Read a file relative to a jail root.",
            json!({"method":"sys_read_file","params":{"path":"src/main.rs"}})),
        mk("sys_write_file", "Write a file (creates parent dirs).",
            json!({"method":"sys_write_file","params":{"path":"notes.md","contents":"..."}})),
        mk("sys_replace_in_file", "Find/replace a unique string in a file.",
            json!({"method":"sys_replace_in_file","params":{"path":"src/main.rs","old_string":"foo","new_string":"bar"}})),
        mk("sys_list_dir", "List entries in a directory.",
            json!({"method":"sys_list_dir","params":{"path":"src"}})),
        mk("sys_search", "Ripgrep-style search.",
            json!({"method":"sys_search","params":{"query":"TODO","path":"src","glob":"*.rs"}})),
        mk("sys_run_command", "Run a bounded command and capture output.",
            json!({"method":"sys_run_command","params":{"cmd":"cargo","args":["check"],"timeout_ms":60000}})),
        mk("sys_web_fetch", "Fetch a URL and return its body.",
            json!({"method":"sys_web_fetch","params":{"url":"https://example.com"}})),
        mk("sys_memory_read", "Read a memory topic file.",
            json!({"method":"sys_memory_read","params":{"topic":"user_preferences"}})),
        mk("sys_memory_write", "Create/update a memory topic.",
            json!({"method":"sys_memory_write","params":{"topic":"user_preferences","content":"---\nname: User Preferences\ndescription: ...\ntype: user\n---\nbody"}})),
        mk("sys_memory_list", "Return the MEMORY.md index.",
            json!({"method":"sys_memory_list","params":{}})),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_frame_includes_all_required_top_level_fields() {
        let b = PlanFrameBuilder {
            task_id: Uuid::nil(),
            context: "test".into(),
            model_provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
        };
        let frame = b.build().unwrap();
        for field in ["type","task_id","task_type","role","context","syscalls",
                      "role_payloads","constraint_schema","worker_id","kernel_prompt","tool_priority"] {
            assert!(frame.get(field).is_some(), "missing field: {field}");
        }
        assert_eq!(frame["role"], "task");
        assert_eq!(frame["task_type"], "chat");
        assert_eq!(frame["syscalls"].as_array().unwrap().len(), 12);
    }
}
