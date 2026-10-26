// AutoTappo 前端主逻辑 (纯原生 ES6，零额外依赖)

const invoke = window.__TAURI__?.core?.invoke || (async (cmd, args) => {
  console.log("[Mock Invoke]", cmd, args);
  if (cmd === "get_status") {
    return { running: false, speed_mode: 0, total_sent: 0 };
  }
  return true;
});

// DOM 元素引用
const statusBadge = document.getElementById("status-badge");
const counterValue = document.getElementById("counter-value");
const btnToggle = document.getElementById("btn-toggle");
const btnToggleIcon = document.getElementById("btn-toggle-icon");
const btnToggleText = document.getElementById("btn-toggle-text");
const btnReset = document.getElementById("btn-reset");
const speedButtons = document.querySelectorAll(".speed-btn");
const btnMinimize = document.getElementById("btn-minimize");
const btnHide = document.getElementById("btn-hide");
const btnQuit = document.getElementById("btn-quit");

let currentRunning = false;
let currentSpeed = 0;

// 更新 UI 状态
function updateUI(status) {
  if (!status) return;

  currentRunning = status.running;
  currentSpeed = status.speed_mode;

  // 1. 运行徽章
  if (currentRunning) {
    statusBadge.textContent = "运行中";
    statusBadge.classList.add("running");
  } else {
    statusBadge.textContent = "已暂停";
    statusBadge.classList.remove("running");
  }

  // 2. 模拟点击计数
  counterValue.textContent = status.total_sent.toLocaleString();

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

// 轮询同步状态（每 500ms 一次，保持与后台原生线程及托盘菜单的实时同步）
async function pollStatus() {
  try {
    const status = await invoke("get_status");
    updateUI(status);
  } catch (err) {
    console.error("Failed to fetch status:", err);
  }
}

// 绑定事件
btnToggle.addEventListener("click", async () => {
  try {
    const newRunning = await invoke("toggle_running");
    currentRunning = newRunning;
    pollStatus();
  } catch (err) {
    console.error("Failed to toggle:", err);
  }
});

btnReset.addEventListener("click", async () => {
  try {
    await invoke("reset_sent_count");
    counterValue.textContent = "0";
  } catch (err) {
    console.error("Failed to reset count:", err);
  }
});

speedButtons.forEach(btn => {
  btn.addEventListener("click", async () => {
    const mode = parseInt(btn.dataset.speed, 10);
    try {
      await invoke("set_speed", { mode });
      currentSpeed = mode;
      pollStatus();
    } catch (err) {
      console.error("Failed to set speed:", err);
    }
  });
});

const hideWindow = async () => {
  try {
    await invoke("minimize_to_tray");
  } catch (err) {
    console.error("Failed to minimize to tray:", err);
  }
};

btnMinimize.addEventListener("click", hideWindow);
btnHide.addEventListener("click", hideWindow);

btnQuit.addEventListener("click", async () => {
  try {
    await invoke("quit_app");
  } catch (err) {
    console.error("Failed to quit app:", err);
  }
});

// 初始化
pollStatus();
setInterval(pollStatus, 500);
