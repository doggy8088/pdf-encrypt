// Tests for public/js/command-builder.js.
//
//   node --test tests/site/
//
// When PDF_ENCRYPT_BIN points to a built binary (for example
// target/release/pdf-encrypt), the generated bash commands are also run
// against a real PDF.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { buildCommand, defaultState, quote } from "../../public/js/command-builder.js";

const state = (overrides) => ({ ...defaultState(), ...overrides });

test("quote leaves safe tokens alone", () => {
  assert.equal(quote("report.pdf", "bash"), "report.pdf");
  assert.equal(quote("C:\\docs\\a.pdf", "powershell"), "C:\\docs\\a.pdf");
});

test("quote escapes single quotes and spaces per shell", () => {
  assert.equal(quote("it's here.pdf", "bash"), `'it'\\''s here.pdf'`);
  assert.equal(quote("it's here.pdf", "powershell"), `'it''s here.pdf'`);
  assert.equal(quote("報告.pdf", "bash"), `'報告.pdf'`);
});

test("quote protects PowerShell splatting and array syntax", () => {
  assert.equal(quote("@team", "powershell"), `'@team'`);
  assert.equal(quote("a,b", "powershell"), `'a,b'`);
  assert.equal(quote("a,b", "bash"), "a,b");
});

test("default encrypt uses an environment variable prompt", () => {
  const result = buildCommand(defaultState());
  assert.deepEqual(result.errors, []);
  assert.deepEqual(result.lines, [
    "read -rsp '新的開啟密碼: ' PDF_OPEN_PASSWORD; echo",
    `pdf-encrypt encrypt --input report.pdf --open-password "$PDF_OPEN_PASSWORD"`,
  ]);
  assert.match(result.notes.join(), /report_encrypted\.pdf/);
});

test("PowerShell uses $env: and Read-Host", () => {
  const result = buildCommand(state({ shell: "powershell" }));
  assert.equal(result.lines[0], "$env:PDF_OPEN_PASSWORD = Read-Host '新的開啟密碼' -MaskInput");
  assert.equal(result.lines[1], "pdf-encrypt encrypt --input report.pdf --open-password $env:PDF_OPEN_PASSWORD");
});

test("encrypt requires at least one password", () => {
  const result = buildCommand(state({ useOpenPassword: false }));
  assert.match(result.errors.join(), /至少需要開啟密碼或擁有者密碼/);
});

test("read-only with only an open password warns", () => {
  const result = buildCommand(state({ permissionMode: "read-only" }));
  assert.match(result.text, /--read-only/);
  assert.match(result.warnings.join(), /擁有者密碼會與開啟密碼相同/);
});

test("custom permissions become --allow flags, none becomes --deny all", () => {
  const some = buildCommand(state({ permissionMode: "custom", allowed: ["print", "copy"] }));
  assert.match(some.text, /--allow print/);
  assert.match(some.text, /--allow copy/);
  const none = buildCommand(state({ permissionMode: "custom", allowed: [] }));
  assert.match(none.text, /--deny all/);
});

test("in-place without force warns about the interactive prompt", () => {
  const result = buildCommand(state({ inPlace: true, output: "ignored.pdf" }));
  assert.match(result.text, /--in-place/);
  assert.doesNotMatch(result.text, /--output/);
  assert.match(result.warnings.join(), /\[y\/N\]/);
});

test("output equal to input is rejected", () => {
  const result = buildCommand(state({ output: "report.pdf" }));
  assert.match(result.errors.join(), /不可與輸入路徑相同/);
});

test("literal passwords are quoted and warn about history", () => {
  const result = buildCommand(state({ passwordMode: "literal", openPassword: "p@ss word" }));
  assert.match(result.text, /--open-password 'p@ss word'/);
  assert.match(result.warnings.join(), /歷史紀錄/);
});

test("edit requires a metadata field", () => {
  const result = buildCommand(state({ command: "edit" }));
  assert.match(result.errors.join(), /至少要勾選一個欄位/);
  const ok = buildCommand(
    state({
      command: "edit",
      metadataEnabled: { ...defaultState().metadataEnabled, title: true, keywords: true },
      metadata: { ...defaultState().metadata, title: "年度報告", keywords: "" },
    }),
  );
  assert.deepEqual(ok.errors, []);
  assert.match(ok.text, /--doc-title '年度報告'/);
  assert.match(ok.text, /--doc-keywords ''/);
});

test("decrypt and info use --password for the input file", () => {
  const decrypt = buildCommand(state({ command: "decrypt", useInputPassword: true }));
  assert.match(decrypt.text, /--password "\$PDF_PASSWORD"/);
  const info = buildCommand(state({ command: "info" }));
  assert.match(info.text, /--json/);
  assert.doesNotMatch(info.text, /--output/);
});

test("long commands wrap with the shell's continuation character", () => {
  const long = state({
    passwordMode: "literal",
    openPassword: "secret",
    useOwnerPassword: true,
    ownerPassword: "owner-secret",
    algorithm: "aes-128",
    permissionMode: "read-only",
  });
  const bash = buildCommand(long);
  assert.ok(bash.lines.length > 1);
  assert.ok(bash.lines.slice(0, -1).every((line) => line.endsWith(" \\")));
  const ps = buildCommand({ ...long, shell: "powershell" });
  assert.ok(ps.lines.slice(0, -1).every((line) => line.endsWith(" `")));
});

const bin = process.env.PDF_ENCRYPT_BIN;
const fixture = "test-output/real_decrypted.pdf";

test("generated bash commands run against the real binary", { skip: !bin || !existsSync(fixture) }, () => {
  const dir = mkdtempSync(join(tmpdir(), "pdf-encrypt-site-"));
  const input = join(dir, "my report.pdf");
  copyFileSync(fixture, input);
  const run = (overrides) => {
    const { text, errors } = buildCommand(state({ passwordMode: "literal", ...overrides }));
    assert.deepEqual(errors, []);
    execFileSync("bash", ["-c", text.replace(/^pdf-encrypt /m, `"${bin}" `)], { cwd: dir, stdio: "pipe" });
  };

  run({
    input,
    output: join(dir, "sealed.pdf"),
    openPassword: "it's secret",
    useOwnerPassword: true,
    ownerPassword: "owner pw",
    permissionMode: "custom",
    allowed: ["print", "copy"],
    metadataEnabled: { ...defaultState().metadataEnabled, title: true },
    metadata: { ...defaultState().metadata, title: "季度報告" },
  });

  const { text } = buildCommand(
    state({ command: "info", input: join(dir, "sealed.pdf"), passwordMode: "literal", useInputPassword: true, inputPassword: "it's secret" }),
  );
  const info = JSON.parse(execFileSync("bash", ["-c", text.replace(/^pdf-encrypt /m, `"${bin}" `)], { encoding: "utf8" }));
  assert.equal(JSON.stringify(info).includes("季度報告"), true);

  run({ command: "decrypt", input: join(dir, "sealed.pdf"), output: join(dir, "open.pdf"), useInputPassword: true, inputPassword: "it's secret" });
  assert.ok(existsSync(join(dir, "open.pdf")));
});
