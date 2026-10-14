#!/usr/bin/env node
"use strict";

// Launcher for the native pdf-encrypt executable. npm installs only the
// platform package that matches this machine (see optionalDependencies).

const { spawnSync } = require("node:child_process");
const path = require("node:path");

const PLATFORMS = {
  "darwin-arm64": "@willh/pdf-encrypt-darwin-arm64",
  "darwin-x64": "@willh/pdf-encrypt-darwin-x64",
  "linux-arm64": "@willh/pdf-encrypt-linux-arm64",
  "linux-x64": "@willh/pdf-encrypt-linux-x64",
  "win32-arm64": "@willh/pdf-encrypt-win32-arm64",
  "win32-x64": "@willh/pdf-encrypt-win32-x64",
};

function resolveBinary() {
  if (process.env.PDF_ENCRYPT_BINARY) {
    return process.env.PDF_ENCRYPT_BINARY;
  }

  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORMS[key];
  if (!pkg) {
    throw new Error(
      `pdf-encrypt does not provide a prebuilt binary for ${key}. ` +
        `Supported platforms: ${Object.keys(PLATFORMS).join(", ")}.`,
    );
  }

  const exe = process.platform === "win32" ? "pdf-encrypt.exe" : "pdf-encrypt";
  try {
    return require.resolve(`${pkg}/bin/${exe}`);
  } catch {
    throw new Error(
      `The platform package ${pkg} is not installed. ` +
        `Reinstall without --omit=optional / --no-optional, e.g. npm install -g @willh/pdf-encrypt.`,
    );
  }
}

let binary;
try {
  binary = resolveBinary();
} catch (error) {
  console.error(`pdf-encrypt: ${error.message}`);
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), {
  stdio: "inherit",
  windowsHide: false,
});

if (result.error) {
  console.error(`pdf-encrypt: failed to run ${path.basename(binary)}: ${result.error.message}`);
  process.exit(1);
}

if (result.signal) {
  process.kill(process.pid, result.signal);
} else {
  process.exit(result.status ?? 1);
}
