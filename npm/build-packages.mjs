#!/usr/bin/env node
// Builds the npm packages for a pdf-encrypt release.
//
//   node npm/build-packages.mjs <version> <binaries-dir> <out-dir>
//
// <binaries-dir> must contain the raw release executables, for example
// pdf-encrypt-aarch64-apple-darwin and pdf-encrypt-x86_64-pc-windows-msvc.exe.
// The script writes one directory per package into <out-dir> and prints the
// package directories in publish order (platform packages first).

import { chmodSync, copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SCOPE = "@willh";
const NAME = "pdf-encrypt";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

// Linux uses the statically linked musl builds so one package runs on glibc and musl.
const PLATFORMS = [
  { target: "aarch64-apple-darwin", os: "darwin", cpu: "arm64" },
  { target: "x86_64-apple-darwin", os: "darwin", cpu: "x64" },
  { target: "aarch64-unknown-linux-musl", os: "linux", cpu: "arm64" },
  { target: "x86_64-unknown-linux-musl", os: "linux", cpu: "x64" },
  { target: "aarch64-pc-windows-msvc", os: "win32", cpu: "arm64" },
  { target: "x86_64-pc-windows-msvc", os: "win32", cpu: "x64" },
];

const COMMON = {
  description: "Encrypt, decrypt, read, and edit PDF document properties from the command line.",
  license: "MIT",
  author: "Will 保哥 <doggy.huang@gmail.com>",
  homepage: "https://github.com/doggy8088/pdf-encrypt",
  // Trusted publishing and provenance require this to match the GitHub repository.
  repository: { type: "git", url: "git+https://github.com/doggy8088/pdf-encrypt.git" },
  bugs: { url: "https://github.com/doggy8088/pdf-encrypt/issues" },
};

const [version, binariesDir, outDir] = process.argv.slice(2);
if (!version || !binariesDir || !outDir) {
  console.error("usage: node npm/build-packages.mjs <version> <binaries-dir> <out-dir>");
  process.exit(2);
}
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`invalid version: ${version}`);
  process.exit(2);
}

const writeJson = (file, value) => writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);

rmSync(outDir, { recursive: true, force: true });
const packageDirs = [];
const optionalDependencies = {};

for (const { target, os, cpu } of PLATFORMS) {
  const name = `${SCOPE}/${NAME}-${os}-${cpu}`;
  const exe = os === "win32" ? `${NAME}.exe` : NAME;
  const source = join(binariesDir, os === "win32" ? `${NAME}-${target}.exe` : `${NAME}-${target}`);
  if (!existsSync(source)) {
    console.error(`missing release executable: ${source}`);
    process.exit(1);
  }

  const dir = join(outDir, `${NAME}-${os}-${cpu}`);
  mkdirSync(join(dir, "bin"), { recursive: true });
  copyFileSync(source, join(dir, "bin", exe));
  chmodSync(join(dir, "bin", exe), 0o755);
  copyFileSync(join(ROOT, "LICENSE"), join(dir, "LICENSE"));
  writeFileSync(
    join(dir, "README.md"),
    `# ${name}\n\nThe ${os}-${cpu} binary (\`${target}\`) for [${SCOPE}/${NAME}](https://www.npmjs.com/package/${SCOPE}/${NAME}). Install \`${SCOPE}/${NAME}\` instead of this package.\n`,
  );
  writeJson(join(dir, "package.json"), {
    name,
    version,
    ...COMMON,
    description: `The ${os}-${cpu} binary for ${SCOPE}/${NAME}.`,
    os: [os],
    cpu: [cpu],
    files: ["bin/", "LICENSE", "README.md"],
    preferUnplugged: true,
  });

  optionalDependencies[name] = version;
  packageDirs.push(dir);
}

const mainDir = join(outDir, NAME);
mkdirSync(join(mainDir, "bin"), { recursive: true });
copyFileSync(join(ROOT, "npm", NAME, "bin", `${NAME}.js`), join(mainDir, "bin", `${NAME}.js`));
chmodSync(join(mainDir, "bin", `${NAME}.js`), 0o755);
copyFileSync(join(ROOT, "npm", NAME, "README.md"), join(mainDir, "README.md"));
copyFileSync(join(ROOT, "LICENSE"), join(mainDir, "LICENSE"));
writeJson(join(mainDir, "package.json"), {
  name: `${SCOPE}/${NAME}`,
  version,
  ...COMMON,
  keywords: ["pdf", "encryption", "decrypt", "password", "cli"],
  bin: { [NAME]: `bin/${NAME}.js` },
  files: ["bin/", "LICENSE", "README.md"],
  engines: { node: ">=18" },
  optionalDependencies,
});
packageDirs.push(mainDir);

console.log(packageDirs.join("\n"));
