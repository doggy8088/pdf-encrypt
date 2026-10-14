// Pure command-building logic for the pdf-encrypt command builder.
// No DOM access here, so the module can be tested with `node --test`.

export const COMMANDS = ["encrypt", "decrypt", "info", "edit"];
export const SHELLS = ["bash", "powershell"];
export const ALGORITHMS = ["aes-256", "aes-128", "rc4-128", "rc4-40"];
export const PERMISSIONS = [
  "print",
  "high-quality-print",
  "modify",
  "copy",
  "annotate",
  "fill-forms",
  "accessibility",
  "assemble",
];

// Order matches `pdf-encrypt encrypt --help`.
export const METADATA_FIELDS = [
  { key: "title", flag: "--doc-title" },
  { key: "author", flag: "--doc-author" },
  { key: "subject", flag: "--doc-subject" },
  { key: "keywords", flag: "--doc-keywords" },
  { key: "description", flag: "--doc-description" },
  { key: "copyrightStatus", flag: "--doc-copyright-status" },
  { key: "copyrightNotice", flag: "--doc-copyright-notice" },
  { key: "copyrightInfoUrl", flag: "--doc-copyright-info-url" },
];

const ENV_NAMES = {
  open: "PDF_OPEN_PASSWORD",
  owner: "PDF_OWNER_PASSWORD",
  input: "PDF_PASSWORD",
};

const PROMPTS = {
  open: "新的開啟密碼",
  owner: "新的擁有者密碼",
  input: "目前的 PDF 密碼",
};

export function defaultState() {
  return {
    shell: "bash",
    command: "encrypt",
    input: "report.pdf",
    output: "",
    inPlace: false,
    force: false,
    passwordMode: "env",
    useOpenPassword: true,
    openPassword: "",
    useOwnerPassword: false,
    ownerPassword: "",
    useInputPassword: false,
    inputPassword: "",
    algorithm: "aes-256",
    permissionMode: "all",
    allowed: [...PERMISSIONS],
    encryptMetadata: true,
    json: true,
    metadata: Object.fromEntries(METADATA_FIELDS.map(({ key }) => [key, ""])),
    metadataEnabled: Object.fromEntries(METADATA_FIELDS.map(({ key }) => [key, false])),
  };
}

const BASH_SAFE = /^[A-Za-z0-9_@%+=:,./-]+$/;
// PowerShell treats a leading @ as splatting and a comma as an array separator.
const POWERSHELL_SAFE = /^[A-Za-z0-9_%+=:./\\-]+$/;

export function quote(value, shell) {
  const text = String(value);
  if (shell === "powershell") {
    return POWERSHELL_SAFE.test(text) ? text : `'${text.replace(/'/g, "''")}'`;
  }
  return BASH_SAFE.test(text) ? text : `'${text.replace(/'/g, `'\\''`)}'`;
}

function envReference(kind, shell) {
  const name = ENV_NAMES[kind];
  return shell === "powershell" ? `$env:${name}` : `"$${name}"`;
}

function promptLine(kind, shell) {
  const name = ENV_NAMES[kind];
  const label = PROMPTS[kind];
  if (shell === "powershell") {
    return `$env:${name} = Read-Host '${label}' -MaskInput`;
  }
  return `read -rsp '${label}: ' ${name}; echo`;
}

function passwordArg(kind, value, state, prelude) {
  if (state.passwordMode === "env") {
    prelude.push(promptLine(kind, state.shell));
    return envReference(kind, state.shell);
  }
  return quote(value, state.shell);
}

function inputPasswordFlag(command) {
  return command === "encrypt" ? "--input-open-password" : "--password";
}

function writesOutput(command) {
  return command === "encrypt" || command === "decrypt" || command === "edit";
}

function defaultOutputName(input, command) {
  const suffix = { encrypt: "_encrypted", decrypt: "_decrypted", edit: "_edited" }[command];
  const dot = input.lastIndexOf(".");
  const slash = Math.max(input.lastIndexOf("/"), input.lastIndexOf("\\"));
  if (dot > slash + 1) {
    return `${input.slice(0, dot)}${suffix}${input.slice(dot)}`;
  }
  return `${input}${suffix}.pdf`;
}

function permissionArgs(state, args) {
  if (state.permissionMode === "read-only") {
    args.push(["--read-only"]);
    return;
  }
  if (state.permissionMode !== "custom") {
    return;
  }
  const allowed = PERMISSIONS.filter((permission) => state.allowed.includes(permission));
  if (allowed.length === PERMISSIONS.length) {
    return;
  }
  if (allowed.length === 0) {
    args.push(["--deny", "all"]);
    return;
  }
  for (const permission of allowed) {
    args.push(["--allow", permission]);
  }
}

/**
 * Builds the command for the given state.
 * Returns { lines, text, errors, warnings, notes }.
 * `errors` are problems pdf-encrypt itself would reject; `warnings` are
 * valid but risky choices; `notes` explain the result.
 */
export function buildCommand(state) {
  const errors = [];
  const warnings = [];
  const notes = [];
  const prelude = [];
  const args = [];
  const shell = SHELLS.includes(state.shell) ? state.shell : "bash";
  const command = COMMANDS.includes(state.command) ? state.command : "encrypt";
  const input = (state.input ?? "").trim();
  const output = (state.output ?? "").trim();
  const s = { ...state, shell, command };

  if (!input) {
    errors.push("請輸入 PDF 檔案路徑。");
  }
  args.push(["--input", input ? quote(input, shell) : "<input.pdf>"]);

  const canInPlace = command === "encrypt" || command === "edit";
  const inPlace = canInPlace && Boolean(state.inPlace);
  if (writesOutput(command)) {
    if (inPlace) {
      args.push(["--in-place"]);
      notes.push("--in-place 會先寫入暫存檔，成功後才取代原檔。");
    } else if (output) {
      if (input && output === input) {
        errors.push("輸出路徑不可與輸入路徑相同；要覆蓋原檔請改用「原地修改」。");
      }
      args.push(["--output", quote(output, shell)]);
    } else if (input) {
      notes.push(`未指定輸出檔，會寫入 ${defaultOutputName(input, command)}。`);
    }
    if ((command === "encrypt" || command === "edit") && state.force) {
      args.push(["--force"]);
    } else if (inPlace) {
      warnings.push("原地修改時會詢問 [y/N] 確認；在 CI 或 AI 代理等非互動環境請勾選「略過覆蓋確認」。");
    }
  }

  if (state.useInputPassword) {
    if (state.passwordMode === "literal" && !state.inputPassword) {
      errors.push("請輸入目前 PDF 的密碼，或取消「輸入檔已加密」。");
    }
    args.push([inputPasswordFlag(command), passwordArg("input", state.inputPassword, s, prelude)]);
  }

  if (command === "encrypt") {
    const open = Boolean(state.useOpenPassword);
    const owner = Boolean(state.useOwnerPassword);
    if (!open && !owner) {
      errors.push("加密至少需要開啟密碼或擁有者密碼其中之一。");
    }
    if (open) {
      if (state.passwordMode === "literal" && !state.openPassword) {
        errors.push("請輸入開啟密碼。");
      }
      args.push(["--open-password", passwordArg("open", state.openPassword, s, prelude)]);
    }
    if (owner) {
      if (state.passwordMode === "literal" && !state.ownerPassword) {
        errors.push("請輸入擁有者密碼。");
      }
      args.push(["--owner-password", passwordArg("owner", state.ownerPassword, s, prelude)]);
    }

    const algorithm = ALGORITHMS.includes(state.algorithm) ? state.algorithm : "aes-256";
    if (algorithm !== "aes-256") {
      args.push(["--algorithm", algorithm]);
    }
    if (algorithm === "rc4-40" || algorithm === "rc4-128") {
      warnings.push(`${algorithm} 只適合相容舊版 PDF reader，新文件建議使用 aes-256。`);
    }

    permissionArgs(s, args);
    const restricted =
      state.permissionMode === "read-only" ||
      (state.permissionMode === "custom" && state.allowed.length < PERMISSIONS.length);
    if (restricted && open && !owner) {
      warnings.push("只設定開啟密碼時，擁有者密碼會與開啟密碼相同，知道密碼的人仍有完整權限。要讓權限限制生效，請另設擁有者密碼。");
    }
    if (restricted && !open) {
      notes.push("沒有開啟密碼：任何人都能開啟 PDF，但 reader 會套用權限限制。");
    }
    if (!state.encryptMetadata) {
      args.push(["--no-encrypt-metadata"]);
    }
  }

  if (command === "encrypt" || command === "edit") {
    let count = 0;
    for (const { key, flag } of METADATA_FIELDS) {
      if (state.metadataEnabled?.[key]) {
        count += 1;
        args.push([flag, quote(state.metadata?.[key] ?? "", shell)]);
      }
    }
    if (command === "edit" && count === 0) {
      errors.push("編輯文件屬性至少要勾選一個欄位。");
    }
  }

  if (command === "info" && state.json) {
    args.push(["--json"]);
  }

  if (state.passwordMode === "literal" && args.some(([flag]) => /password/.test(flag))) {
    warnings.push("密碼會以明文留在 shell 歷史紀錄中；建議改用「環境變數」模式。");
  }
  if (state.passwordMode === "env" && shell === "powershell" && prelude.length > 0) {
    notes.push("Read-Host -MaskInput 需要 PowerShell 7.1 以上版本。");
  }

  const main = ["pdf-encrypt", command, ...args.map((parts) => parts.join(" "))];
  const continuation = shell === "powershell" ? " `" : " \\";
  const commandLines =
    main.join(" ").length <= 80
      ? [main.join(" ")]
      : [
          `${main[0]} ${main[1]}${continuation}`,
          ...main.slice(2).map((part, index, rest) => `  ${part}${index < rest.length - 1 ? continuation : ""}`),
        ];
  const lines = [...prelude, ...commandLines];

  return { lines, text: lines.join("\n"), errors, warnings, notes };
}
