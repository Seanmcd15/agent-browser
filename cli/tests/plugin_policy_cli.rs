//! Integration tests for policy enforcement on standalone plugin commands.

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use tempfile::TempDir;

    const BIN: &str = env!("CARGO_BIN_EXE_agent-browser");

    fn plugin_command(tmp: &TempDir, extra_args: &[&str]) -> (Command, std::path::PathBuf) {
        let marker = tmp.path().join("plugin-ran");
        let plugin = tmp.path().join("plugin");
        std::fs::write(
            &plugin,
            format!(
                "#!/bin/sh\ncat >/dev/null\nprintf ran > '{}'\nprintf '%s' '{{\"protocol\":\"agent-browser.plugin.v1\",\"success\":true,\"data\":{{}}}}'\n",
                marker.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&plugin).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&plugin, permissions).unwrap();

        let plugins = serde_json::json!([{
            "name": "danger",
            "command": plugin,
            "capabilities": ["command.run"]
        }]);
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();

        let mut command = Command::new(BIN);
        command
            .arg("--json")
            .args(extra_args)
            .args(["plugin", "run", "danger", "danger.run"])
            .env("AGENT_BROWSER_PLUGINS", plugins.to_string())
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("NO_COLOR", "1");
        (command, marker)
    }

    #[test]
    fn plugin_run_obeys_denied_action_policy() {
        let tmp = TempDir::new().unwrap();
        let policy = tmp.path().join("policy.json");
        std::fs::write(&policy, r#"{"deny":["plugin:danger:command.run"]}"#).unwrap();
        let policy_arg = policy.to_string_lossy().into_owned();
        let (mut command, marker) = plugin_command(&tmp, &["--action-policy", policy_arg.as_str()]);

        let output = command.output().expect("failed to invoke plugin command");

        assert!(!output.status.success());
        assert!(!marker.exists(), "denied plugin process was executed");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("denied by policy"),
            "unexpected stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    #[test]
    fn plugin_run_fails_closed_when_confirmation_cannot_be_requested() {
        let tmp = TempDir::new().unwrap();
        let (mut command, marker) =
            plugin_command(&tmp, &["--confirm-actions", "plugin:danger:command.run"]);

        let output = command.output().expect("failed to invoke plugin command");

        assert!(!output.status.success());
        assert!(!marker.exists(), "unconfirmed plugin process was executed");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("requires confirmation"),
            "unexpected stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
