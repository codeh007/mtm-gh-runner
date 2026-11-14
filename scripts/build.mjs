import { execFileSync, spawnSync } from "node:child_process";
import { chmod, copyFile, mkdir, readFile, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(
  await readFile(join(packageRoot, "package.json"), "utf8"),
);
const binary = join(packageRoot, "dist", "mtm-gh-runner");
const notices = join(packageRoot, "dist", "THIRD_PARTY_LICENSES.json");

const cargoOptions = {
  cwd: packageRoot,
  encoding: "utf8",
  maxBuffer: 16 * 1024 * 1024,
  stdio: ["ignore", "pipe", "inherit"],
};

async function validate() {
  const binaryInfo = await stat(binary);
  if (
    !binaryInfo.isFile() ||
    binaryInfo.size === 0 ||
    (binaryInfo.mode & 0o111) === 0
  ) {
    throw new Error("dist/mtm-gh-runner must be a non-empty executable file");
  }
  const version = spawnSync(binary, ["--version"], { encoding: "utf8" });
  if (version.error) throw version.error;
  if (
    version.status !== 0 ||
    version.stdout.trim() !== `mtm-gh-runner ${manifest.version}`
  ) {
    throw new Error(
      `dist/mtm-gh-runner version does not match package.json ${manifest.version}`,
    );
  }
  const noticeInfo = await stat(notices);
  if (!noticeInfo.isFile() || noticeInfo.size === 0) {
    throw new Error("dist/THIRD_PARTY_LICENSES.json must be a non-empty file");
  }
  const licenseData = JSON.parse(await readFile(notices, "utf8"));
  if (
    licenseData === null ||
    typeof licenseData !== "object" ||
    !Array.isArray(licenseData.licenses) ||
    licenseData.licenses.length === 0 ||
    licenseData.licenses.some(
      (license) =>
        typeof license?.name !== "string" ||
        typeof license?.text !== "string" ||
        !license.text,
    )
  ) {
    throw new Error(
      "dist/THIRD_PARTY_LICENSES.json must contain license records",
    );
  }
}

async function build() {
  const messages = execFileSync(
    "cargo",
    [
      "build",
      "--release",
      "--locked",
      "--message-format=json-render-diagnostics",
    ],
    cargoOptions,
  )
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  const artifacts = messages.filter(
    (message) =>
      message.reason === "compiler-artifact" &&
      message.target?.kind?.includes("bin") &&
      message.target.src_path === join(packageRoot, "src", "main.rs") &&
      typeof message.executable === "string",
  );
  if (artifacts.length !== 1)
    throw new Error(
      `expected one mtm-gh-runner compiler artifact, got ${artifacts.length}`,
    );
  await mkdir(join(packageRoot, "dist"), { recursive: true });
  await copyFile(artifacts[0].executable, binary);
  await chmod(binary, 0o755);
  execFileSync(
    "cargo",
    [
      "about",
      "generate",
      "--locked",
      "--fail",
      "--format",
      "json",
      "-o",
      notices,
    ],
    cargoOptions,
  );
  await validate();
}

if (process.argv[2] === "build") await build();
else if (process.argv[2] === "prepack") await validate();
else throw new Error("expected build or prepack");
