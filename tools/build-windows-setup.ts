/**
 * Build Cilica Setup, Cilica's installer window on Windows (apps/setup, docs/releasing.md).
 *
 * Setup is built twice around Tauri's own build:
 *   1. `prepare`: the frontend, then Setup without an installer inside. That copy can only remove
 *      Cilica; tauri.windows-setup.conf.json bundles it into Cilica's NSIS installer as
 *      `cilica-setup.exe`, and hooks.nsh points Installed apps at it.
 *   2. Tauri builds Cilica and its NSIS installer (`tauri build --config
 *      src-tauri/tauri.windows-setup.conf.json`), as tauri-action does in CI.
 *   3. `package`: Setup again, with that NSIS installer inside (build.rs reads
 *      CILICA_SETUP_PAYLOAD), written to target/release/bundle/setup/Cilica_<version>_x64_Setup.exe.
 *
 * Run on Windows from the repo root:
 *   bun tools/build-windows-setup.ts              all three steps
 *   bun tools/build-windows-setup.ts prepare      step 1, before tauri-action
 *   bun tools/build-windows-setup.ts package      step 3, after it
 *
 * Signing: set WINDOWS_SIGN_COMMAND to a command with `%1` for the file (for example
 * `signtool sign /fd sha256 /tr http://timestamp.digicert.com /td sha256 /a "%1"`) and both
 * copies of Setup are signed. Unsigned, SmartScreen warns on first run.
 */
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const TARGET = process.env.CARGO_TARGET_DIR ?? join(ROOT, "target");
const RELEASE = join(TARGET, "release");
/** Where the remove-only copy waits for Tauri to bundle it (tauri.windows-setup.conf.json). */
const UNINSTALL_COPY = join(TARGET, "windows-setup", "cilica-setup.exe");
const OUT_DIR = join(RELEASE, "bundle", "setup");

function run(command: string, args: string[], env: Record<string, string> = {}) {
  console.log(`\n> ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, {
    cwd: ROOT,
    stdio: "inherit",
    shell: process.platform === "win32",
    env: { ...process.env, ...env },
  });
  if (result.status !== 0) {
    console.error(`\n${command} failed (exit ${result.status}).`);
    process.exit(result.status ?? 1);
  }
}

/** The one place the app version lives: `[workspace.package]` in the root Cargo.toml. */
function version(): string {
  const toml = readFileSync(join(ROOT, "Cargo.toml"), "utf8");
  const section = toml.split("[workspace.package]")[1] ?? "";
  const match = section.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match?.[1]) throw new Error("No version under [workspace.package] in Cargo.toml.");
  return match[1];
}

function sign(file: string) {
  const template = process.env.WINDOWS_SIGN_COMMAND;
  if (!template) return;
  // A whole command line, run by the shell, so quoted paths in it stay whole.
  run(template.replaceAll("%1", file), []);
}

function buildSetup(env: Record<string, string>) {
  run("cargo", ["build", "--release", "-p", "cilica-setup", "--features", "custom-protocol"], env);
  return join(RELEASE, "cilica-setup.exe");
}

/** Step 1: the frontend Setup embeds, then the remove-only copy. */
function prepare() {
  run("bun", ["run", "--cwd", "apps/desktop", "build"]);
  const built = buildSetup({ CILICA_SETUP_PAYLOAD: "", CILICA_SETUP_APP_BYTES: "" });
  mkdirSync(join(TARGET, "windows-setup"), { recursive: true });
  copyFileSync(built, UNINSTALL_COPY);
  sign(UNINSTALL_COPY);
  console.log(`\nRemove-only copy: ${UNINSTALL_COPY}`);
}

/** Tauri's NSIS installer for this version. */
function nsisInstaller(v: string): string {
  const dir = join(RELEASE, "bundle", "nsis");
  const name = existsSync(dir)
    ? readdirSync(dir).find((f) => f.endsWith("-setup.exe") && f.includes(`_${v}_`))
    : undefined;
  if (!name) {
    throw new Error(`No NSIS installer for ${v} in ${dir}. Run Tauri's build first (step 2).`);
  }
  return join(dir, name);
}

/** Cilica's installed size: its program file and the copy of Setup beside it. */
function appBytes(): number {
  const exe = ["Cilica.exe", "cilica-desktop.exe"]
    .map((name) => join(RELEASE, name))
    .find((path) => existsSync(path));
  const size = (path: string | undefined) => (path && existsSync(path) ? statSync(path).size : 0);
  return size(exe) + size(UNINSTALL_COPY);
}

/** Step 3: Setup with Cilica's installer inside. */
function pack() {
  const v = version();
  const installer = nsisInstaller(v);
  const built = buildSetup({
    CILICA_SETUP_PAYLOAD: installer,
    CILICA_SETUP_APP_BYTES: String(appBytes()),
  });
  mkdirSync(OUT_DIR, { recursive: true });
  const out = join(OUT_DIR, `Cilica_${v}_x64_Setup.exe`);
  copyFileSync(built, out);
  sign(out);
  console.log(`\nCilica Setup: ${out}`);
}

if (process.platform !== "win32") {
  console.error("Cilica Setup is built on Windows. On macOS or Linux, run `cargo run -p cilica-setup` to try its window.");
  process.exit(1);
}

const step = process.argv[2] ?? "all";
switch (step) {
  case "prepare":
    prepare();
    break;
  case "package":
    pack();
    break;
  case "all":
    prepare();
    run("bun", [
      "run",
      "--cwd",
      "apps/desktop",
      "tauri",
      "build",
      "--bundles",
      "nsis",
      "--config",
      "src-tauri/tauri.windows-setup.conf.json",
    ]);
    pack();
    break;
  default:
    console.error(`Unknown step "${step}". Use prepare, package, or nothing for all three.`);
    process.exit(1);
}
