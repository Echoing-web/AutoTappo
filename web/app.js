// AutoTappo 前端主逻辑 (支持原生浏览器独立双击预览 & Gitee国内直连检测)

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
const btnCheckNow = document.getElementById("btn-check-now");
const btnCheckText = document.getElementById("btn-check-text");
const btnDownload = document.getElementById("btn-download");

// 状态变量
let currentRunning = false;
let currentSpeed = 0;
let localSentCount = 0;
let previewInterval = null;

lblCurVersion.textContent = CURRENT_VERSION;

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
    btnToggleText.textContent = "暂停挂机 (F6)";
  } else {
    btnToggle.classList.remove("running");
    btnToggleIcon.textContent = "▶";
    btnToggleText.textContent = "开始挂机 (F6)";
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
    // 浏览器预览模式切换
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

// ---------------- 检查更新逻辑 (Gitee 国内直连优先) ----------------
function openUpdateModal() {
  updateModal.classList.add("show");
  // 首次打开自动触发一次检查
  checkUpdate();
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

// 版本比对辅助函数 (例如 "v0.1.0" 比对 "v0.1.1")
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

// 检查更新主函数
async function checkUpdate() {
  updateSpinner.style.display = "inline-block";
  updateStatusBox.className = "update-status-box";
  updateStatusText.textContent = "正在通过 Gitee 国内高速通道检查最新版本...";
  btnCheckNow.disabled = true;
  btnCheckText.textContent = "正在检查...";
  changelogArea.style.display = "none";
  btnDownload.style.display = "none";

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

  updateSpinner.style.display = "none";
  btnCheckNow.disabled = false;
  btnCheckText.textContent = "再次检查";

  // 3. 处理比对结果
  if (latestData && latestData.tag_name) {
    const latestTag = latestData.tag_name;
    lblLatestVersion.textContent = latestTag;

    const cmp = compareVersions(latestTag, CURRENT_VERSION);
    if (cmp > 0) {
      // 发现新版本
      updateStatusBox.classList.add("has-new");
      updateStatusText.textContent = `🎉 发现新版本 ${latestTag} (${source} 源)`;
      updateDot.classList.add("active");

      // 显示更新日志
      const body = latestData.body || latestData.description || "包含稳定性优化与体验升级。";
      changelogContent.textContent = body;
      changelogArea.style.display = "block";

      // 提供下载入口
      btnDownload.href = latestData.html_url || "https://github.com/To0Ru/AutoTappo/releases/latest";
      btnDownload.style.display = "inline-flex";
    } else {
      // 已是最新
      updateStatusBox.classList.add("is-latest");
      updateStatusText.textContent = `✨ 当前已是最新版本！(${source} 源同步)`;
      updateDot.classList.remove("active");
    }
  } else {
    // 仓库尚未发布正式 Release 时的友好提示
    lblLatestVersion.textContent = CURRENT_VERSION;
    updateStatusBox.classList.add("is-latest");
    updateStatusText.textContent = `✨ 当前运行版本 ${CURRENT_VERSION} 已是最新稳定版！`;
    updateDot.classList.remove("active");
  }
}

btnCheckNow.addEventListener("click", checkUpdate);

// 启动轮询
pollStatus();
if (isTauri) {
  setInterval(pollStatus, 500);
}
