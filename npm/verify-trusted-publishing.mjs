#!/usr/bin/env node
// Verifies npm trusted publishing from GitHub Actions without publishing.
//
// For each package, the script requests a GitHub OIDC token and exchanges it
// for a short-lived npm publish token, the same exchange `npm publish` does.
// The npm token is discarded. The job needs `id-token: write`.

const PACKAGES = [
  "@willh/pdf-encrypt-darwin-arm64",
  "@willh/pdf-encrypt-darwin-x64",
  "@willh/pdf-encrypt-linux-arm64",
  "@willh/pdf-encrypt-linux-x64",
  "@willh/pdf-encrypt-win32-arm64",
  "@willh/pdf-encrypt-win32-x64",
  "@willh/pdf-encrypt",
];
const REGISTRY = "https://registry.npmjs.org";

const { ACTIONS_ID_TOKEN_REQUEST_URL: requestUrl, ACTIONS_ID_TOKEN_REQUEST_TOKEN: requestToken } = process.env;
if (!requestUrl || !requestToken) {
  console.error("GitHub OIDC is not available. Run this in GitHub Actions with `id-token: write`.");
  process.exit(1);
}

const url = new URL(requestUrl);
url.searchParams.set("audience", `npm:${new URL(REGISTRY).hostname}`);
const oidc = await fetch(url, { headers: { Authorization: `Bearer ${requestToken}` } });
if (!oidc.ok) {
  console.error(`Failed to get a GitHub OIDC token: HTTP ${oidc.status}`);
  process.exit(1);
}
const { value: idToken } = await oidc.json();

let failed = 0;
for (const name of PACKAGES) {
  const escaped = name.replace("/", "%2f");
  const response = await fetch(`${REGISTRY}/-/npm/v1/oidc/token/exchange/package/${escaped}`, {
    method: "POST",
    headers: { Authorization: `Bearer ${idToken}` },
  });
  if (response.ok) {
    console.log(`ok    ${name}`);
  } else {
    failed += 1;
    const body = (await response.text()).slice(0, 300);
    console.log(`FAIL  ${name}: HTTP ${response.status} ${body}`);
  }
}

if (failed > 0) {
  console.error(`${failed} package(s) failed trusted publishing authentication.`);
  process.exit(1);
}
console.log("Trusted publishing authentication succeeded; nothing was published.");
