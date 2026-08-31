//! Integration tests for the `agent-browser mcp` stdio server.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_agent-browser");

fn wait_for_daemon_exit(pid_path: &std::path::Path) {
    for _ in 0..100 {
        if !pid_path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    if let Ok(pid) = std::fs::read_to_string(pid_path) {
        #[cfg(unix)]
        if let Ok(pid) = pid.trim().parse::<i32>() {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }

        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", pid.trim(), "/F"])
                .output();
        }
    }

    panic!("MCP test daemon did not exit after idle timeout");
}

#[test]
fn mcp_startup_action_policy_applies_to_tool_subprocesses() {
    let tmp = TempDir::new().unwrap();
    let socket_dir = tmp.path().join("sockets");
    let home = tmp.path().join("home");
    let policy_path = tmp.path().join("policy.json");
    let daemon_pid_path = socket_dir.join("mcp-policy-test.pid");
    std::fs::create_dir_all(&socket_dir).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(&policy_path, r#"{"deny":["close"]}"#).unwrap();

    let mut child = Command::new(BIN)
        .args(["--action-policy", policy_path.to_str().unwrap(), "mcp"])
        .current_dir(tmp.path())
        .env("AGENT_BROWSER_SOCKET_DIR", &socket_dir)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env_remove("AGENT_BROWSER_ALLOWED_DOMAINS")
        .env_remove("AGENT_BROWSER_ACTION_POLICY")
        .env_remove("AGENT_BROWSER_POLICY")
        .env_remove("AGENT_BROWSER_CONFIRM_ACTIONS")
        .env("AGENT_BROWSER_IDLE_TIMEOUT_MS", "100")
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start MCP server");

    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "agent_browser_close",
            "arguments": {
                "session": "mcp-policy-test"
            }
        }
    });
    let shutdown = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "shutdown"
    });

    {
        let stdin = child.stdin.as_mut().expect("MCP stdin should be piped");
        writeln!(stdin, "{request}").unwrap();
        writeln!(stdin, "{shutdown}").unwrap();
    }
    drop(child.stdin.take());

    let output = child
        .wait_with_output()
        .expect("failed to wait for MCP server");
    wait_for_daemon_exit(&daemon_pid_path);
    assert!(
        output.status.success(),
        "MCP server failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("MCP stdout should be UTF-8");
    let response: Value =
        serde_json::from_str(stdout.lines().next().expect("missing MCP tool response")).unwrap();

    assert_eq!(response["result"]["isError"], true, "{response}");
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("denied by policy")),
        "{response}"
    );
}
