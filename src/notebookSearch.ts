import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles.css";

interface NotebookPageSearchResult {
  pageId: string;
  notebookId: string;
  title: string;
  snippet: string;
}

const currentWindow = getCurrentWindow();
let searchTimer = 0;
let isComposing = false;
let results: NotebookPageSearchResult[] = [];
let selectedIndex = 0;

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <main class="notebook-search-shell">
    <section id="notebookSearch" class="notebook-search" aria-label="搜索 Folia">
      <input id="notebookSearchInput" type="search" placeholder="搜索 Folia page" autocomplete="off" />
      <div id="notebookSearchResults" class="notebook-search-results"></div>
    </section>
  </main>
`;

const input = document.querySelector<HTMLInputElement>("#notebookSearchInput")!;
const resultList = document.querySelector<HTMLElement>("#notebookSearchResults")!;

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#039;",
  }[char] || char));
}

function stripHtml(value: string) {
  const container = document.createElement("div");
  container.innerHTML = value;
  return (container.textContent || "").replace(/\s+/g, " ").trim();
}

function renderResults() {
  resultList.innerHTML = "";
  results.forEach((result, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "notebook-result";
    button.classList.toggle("is-selected", index === selectedIndex);
    button.innerHTML = `
      <span>${escapeHtml(result.title || "Untitled")}</span>
      <small>${escapeHtml(stripHtml(result.snippet || ""))}</small>
    `;
    button.addEventListener("click", () => {
      selectedIndex = index;
      void openSelectedResult();
    });
    resultList.appendChild(button);
  });
}

async function searchNotebookPages(query: string) {
  const trimmed = query.trim();
  if (!trimmed) {
    results = [];
    selectedIndex = 0;
    renderResults();
    return;
  }
  try {
    results = await invoke<NotebookPageSearchResult[]>("search_notebook_pages", { query: trimmed, limit: 8 });
    selectedIndex = 0;
    renderResults();
  } catch (error) {
    results = [];
    selectedIndex = 0;
    resultList.innerHTML = `<p>${escapeHtml(String(error))}</p>`;
  }
}

async function openSelectedResult() {
  const result = results[selectedIndex];
  if (!result) return;
  try {
    await invoke("open_notebook_card", { pageId: result.pageId });
    await currentWindow.close();
  } catch (error) {
    resultList.innerHTML = `<p>${escapeHtml(String(error))}</p>`;
  }
}

function scheduleSearch() {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    void searchNotebookPages(input.value);
  }, 140);
}

input.addEventListener("compositionstart", () => {
  isComposing = true;
});

input.addEventListener("compositionend", () => {
  isComposing = false;
  scheduleSearch();
});

input.addEventListener("input", (event) => {
  if (isComposing || (event instanceof InputEvent && event.isComposing)) return;
  scheduleSearch();
});

input.addEventListener("keydown", async (event) => {
  if (isComposing || event.isComposing || event.key === "Process") return;
  if (event.key === "Escape") {
    event.preventDefault();
    await currentWindow.close();
    return;
  }
  if (event.key === "ArrowDown") {
    event.preventDefault();
    selectedIndex = Math.min(results.length - 1, selectedIndex + 1);
    renderResults();
    return;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    selectedIndex = Math.max(0, selectedIndex - 1);
    renderResults();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    await openSelectedResult();
  }
});

window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    void currentWindow.close();
  }
});

requestAnimationFrame(() => {
  input.focus({ preventScroll: true });
});
