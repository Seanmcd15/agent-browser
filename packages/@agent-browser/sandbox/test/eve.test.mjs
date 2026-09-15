import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { access, chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { promisify } from "node:util";

import {
  agentBrowserRevalidationKey,
  buildAgentBrowserCommand,
  installAgentBrowser,
  runAgentBrowser,
} from "../dist/eve.js";

const execFileAsync = promisify(execFile);

test("builds Eve revalidation key from install options", () => {
  assert.equal(
    agentBrowserRevalidationKey({ installSpec: "agent-browser@1.2.3" }),
    "agent-browser:bootstrap-4:agent-browser@1.2.3:browser:system-deps",
  );
  assert.equal(
    agentBrowserRevalidationKey({ installSpec: "agent-browser@1.2.3", installSystemDependencies: false }),
    "agent-browser:bootstrap-4:agent-browser@1.2.3:browser:no-system-deps",
  );
});

test("builds Eve shell command", () => {
  assert.equal(
    buildAgentBrowserCommand(["open", "https://example.com"], { session: "s1" }),
    "agent-browser --session s1 open https://example.com --json",
  );
});

test("installs agent-browser in an Eve sandbox", async () => {
  const commands = [];
  const sandbox = {
    id: "sandbox-1",
    async run({ command }) {
      commands.push(command);
      return { exitCode: 0, stdout: "", stderr: "" };
    },
  };

  await installAgentBrowser(sandbox, { installSpec: "agent-browser@1.2.3" });

  assert.equal(commands.length, 3);
  assert.match(commands[0], /^if command -v apt-get/);
  assert.match(commands[0], /libglib2\.0-0t64/);
  assert.match(commands[0], /libasound2t64/);
  assert.match(commands[0], /sudo ldconfig; elif command -v dnf/);
  assert.match(commands[0], /sudo ldconfig; else echo/);
  assert.match(commands[0], /sudo dnf install -y --skip-broken -- glib2 nss/);
  assert.equal(commands[1], "npm install -g agent-browser@1.2.3");
  assert.equal(commands[2], "agent-browser install");
});

test("validates the Eve apt plan before installing packages", async () => {
  const directory = await mkdtemp(join(tmpdir(), "agent-browser-eve-apt-"));
  const installMarker = join(directory, "install-ran");
  const writeExecutable = async (name, contents) => {
    const path = join(directory, name);
    await writeFile(path, contents);
    await chmod(path, 0o755);
  };

  try {
    await writeExecutable("sudo", "#!/bin/sh\nexec \"$@\"\n");
    await writeExecutable(
      "apt-get",
      [
        "#!/bin/sh",
        "if [ \"$1\" = update ]; then exit 0; fi",
        "if [ \"$1\" = install ] && [ \"$2\" = --simulate ]; then",
        "  printf '%s\\n' \"$APT_SIMULATION_OUTPUT\"",
        "  exit 0",
        "fi",
        "if [ \"$1\" = install ] && [ \"$2\" = -y ]; then",
        "  : > \"$INSTALL_MARKER\"",
        "  exit 0",
        "fi",
        "exit 1",
        "",
      ].join("\n"),
    );
    await writeExecutable("apt-cache", "#!/bin/sh\nexit 1\n");
    await writeExecutable(
      "grep",
      [
        "#!/bin/sh",
        "while IFS= read -r line; do",
        "  case \"$line\" in 'Remv '*) exit 0;; esac",
        "done",
        "exit 1",
        "",
      ].join("\n"),
    );
    await writeExecutable("ldconfig", "#!/bin/sh\nexit 0\n");

    let simulationOutput = "Remv critical-package [1.0]";
    let invocation = 0;
    const sandbox = {
      id: "sandbox-1",
      async run({ command }) {
        invocation += 1;
        if (invocation > 1) {
          return { exitCode: 0, stdout: "", stderr: "" };
        }
        try {
          const result = await execFileAsync("/bin/sh", ["-c", command], {
            env: { APT_SIMULATION_OUTPUT: simulationOutput, INSTALL_MARKER: installMarker, PATH: directory },
          });
          return { exitCode: 0, stdout: result.stdout, stderr: result.stderr };
        } catch (error) {
          return {
            exitCode: typeof error.code === "number" ? error.code : 1,
            stdout: error.stdout ?? "",
            stderr: error.stderr ?? "",
          };
        }
      },
    };

    await assert.rejects(
      () => installAgentBrowser(sandbox, { installSpec: "agent-browser@1.2.3" }),
      /apt would remove installed packages/,
    );
    await assert.rejects(() => access(installMarker), { code: "ENOENT" });

    simulationOutput = "";
    invocation = 0;
    await installAgentBrowser(sandbox, { installSpec: "agent-browser@1.2.3" });
    await access(installMarker);

    await rm(installMarker);
    await rm(join(directory, "grep"));
    simulationOutput = "Remv critical-package [1.0]";
    invocation = 0;
    await assert.rejects(
      () => installAgentBrowser(sandbox, { installSpec: "agent-browser@1.2.3" }),
      /could not check apt simulation for package removals/,
    );
    await assert.rejects(() => access(installMarker), { code: "ENOENT" });
  } finally {
    await rm(directory, { force: true, recursive: true });
  }
});

test("skips Eve system dependencies when explicitly disabled", async () => {
  const commands = [];
  const sandbox = {
    id: "sandbox-1",
    async run({ command }) {
      commands.push(command);
      return { exitCode: 0, stdout: "", stderr: "" };
    },
  };

  await installAgentBrowser(sandbox, {
    installSpec: "agent-browser@1.2.3",
    installSystemDependencies: false,
  });

  assert.deepEqual(commands, ["npm install -g agent-browser@1.2.3", "agent-browser install"]);
});

test("runs agent-browser through ctx.getSandbox", async () => {
  const commands = [];
  const ctx = {
    async getSandbox() {
      return {
        id: "sandbox/id 1",
        async run({ command }) {
          commands.push(command);
          return { exitCode: 0, stdout: '{"ok":true}', stderr: "" };
        },
      };
    },
  };

  const result = await runAgentBrowser(ctx, ["open", "https://example.com"]);

  assert.deepEqual(result.json, { ok: true });
  assert.equal(commands[0], "agent-browser --session eve-sandbox-id-1 open https://example.com --json");
});

test("uses a short generated session for long Eve sandbox ids", async () => {
  const commands = [];
  const ctx = {
    async getSandbox() {
      return {
        id: "eve-sbx-ses-vercel-1d940340bdba4563-wrun_01KVKDK1Z3GC3XEC86DGWRWRMH-__root__",
        async run({ command }) {
          commands.push(command);
          return { exitCode: 0, stdout: '{"ok":true}', stderr: "" };
        },
      };
    },
  };

  await runAgentBrowser(ctx, ["open", "https://example.com"]);

  const session = commands[0].match(/--session ([^ ]+)/)?.[1];
  assert.equal(session.length <= 48, true);
  assert.match(commands[0], /^agent-browser --session eve-eve-sbx-ses-vercel-.+-[a-f0-9]{8} open/);
});

test("accepts Eve promise-like sandbox methods", async () => {
  const thenable = (value) => ({
    then(resolve) {
      resolve(value);
    },
  });
  const ctx = {
    getSandbox() {
      return thenable({
        id: "sandbox-1",
        run() {
          return thenable({ exitCode: 0, stdout: '{"ok":true}', stderr: "" });
        },
      });
    },
  };

  const result = await runAgentBrowser(ctx, ["snapshot"]);

  assert.deepEqual(result.json, { ok: true });
});

test("throws when Eve sandbox command fails", async () => {
  const ctx = {
    async getSandbox() {
      return {
        id: "sandbox-1",
        async run() {
          return { exitCode: 2, stdout: "", stderr: "no chrome" };
        },
      };
    },
  };

  await assert.rejects(() => runAgentBrowser(ctx, ["snapshot"]), /no chrome/);
});
