// AutoTappo 前端主逻辑 (支持原生浏览器独立双击预览 & Gitee国内直连检测 & 可自定义全局热键)

const CURRENT_VERSION = "v0.1.0";
const GITEE_API_URL = "https://gitee.com/api/v5/repos/To0Ru/AutoTappo/releases/latest";
const GITHUB_API_URL = "https://api.github.com/repos/To0Ru/AutoTappo/releases/latest";

// 检测是否在 Tauri 原生客户端运行
const isTauri = typeof window.__TAURI__ !== "undefined" && typeof window.__TAURI__.core !== "undefined";

// 安全 invoke 包装
const invoke = isTauri ? window.__TAURI__.core.invoke : async (cmd, args) => {
  console.log("[Browser Preview Mode] Invoke called:", cmd, args);
  return null;
};

// DOM 元素引用
const statusBadge = document.getElementById("status-badge");
const counterValue = document.getElementById("counter-value");
const btnToggle = document.getElementById("btn-toggle");
const btnToggleIcon = document.getElementById("btn-toggle-icon");
const btnToggleText = document.getElementById("btn-toggle-text");
const btnReset = document.getElementById("btn-reset");
const speedButtons = document.querySelectorAll(".speed-btn");
const btnBell = document.getElementById("btn-bell");
const updateDot = document.getElementById("update-dot");
const btnHide = document.getElementById("btn-hide");
const btnQuit = document.getElementById("btn-quit");

// 热键自定义 DOM
const btnHotkey = document.getElementById("btn-hotkey");
const lblHotkey = document.getElementById("lbl-hotkey");
const hotkeyTip = document.getElementById("hotkey-tip");

// Modal 弹窗元素
const updateModal = document.getElementById("update-modal");
const btnModalClose = document.getElementById("btn-modal-close");
const lblCurVersion = document.getElementById("lbl-cur-version");
const lblLatestVersion = document.getElementById("lbl-latest-version");
const updateStatusBox = document.getElementById("update-status-box");
const updateStatusText = document.getElementById("update-status-text");
const updateSpinner = document.getElementById("update-spinner");
const changelogArea = document.getElementById("changelog-area");
const changelogContent = document.getElementById("changelog-content");
const updateGuideCard = document.getElementById("update-guide-card");
const linkGithubRepo = document.getElementById("link-github-repo");
const lblGithubRepoUrl = document.getElementById("lbl-github-repo-url");
const btnCheckNow = document.getElementById("btn-check-now");
const btnCheckText = document.getElementById("btn-check-text");
const btnDownload = document.getElementById("btn-download");

// 状态变量
let currentRunning = false;
let currentSpeed = 0;
let localSentCount = 0;
let previewInterval = null;
let lastCheckTime = 0;
let cachedLatestData = null;

// 热键状态 (默认 F6, VK: 0x75 = 117)
let currentHotkeyName = localStorage.getItem("autotappo_hotkey_name") || "F6";
let currentHotkeyVK = parseInt(localStorage.getItem("autotappo_hotkey_vk"), 10) || 0x75;
let isRecordingHotkey = false;

const versionText = document.querySelector(".version-text");
if (versionText) versionText.textContent = CURRENT_VERSION;
lblCurVersion.textContent = CURRENT_VERSION;
lblHotkey.textContent = currentHotkeyName;

// 更新主界面 UI
function updateUI(status) {
  if (!status) return;

  currentRunning = status.running;
  currentSpeed = status.speed_mode;
  localSentCount = status.total_sent;

  // 1. 运行状态徽章
  if (currentRunning) {
    statusBadge.textContent = "运行中";
    statusBadge.classList.add("running");
  } else {
    statusBadge.textContent = "已暂停";
    statusBadge.classList.remove("running");
  }

  // 2. 模拟点击计数
  counterValue.textContent = localSentCount.toLocaleString();

  // 3. 大按钮状态
  if (currentRunning) {
    btnToggle.classList.add("running");
    btnToggleIcon.textContent = "⏸";
    btnToggleText.textContent = `暂停挂机 (${currentHotkeyName})`;
  } else {
    btnToggle.classList.remove("running");
    btnToggleIcon.textContent = "▶";
    btnToggleText.textContent = `开始挂机 (${currentHotkeyName})`;
  }

  // 4. 速度档位高亮
  speedButtons.forEach(btn => {
    const sp = parseInt(btn.dataset.speed, 10);
    if (sp === currentSpeed) {
      btn.classList.add("active");
    } else {
      btn.classList.remove("active");
    }
  });
}

// 轮询同步状态
async function pollStatus() {
  if (isTauri) {
    try {
      const status = await invoke("get_status");
      updateUI(status);
    } catch (err) {
      console.error("Failed to fetch status:", err);
    }
  }
}

// ---------------- 浏览器独立预览模式支持 ----------------
if (!isTauri) {
  console.info("💡 当前处于【纯浏览器预览模式】，已自动开启交互模拟与测试。");
  updateUI({
    running: false,
    speed_mode: 0,
    total_sent: 0
  });

  function startPreviewLoop() {
    if (previewInterval) clearInterval(previewInterval);
    const speedIntervals = [100, 45, 22]; // 休闲(~10/s), 快速(~22/s), 极速(~45/s)
    previewInterval = setInterval(() => {
      if (currentRunning) {
        localSentCount += 1;
        counterValue.textContent = localSentCount.toLocaleString();
      }
    }, speedIntervals[currentSpeed] || 100);
  }
  startPreviewLoop();
}

// ---------------- 交互事件绑定 ----------------
btnToggle.addEventListener("click", async () => {
  if (isTauri) {
    try {
      const newRunning = await invoke("toggle_running");
      currentRunning = newRunning;
      pollStatus();
    } catch (err) {
      console.error(err);
    }
  } else {
    currentRunning = !currentRunning;
    updateUI({
      running: currentRunning,
      speed_mode: currentSpeed,
      total_sent: localSentCount
    });
  }
});

btnReset.addEventListener("click", async () => {
  if (isTauri) {
    try {
      await invoke("reset_sent_count");
    } catch (err) {
      console.error(err);
    }
  }
  localSentCount = 0;
  counterValue.textContent = "0";
});

speedButtons.forEach(btn => {
  btn.addEventListener("click", async () => {
    const mode = parseInt(btn.dataset.speed, 10);
    currentSpeed = mode;
    if (isTauri) {
      try {
        await invoke("set_speed", { mode });
        pollStatus();
      } catch (err) {
        console.error(err);
      }
    } else {
      updateUI({
        running: currentRunning,
        speed_mode: currentSpeed,
        total_sent: localSentCount
      });
    }
  });
});

const hideWindow = async () => {
  if (isTauri) {
    try {
      await invoke("minimize_to_tray");
    } catch (err) {
      console.error(err);
    }
  } else {
    alert("【预览提示】在编译后的应用中，点击将无缝收进 Windows 系统托盘守护。");
  }
};

btnHide.addEventListener("click", hideWindow);

btnQuit.addEventListener("click", async () => {
  if (isTauri) {
    try {
      await invoke("quit_app");
    } catch (err) {
      console.error(err);
    }
  } else {
    alert("【预览提示】在编译后的应用中，点击将完全退出守护进程。");
  }
});

// ---------------- 全局热键自定义录制逻辑 ----------------
const FORBIDDEN_HOTKEYS = {
  "F7": "F7 为官方游戏模式键，无法绑定",
  "F8": "F8 为官方多屏幕切换键，无法绑定",
  "F9": "F9 为官方老板键隐藏键，无法绑定",
};

// 常见按键映射为 Windows Virtual Key (VK)
function getWinVirtualKey(e) {
  // F1 - F12
  if (e.code.startsWith("F") && !isNaN(e.code.slice(1))) {
    const num = parseInt(e.code.slice(1), 10);
    if (num >= 1 && num <= 12) return { vk: 0x6F + num, name: e.code };
  }
  const specialMap = {
    "Home": { vk: 0x24, name: "Home" },
    "End": { vk: 0x23, name: "End" },
    "PageUp": { vk: 0x21, name: "PgUp" },
    "PageDown": { vk: 0x22, name: "PgDn" },
    "Insert": { vk: 0x2D, name: "Ins" },
    "Delete": { vk: 0x2E, name: "Del" },
    "Pause": { vk: 0x13, name: "Pause" },
    "ScrollLock": { vk: 0x91, name: "ScrLk" },
    "Backquote": { vk: 0xC0, name: "`" },
    "NumpadAdd": { vk: 0x6B, name: "Num+" },
    "NumpadSubtract": { vk: 0x6D, name: "Num-" },
    "NumpadMultiply": { vk: 0x6A, name: "Num*" },
    "NumpadDivide": { vk: 0x6F, name: "Num/" },
  };
  if (specialMap[e.code]) return specialMap[e.code];

  // 字母键 A-Z
  if (e.code.startsWith("Key")) {
    const char = e.code.slice(3).toUpperCase();
    return { vk: char.charCodeAt(0), name: char };
  }

  return null;
}

btnHotkey.addEventListener("click", () => {
  if (isRecordingHotkey) {
    cancelRecording();
    return;
  }
  startRecording();
});

function startRecording() {
  isRecordingHotkey = true;
  btnHotkey.classList.add("recording");
  lblHotkey.textContent = "请按键...";
  hotkeyTip.textContent = "按 Esc 取消";

  window.addEventListener("keydown", handleRecordKey, { capture: true });
}

function cancelRecording() {
  isRecordingHotkey = false;
  btnHotkey.classList.remove("recording");
  lblHotkey.textContent = currentHotkeyName;
  hotkeyTip.textContent = "点击修改";
  window.removeEventListener("keydown", handleRecordKey, { capture: true });
}

async function handleRecordKey(e) {
  e.preventDefault();
  e.stopPropagation();

  if (e.key === "Escape") {
    cancelRecording();
    return;
  }

  // 检查是否为禁止占用的按键
  if (FORBIDDEN_HOTKEYS[e.key]) {
    alert(`【提示】${FORBIDDEN_HOTKEYS[e.key]}，请换其他按键。`);
    cancelRecording();
    return;
  }

  const result = getWinVirtualKey(e);
  if (!result) {
    alert("【提示】请按下有效的功能键 (如 F1~F6, F10~F12, Home, PgUp, 数字小键盘等)");
    cancelRecording();
    return;
  }

  currentHotkeyName = result.name;
  currentHotkeyVK = result.vk;

  localStorage.setItem("autotappo_hotkey_name", currentHotkeyName);
  localStorage.setItem("autotappo_hotkey_vk", currentHotkeyVK.toString());

  lblHotkey.textContent = currentHotkeyName;
  btnHotkey.classList.remove("recording");
  hotkeyTip.textContent = "修改成功";
  setTimeout(() => { hotkeyTip.textContent = "点击修改"; }, 1500);

  // 同步给 Rust 后端
  if (isTauri) {
    try {
      await invoke("set_hotkey", { vk: currentHotkeyVK });
    } catch (err) {
      console.error("Failed to set hotkey in backend:", err);
    }
  }

  // 刷新按钮文字
  updateUI({
    running: currentRunning,
    speed_mode: currentSpeed,
    total_sent: localSentCount
  });

  isRecordingHotkey = false;
  window.removeEventListener("keydown", handleRecordKey, { capture: true });
}

// 页面加载时向后端同步保存的热键
if (isTauri) {
  invoke("set_hotkey", { vk: currentHotkeyVK }).catch(() => {});
}

// ---------------- 检查更新逻辑 (Gitee 国内直连优先 + 红点感知) ----------------
function openUpdateModal() {
  updateModal.classList.add("show");
  if (cachedLatestData) {
    applyUpdateData(cachedLatestData);
  } else {
    checkUpdate({ silent: false });
  }
}

function closeUpdateModal() {
  updateModal.classList.remove("show");
}

btnBell.addEventListener("click", openUpdateModal);
btnModalClose.addEventListener("click", closeUpdateModal);
updateModal.addEventListener("click", (e) => {
  if (e.target === updateModal) {
    closeUpdateModal();
  }
});

function compareVersions(v1, v2) {
  const clean1 = (v1 || "").replace(/^v/, "").split(".").map(Number);
  const clean2 = (v2 || "").replace(/^v/, "").split(".").map(Number);
  for (let i = 0; i < Math.max(clean1.length, clean2.length); i++) {
    const num1 = clean1[i] || 0;
    const num2 = clean2[i] || 0;
    if (num1 > num2) return 1;
    if (num1 < num2) return -1;
  }
  return 0;
}

async function checkUpdate({ silent = false } = {}) {
  lastCheckTime = Date.now();

  if (!silent) {
    updateSpinner.style.display = "inline-block";
    updateStatusBox.className = "update-status-box";
    updateStatusText.textContent = "正在通过 Gitee 国内高速通道检查最新版本...";
    btnCheckNow.disabled = true;
    btnCheckText.textContent = "正在检查...";
    changelogArea.style.display = "none";
    if (updateGuideCard) updateGuideCard.style.display = "none";
    btnDownload.style.display = "none";
  }

  let latestData = null;
  let source = "Gitee";

  // 1. 优先尝试 Gitee API (国内直连)
  try {
    const res = await fetch(GITEE_API_URL, { headers: { "Accept": "application/json" } });
    if (res.ok) {
      latestData = await res.json();
    }
  } catch (e) {
    console.warn("Gitee check failed, trying fallback:", e);
  }

  // 2. 若 Gitee 暂无数据（尚未镜像）或失败，自动尝试 GitHub API 兜底
  if (!latestData || !latestData.tag_name) {
    try {
      source = "GitHub";
      const res = await fetch(GITHUB_API_URL, { headers: { "Accept": "application/json" } });
      if (res.ok) {
        latestData = await res.json();
      }
    } catch (e) {
      console.warn("GitHub check failed:", e);
    }
  }

  cachedLatestData = { data: latestData, source };

  if (!silent) {
    updateSpinner.style.display = "none";
    btnCheckNow.disabled = false;
    btnCheckText.textContent = "再次检查";
  }

  applyUpdateData(cachedLatestData, silent);
}

function applyUpdateData(result, silent = false) {
  if (!result) return;
  const { data: latestData, source } = result;

  if (latestData && latestData.tag_name) {
    const latestTag = latestData.tag_name;
    lblLatestVersion.textContent = latestTag;

    const cmp = compareVersions(latestTag, CURRENT_VERSION);
    if (cmp > 0) {
      updateDot.classList.add("active");
      updateStatusBox.className = "update-status-box has-new";
      updateStatusText.innerHTML = `🎉 <strong>发现新版本 ${latestTag} (${source} 源)</strong>`;

      const body = latestData.body || latestData.description || "包含稳定性优化与体验升级。";
      changelogContent.textContent = body;
      changelogArea.style.display = "block";

      // 明确给出 GitHub 仓库地址与安装提示
      const repoUrl = latestData.html_url || "https://github.com/To0Ru/AutoTappo/releases";
      if (updateGuideCard) {
        updateGuideCard.style.display = "flex";
        linkGithubRepo.href = repoUrl;
        lblGithubRepoUrl.textContent = repoUrl;
      }

      btnDownload.href = repoUrl;
      btnDownload.style.display = "inline-flex";
    } else {
      updateDot.classList.remove("active");
      updateStatusBox.className = "update-status-box is-latest";
      updateStatusText.textContent = `✨ 当前已是最新版本！(${source} 源同步)`;
      changelogArea.style.display = "none";
      if (updateGuideCard) updateGuideCard.style.display = "none";
      btnDownload.style.display = "none";
    }
  } else {
    updateDot.classList.remove("active");
    lblLatestVersion.textContent = CURRENT_VERSION;
    updateStatusBox.className = "update-status-box is-latest";
    updateStatusText.textContent = `✨ 当前运行版本 ${CURRENT_VERSION} 已是最新稳定版！`;
    changelogArea.style.display = "none";
    if (updateGuideCard) updateGuideCard.style.display = "none";
    btnDownload.style.display = "none";
  }
}

btnCheckNow.addEventListener("click", () => checkUpdate({ silent: false }));

// ---------------- 界面唤醒 / 打开时的静默自动检测逻辑 ----------------
function triggerAutoCheck() {
  const now = Date.now();
  if (now - lastCheckTime > 60000) {
    checkUpdate({ silent: true });
  }
}

window.addEventListener("focus", triggerAutoCheck);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    triggerAutoCheck();
  }
});
triggerAutoCheck();

// 启动状态轮询
pollStatus();
if (isTauri) {
  setInterval(pollStatus, 500);
}

// 方便在纯浏览器独立预览时随时测试“发现新版本”的视觉效果
window.testNewVersionUI = function() {
  openUpdateModal();
  applyUpdateData({
    data: {
      tag_name: "v0.2.0",
      html_url: "https://github.com/To0Ru/AutoTappo/releases",
      body: "1. 新增【自动开箱】功能，满额自动抽卡\n2. 优化后台静默避让策略，提升长时间挂机稳定性\n3. 支持更多自定义全局快捷键"
    },
    source: "Gitee"
  });
};
