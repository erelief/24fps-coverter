import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

// ---- State ----
let isProcessing = false;
let contextMenuRegistered = false;

// File queue: when files arrive while already processing, they wait here
let fileQueue = [];

// Cross-batch progress tracking
let totalFiles = 0;       // total files across all batches (initial + queued)
let completedFiles = 0;   // files completed so far

// ---- DOM refs ----
const app = document.getElementById("app");

// ---- Build UI ----
async function buildUI() {
  app.innerHTML = `
    <div class="drop-zone" id="dropZone">
      <div class="drop-zone-text">
        拖拽视频文件或文件夹到这里，或点击选择文件<br>
        支持 MP4, MKV, AVI, MOV 等格式
      </div>
    </div>

    <div class="status-bar">
      <span class="status-label" id="statusLabel">等待文件...</span>
    </div>

    <div class="progress-wrapper">
      <div class="progress-bar-track">
        <div class="progress-bar-fill" id="progressFill"></div>
      </div>
      <button class="btn btn-stop" id="stopBtn" disabled><svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M4.929 4.929 19.07 19.071"/></svg></button>
    </div>

    <div class="log-panel">
      <div class="log-header">处理日志</div>
      <div class="log-content" id="logContent"></div>
    </div>

    <div class="settings-area">
      <div class="toggle-switch" id="contextMenuToggle">
        <div class="toggle-track" id="toggleTrack">
          <div class="toggle-thumb"></div>
        </div>
        <span>右键菜单集成</span>
      </div>
    </div>
  `;

  setupDropZone();
  setupStopButton();
  setupContextMenuToggle();
  setupNativeDragDrop();
  setupEventListeners();
  await loadState();

  // If launched with files (right-click context menu), auto-start conversion
  try {
    const initialFiles = await invoke("cmd_get_initial_files");
    if (initialFiles && initialFiles.length > 0) {
      addLog(`通过右键菜单接收到 ${initialFiles.length} 个文件`);
      totalFiles = initialFiles.length;
      completedFiles = 0;
      startConversion(initialFiles);
    }
  } catch (_) {}

  // Poll for forwarded files from secondary instances
  pollPendingFiles();
}

// ---- Drop Zone ----
function setupDropZone() {
  const zone = document.getElementById("dropZone");
  if (!zone) return;
  zone.addEventListener("click", () => { if (!isProcessing) openFilePicker(); });
}

async function openFilePicker() {
  let selected;
  try {
    selected = await open({
      multiple: true,
      filters: [
        { name: "视频文件", extensions: ["mp4","mkv","avi","mov","flv","wmv","webm","m4v","mpg","mpeg","m2ts","ts"] },
        { name: "所有文件", extensions: ["*"] },
      ],
    });
  } catch (e) {
    console.error("[openFilePicker] Error:", e);
    addLog(`打开文件对话框失败: ${e}`, true);
    return;
  }

  if (!selected) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  if (paths.length > 0) handleFiles(paths);
}

// ---- Native Drag-Drop ----
function setupNativeDragDrop() {
  const zone = document.getElementById("dropZone");

  getCurrentWebviewWindow().onDragDropEvent((event) => {
    switch (event.payload.type) {
      case "over":
        if (zone) zone.classList.add("drag-over");
        break;

      case "drop": {
        if (zone) zone.classList.remove("drag-over");
        const paths = event.payload.paths || [];
        if (paths.length > 0) {
          handleFiles(paths);
        } else {
          addLog("未获取到文件路径", true);
        }
        break;
      }

      case "cancel":
        if (zone) zone.classList.remove("drag-over");
        break;
    }
  });
}

// ---- File handling with queue ----
function handleFiles(paths) {
  if (isProcessing) {
    fileQueue.push(...paths);
    totalFiles += paths.length;
    addLog(`${paths.length} 个文件加入队列 (共 ${totalFiles} 个)`);
    return;
  }
  totalFiles = paths.length;
  completedFiles = 0;
  startConversion(paths);
}

async function startConversion(paths) {
  try {
    await invoke("cmd_convert_files", { paths });
    setProcessing(true);
  } catch (e) {
    addLog(`错误: ${e}`, true);
  }
}

function startQueuedFiles() {
  if (fileQueue.length === 0) return;
  const next = fileQueue.splice(0);
  addLog(`继续处理队列中 ${next.length} 个文件`);
  startConversion(next);
}

// ---- Stop ----
function setupStopButton() {
  const btn = document.getElementById("stopBtn");
  if (!btn) return;
  btn.addEventListener("click", async () => {
    try {
      await invoke("cmd_cancel_conversion");
      addLog("转换已停止", true);
      fileQueue = [];
      totalFiles = 0;
      completedFiles = 0;
      setProcessing(false);
    } catch (e) {
      addLog(`停止失败: ${e}`, true);
    }
  });
}

function setProcessing(processing) {
  isProcessing = processing;
  const btn = document.getElementById("stopBtn");
  if (btn) btn.disabled = !processing;
}

// ---- Log ----
function addLog(message, isError = false) {
  const content = document.getElementById("logContent");
  if (!content) return;
  const entry = document.createElement("div");
  entry.className = "log-entry" + (isError ? " error" : "");
  entry.textContent = message;
  content.appendChild(entry);
  content.scrollTop = content.scrollHeight;
}

// ---- Progress ----
function updateProgress(percent, filename) {
  const fill = document.getElementById("progressFill");
  const label = document.getElementById("statusLabel");
  if (!fill) return;

  const shortName = filename ? filename.split(/[\\/]/).pop() : null;

  // Build [current/total] prefix for status bar
  const pos = totalFiles > 1 ? `[${completedFiles + 1}/${totalFiles}] ` : "";

  if (percent < 0) {
    fill.className = "progress-bar-fill indeterminate";
    fill.style.width = "30%";
    if (label) label.textContent = shortName ? `${pos}${shortName}...` : "处理中...";
  } else {
    fill.className = "progress-bar-fill";
    fill.style.width = `${percent}%`;
    if (label && shortName) {
      label.textContent = `${pos}${shortName}  (${Math.round(percent)}%)`;
    }
  }
}

// ---- Events ----
function setupEventListeners() {
  listen("conversion-progress", (event) => {
    const { percent, filename } = event.payload;
    updateProgress(percent, filename);
  });

  listen("conversion-log", (event) => {
    const { message, is_error } = event.payload;
    addLog(message, is_error);
  });

  listen("conversion-complete", (event) => {
    const { success_count, total } = event.payload;
    completedFiles += success_count;
    updateProgress(100, null);

    const label = document.getElementById("statusLabel");

    if (completedFiles >= totalFiles || fileQueue.length === 0) {
      // All done
      if (label) label.textContent = `转换完成！${completedFiles}/${totalFiles} 成功`;
      addLog("-".repeat(40));
      addLog(`全部完成: ${completedFiles}/${totalFiles} 个文件成功`);
      setProcessing(false);
    } else {
      // More in queue
      if (label) label.textContent = `${completedFiles}/${totalFiles} 完成，队列中还有 ${fileQueue.length} 个...`;
      addLog(`进度: ${completedFiles}/${totalFiles} 完成`);
      setProcessing(false);
      setTimeout(() => startQueuedFiles(), 500);
    }
  });
}

// ---- Context Menu Toggle ----
function setupContextMenuToggle() {
  const toggle = document.getElementById("contextMenuToggle");
  if (!toggle) return;
  toggle.addEventListener("click", async () => {
    try {
      if (contextMenuRegistered) {
        await invoke("cmd_unregister_context_menu");
        contextMenuRegistered = false;
      } else {
        const msg = await invoke("cmd_register_context_menu");
        addLog(msg);
        contextMenuRegistered = true;
      }
      updateToggleUI();
    } catch (e) { addLog(`右键菜单操作失败: ${e}`, true); }
  });
}

function updateToggleUI() {
  const track = document.getElementById("toggleTrack");
  if (track) track.classList.toggle("active", contextMenuRegistered);
}

async function loadState() {
  try {
    contextMenuRegistered = await invoke("cmd_is_context_menu_registered");
    updateToggleUI();

    const encoderInfo = await invoke("cmd_get_encoder_info");
    addLog(`就绪 - 使用 ${encoderInfo}`);
  } catch (e) {
    addLog(`初始化错误: ${e}`, true);
  }
}

// ---- Pending Files Poll ----
function pollPendingFiles() {
  setInterval(async () => {
    try {
      const pending = await invoke("cmd_check_pending_files");
      if (pending && pending.length > 0) {
        handleFiles(pending);
      }
    } catch (_) {}
  }, 1500);
}

// ---- Init ----
console.log("[main.js] Starting buildUI...");
buildUI();
