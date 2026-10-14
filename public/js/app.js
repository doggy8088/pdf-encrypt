import { buildCommand, defaultState } from "./command-builder.js";

const root = document.documentElement;
const THEME_KEY = "pdf-encrypt-theme";
const SHELL_KEY = "pdf-encrypt-shell";

function readStorage(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStorage(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage can be unavailable (private mode, blocked site data); the choice still applies to this visit.
  }
}

/* ---------- Toast ---------- */

const toast = document.getElementById("toast");
let toastTimer;
function showToast(message) {
  toast.textContent = message;
  toast.classList.add("is-visible");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove("is-visible"), 2400);
}

/* ---------- Theme ---------- */

const themeToggle = document.getElementById("theme-toggle");
function renderThemeToggle() {
  const dark = root.getAttribute("data-theme") === "dark";
  themeToggle.setAttribute("aria-pressed", String(dark));
  themeToggle.title = dark ? "深色主題：開啟" : "深色主題：關閉";
}
themeToggle.addEventListener("click", () => {
  const next = root.getAttribute("data-theme") === "dark" ? "light" : "dark";
  root.setAttribute("data-theme", next);
  writeStorage(THEME_KEY, next);
  renderThemeToggle();
});
window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (event) => {
  const stored = readStorage(THEME_KEY);
  if (stored !== "light" && stored !== "dark") {
    root.setAttribute("data-theme", event.matches ? "dark" : "light");
    renderThemeToggle();
  }
});
renderThemeToggle();

/* ---------- Copy ---------- */

function fallbackCopy(text) {
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.append(area);
  area.select();
  let ok = false;
  try {
    ok = document.execCommand("copy");
  } catch (error) {
    console.warn("pdf-encrypt: execCommand copy failed", error);
  }
  area.remove();
  return ok;
}

function selectContents(element) {
  const range = document.createRange();
  range.selectNodeContents(element);
  const selection = window.getSelection();
  selection.removeAllRanges();
  selection.addRange(range);
}

async function copyText(text, sourceElement) {
  if (navigator.clipboard && window.isSecureContext) {
    try {
      await navigator.clipboard.writeText(text);
      showToast("已複製到剪貼簿");
      return;
    } catch (error) {
      console.warn("pdf-encrypt: Clipboard API failed, trying fallback", error);
    }
  }
  if (fallbackCopy(text)) {
    showToast("已複製到剪貼簿");
    return;
  }
  if (sourceElement) {
    selectContents(sourceElement);
  }
  showToast("無法自動複製，已選取文字，請按 Ctrl+C 或 ⌘C");
}

document.addEventListener("click", (event) => {
  const button = event.target.closest("[data-copy]");
  if (button) {
    copyText(button.dataset.copy, button.parentElement.querySelector("code"));
  }
});

/* ---------- Tabs ---------- */

for (const tabs of document.querySelectorAll("[data-tabs]")) {
  const list = [...tabs.querySelectorAll('[role="tab"]')];
  const select = (tab, focus) => {
    for (const other of list) {
      const selected = other === tab;
      other.setAttribute("aria-selected", String(selected));
      other.tabIndex = selected ? 0 : -1;
      document.getElementById(other.getAttribute("aria-controls")).hidden = !selected;
    }
    if (focus) tab.focus();
  };
  list.forEach((tab, index) => {
    tab.addEventListener("click", () => select(tab, false));
    tab.addEventListener("keydown", (event) => {
      const moves = { ArrowRight: 1, ArrowLeft: -1, Home: -index, End: list.length - 1 - index };
      if (event.key in moves) {
        event.preventDefault();
        select(list[(index + moves[event.key] + list.length) % list.length], true);
      }
    });
  });
}

/* ---------- Command builder ---------- */

const form = document.getElementById("builder-form");
const output = document.querySelector("#command-output code");
const messages = document.getElementById("command-messages");
const inputPasswordLabel = document.getElementById("input-password-label");
let current = defaultState();

const storedShell = readStorage(SHELL_KEY);
if (storedShell === "bash" || storedShell === "powershell") {
  current.shell = storedShell;
}

function readForm() {
  const data = new FormData(form);
  const state = defaultState();
  for (const key of ["command", "shell", "passwordMode", "algorithm", "permissionMode"]) {
    state[key] = data.get(key);
  }
  // Read text fields from the elements: FormData skips disabled fields, and a
  // field can still be disabled from the previous render when a preset is applied.
  for (const key of ["input", "output", "openPassword", "ownerPassword", "inputPassword"]) {
    state[key] = form.elements[key].value;
  }
  for (const key of ["inPlace", "force", "useOpenPassword", "useOwnerPassword", "useInputPassword", "encryptMetadata", "json"]) {
    state[key] = data.get(key) === "on";
  }
  state.allowed = data.getAll("allowed");
  const enabled = new Set(data.getAll("meta-on"));
  for (const field of form.querySelectorAll('input[name="meta"]')) {
    state.metadata[field.dataset.key] = field.value;
    state.metadataEnabled[field.dataset.key] = enabled.has(field.dataset.key);
  }
  return state;
}

function writeForm(state) {
  for (const key of ["command", "shell", "passwordMode", "permissionMode"]) {
    const radio = form.querySelector(`input[name="${key}"][value="${state[key]}"]`);
    if (radio) radio.checked = true;
  }
  form.elements.algorithm.value = state.algorithm;
  for (const key of ["input", "output", "openPassword", "ownerPassword", "inputPassword"]) {
    form.elements[key].value = state[key];
  }
  for (const key of ["inPlace", "force", "useOpenPassword", "useOwnerPassword", "useInputPassword", "encryptMetadata", "json"]) {
    form.elements[key].checked = Boolean(state[key]);
  }
  for (const box of form.querySelectorAll('input[name="allowed"]')) {
    box.checked = state.allowed.includes(box.value);
  }
  for (const box of form.querySelectorAll('input[name="meta-on"]')) {
    box.checked = Boolean(state.metadataEnabled[box.value]);
  }
  for (const field of form.querySelectorAll('input[name="meta"]')) {
    field.value = state.metadata[field.dataset.key] ?? "";
  }
}

function syncVisibility(state) {
  for (const element of form.querySelectorAll("[data-show-for]")) {
    element.hidden = !element.dataset.showFor.split(" ").includes(state.command);
  }
  for (const element of form.querySelectorAll("[data-show-mode]")) {
    element.hidden = element.dataset.showMode !== state.passwordMode;
  }
  for (const element of form.querySelectorAll("[data-show-permission]")) {
    element.hidden = element.dataset.showPermission !== state.permissionMode;
  }
  inputPasswordLabel.textContent = state.command === "encrypt" ? "輸入檔已加密（重新加密）" : "輸入檔已加密";
  form.elements.output.disabled = state.inPlace && (state.command === "encrypt" || state.command === "edit");
  for (const [name, toggle] of [
    ["openPassword", "useOpenPassword"],
    ["ownerPassword", "useOwnerPassword"],
    ["inputPassword", "useInputPassword"],
  ]) {
    form.elements[name].disabled = !state[toggle];
  }
  for (const field of form.querySelectorAll('input[name="meta"]')) {
    field.disabled = !state.metadataEnabled[field.dataset.key];
  }
}

function renderMessages(result) {
  messages.replaceChildren();
  const groups = [
    ["is-error", "錯誤", result.errors],
    ["is-warning", "注意", result.warnings],
    ["is-note", "說明", result.notes],
  ];
  for (const [className, label, items] of groups) {
    for (const text of items) {
      const item = document.createElement("li");
      item.className = className;
      const strong = document.createElement("strong");
      strong.textContent = label;
      const span = document.createElement("span");
      span.textContent = text;
      item.append(strong, span);
      messages.append(item);
    }
  }
}

function render() {
  current = readForm();
  syncVisibility(current);
  const result = buildCommand(current);
  output.textContent = result.text;
  renderMessages(result);
  return result;
}

form.addEventListener("input", render);
form.addEventListener("change", (event) => {
  if (event.target.name === "shell") {
    writeStorage(SHELL_KEY, event.target.value);
  }
  render();
});
form.addEventListener("submit", (event) => event.preventDefault());

document.getElementById("copy-command").addEventListener("click", () => {
  const result = render();
  if (result.errors.length > 0) {
    showToast(`指令還不完整：${result.errors[0]}`);
    return;
  }
  copyText(result.text, document.getElementById("command-output"));
});

const SCENARIOS = {
  protect: {
    fact: "原檔保留，另存為 report_encrypted.pdf；擁有者密碼自動與開啟密碼相同。",
    state: {},
  },
  readonly: {
    fact: "任何人都能開啟，但不能修改內容或組合頁面；擁有者密碼保有完整權限。",
    state: { output: "report_readonly.pdf", useOpenPassword: false, useOwnerPassword: true, permissionMode: "read-only" },
  },
  decrypt: {
    fact: "寫出不含加密的一般 PDF；只有權限限制、沒有開啟密碼的檔案可以省略密碼。",
    state: { command: "decrypt", input: "report_encrypted.pdf", useInputPassword: true },
  },
  info: {
    fact: "唯讀查詢，不改動檔案；JSON 可直接交給 AI 代理或其他程式使用。",
    state: { command: "info", input: "annual_encrypted.pdf", useInputPassword: true, json: true },
  },
  rekey: {
    fact: "以舊密碼開啟後重新加密，可以同時更換密碼與演算法。",
    state: { input: "report_encrypted.pdf", output: "report_rekeyed.pdf", useInputPassword: true, algorithm: "aes-128" },
  },
};

function applyPreset(name) {
  const preset = SCENARIOS[name];
  if (!preset) return;
  writeForm({ ...defaultState(), shell: current.shell, ...preset.state });
  render();
}

writeForm(current);
render();

/* ---------- Scenario index with preview ---------- */

const scenarioButtons = [...document.querySelectorAll(".scenario")];
const terminals = [...document.querySelectorAll(".terminal")];
const fact = document.getElementById("scenario-fact");
let selectedScenario = "protect";

function showScenario(name) {
  for (const terminal of terminals) {
    terminal.classList.toggle("is-on", terminal.dataset.for === name);
  }
  fact.textContent = SCENARIOS[name].fact;
}

for (const button of scenarioButtons) {
  const name = button.dataset.scenario;
  button.addEventListener("click", () => {
    selectedScenario = name;
    for (const other of scenarioButtons) {
      other.setAttribute("aria-pressed", String(other === button));
    }
    showScenario(name);
  });
  button.addEventListener("pointerenter", (event) => {
    if (event.pointerType === "mouse") {
      button.classList.add("is-preview");
      showScenario(name);
    }
  });
  button.addEventListener("pointerleave", (event) => {
    if (event.pointerType === "mouse") {
      button.classList.remove("is-preview");
      showScenario(selectedScenario);
    }
  });
}

document.getElementById("scenario-to-builder").addEventListener("click", () => {
  applyPreset(selectedScenario);
  document.getElementById("builder").scrollIntoView();
  document.querySelector('#builder-form input[name="command"]:checked').focus({ preventScroll: true });
});

/* ---------- FAQ cards ---------- */

for (const cards of document.querySelectorAll("[data-cards]")) {
  const items = [...cards.querySelectorAll(".card")];
  for (const card of items) {
    const toggle = card.querySelector(".card-toggle");
    toggle.addEventListener("click", () => {
      const opening = !card.classList.contains("is-open");
      for (const other of items) {
        const open = other === card && opening;
        other.classList.toggle("is-open", open);
        other.querySelector(".card-toggle").setAttribute("aria-expanded", String(open));
      }
    });
  }
}

/* ---------- Phone nav toggle ---------- */

const topbar = document.querySelector(".topbar");
const navToggle = document.getElementById("nav-toggle");
function setNavOpen(open) {
  topbar.classList.toggle("is-open", open);
  navToggle.setAttribute("aria-expanded", String(open));
  navToggle.textContent = open ? "關閉" : "目錄";
}
navToggle.addEventListener("click", () => setNavOpen(!topbar.classList.contains("is-open")));
document.querySelector(".nav-list").addEventListener("click", (event) => {
  if (event.target.closest("a")) setNavOpen(false);
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && topbar.classList.contains("is-open")) {
    setNavOpen(false);
    navToggle.focus();
  }
});

/* ---------- Current section in the top bar ---------- */

const navLinks = [...document.querySelectorAll(".nav-list a")];
const navTargets = navLinks.map((link) => document.querySelector(link.getAttribute("href"))).filter(Boolean);

function markCurrent(id) {
  for (const link of navLinks) {
    if (link.getAttribute("href") === `#${id}`) {
      link.setAttribute("aria-current", "location");
      // Keep the current item visible in the sideways-scrolling nav row on narrow screens.
      const nav = link.closest(".site-nav");
      if (nav.scrollWidth > nav.clientWidth) {
        const offset = link.getBoundingClientRect().left - nav.getBoundingClientRect().left;
        nav.scrollTo({ left: nav.scrollLeft + offset - 16, behavior: "smooth" });
      }
    } else {
      link.removeAttribute("aria-current");
    }
  }
}

if ("IntersectionObserver" in window) {
  const visible = new Set();
  const sectionObserver = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) visible.add(entry.target);
        else visible.delete(entry.target);
      }
      const current = navTargets.find((target) => visible.has(target));
      markCurrent(current ? current.id : "");
    },
    { rootMargin: "-35% 0px -60% 0px" },
  );
  navTargets.forEach((target) => sectionObserver.observe(target));
}

/* ---------- Entrances ---------- */

const reveals = [...document.querySelectorAll(".reveal")];
const groups = new Map();
for (const element of reveals) {
  const section = element.closest("section, .hero-field, footer") ?? document.body;
  const index = groups.get(section) ?? 0;
  element.style.setProperty("--delay", `${Math.min(index, 8) * 70}ms`);
  groups.set(section, index + 1);
}

if ("IntersectionObserver" in window) {
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          entry.target.classList.add("is-in");
          observer.unobserve(entry.target);
        }
      }
    },
    { rootMargin: "0px 0px -8% 0px" },
  );
  reveals.forEach((element) => observer.observe(element));
} else {
  reveals.forEach((element) => element.classList.add("is-in"));
}

root.classList.add("app-ready");
