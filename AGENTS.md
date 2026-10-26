# AGENTS.md · AutoTappo 开发者与智能体架构指南

> 本文件为后续在此项目工作的 AI Agent 及人类开发者提供**核心上下文、逆向发现、避坑指南与新架构规划**。

---

## 1. 项目背景与目标

**AutoTappo** 是一款专为库洛游戏《鸣潮》（Wuthering Waves）官方桌面宠物挂件 **WuwaTappo** 设计的**超轻量、纯静默、高拟真后台连点与数据看板助手**。

### 核心体验诉求
1. **完全后台无感**：用户在正常敲代码、写文档、打游戏、看视频的同时，桌宠在后台全速累计有效点击数；
2. **绝对零鼠标干涉**：严禁任何物理鼠标指针抖动或位移；
3. **零快捷键冲突**：不抢占系统打字输入、不破坏输入法中英文切换（Shift 键）、不打断鼠标划词或拖拽；
4. **原生桌面体验**：全新技术栈采用 **Rust + Tauri v2 + 轻量前端**，支持**最小化到系统托盘常驻守护**，关闭窗口时销毁 WebView2 节约内存（参考 `CampusFlow` 托盘模型）。

---

## 2. WuwaTappo 官方桌宠逆向研究总结

通过对官方二进制程序（`D:\wuwatappo\WuwaTappo.exe`）以及解压缩的前端 JavaScript 代码的逆向分析，确认以下关键事实：

### 2.1 统计数据存储与 XOR 混淆解密
桌宠的本地数据位于 `%APPDATA%\com.kuro.wuwatappo.cn\`：
1. **累计点击文件**：`cumulative_click_cn-cn-prod.dat`
   - **XOR 混淆密钥**：`kuro-mc-cumulative-click-v1`
   - **解密后结构**：`{"count": 1252, "cuid": "521924715", "environment": "cn-cn-prod"}`
2. **按键统计文件**：`key_press_count_cn-cn-prod.dat`
   - **XOR 混淆密钥**：`kuro-mc-keypress-store-v1`
   - **解密后结构**：`{"t": 3, "environment": "cn-cn-prod"}`

> **注**：Rust 后端可内置该密钥，实现启动时或定时读取用户真实的累计有效点击数，展示在看板上。

### 2.2 官方输入监听与拍桌机制
- 官方基于 Rust 的 `rdev` 库安装了 Windows 全局低级输入钩子（`WH_KEYBOARD_LL` 与 `WH_MOUSE_LL`）；
- 当系统捕获到键鼠动作时，Rust 后端向 WebView 抛送 `key-press`、`key-release`、`global-clap-mouse` 等事件；
- **核心计数防刷规则**：
  - **长按同一个按键不会重复累加**（`Pr.has(e)` 去重），必须有交替击键或明确的松开事件（`key-release`）；
  - 每满 999 次点击后归零并从当前装备表情中随机浮现一个气泡表情。

### 2.3 官方硬编码快捷键（严重避坑指南！）
桌宠前端源码中写死了系统级快捷键，**本项目必须严格避开**：
- **`F8`**：**切换显示器（跨屏幕调度）**（`switchDisplayMonitor.js: It = new Set(["F8", "f8"])`）
- **`F9`**：**老板键（隐藏/显示桌宠）**
- **`F7`**：**关闭游戏模式 / 恢复鼠标交互**
- **`Ctrl+Alt+P`**：**切换鼠标穿透**
- **`Ctrl+Alt+1~5`**：播放已装备表情

> **原则**：AutoTappo 的启动/暂停快捷键严禁使用 F7/F8/F9，建议采用 **`F6`** 或 **`F10`**。

---

## 3. 防干扰与输入拟真技术规范

### 3.1 幻影按键 (Phantom Keys)
为了确保发送的按键**绝对不在任何浏览器、终端、聊天框中打出字符**，且**不与任何日常软件快捷键冲突**，统一采用 Windows 虚拟扩展功能键：
- **`VK_F21` ~ `VK_F24`**（`0x84 ~ 0x87`）
- 击键时在 F21 ~ F24 之间随机轮询交替，满足桌宠交替按键计数的机制。

### 3.2 全场景礼貌避让 (Polite Pausing)
利用 Windows `GetAsyncKeyState` 毫秒级侦测用户物理操作：
1. **打字与输入法保护**：侦测物理 A-Z、0-9、空格、退格、回车；
2. **修饰键保护**：侦测物理 `Shift`、`Ctrl`、`Alt`、`Win`；
   - 微软拼音等输入法依赖“按下 Shift 并松开，中途无其它按键”触发中英文切换，检测到 Shift 时必须立即暂停发包，且松开后给予 150ms 冷却；
3. **鼠标交互保护**：侦测物理鼠标左键、右键、中键（`0x01, 0x02, 0x04`），用户划词拖拽或选中文本时绝对不打断。

### 3.3 鼠标光标防消失与防闪烁
- 启动时通过 `SystemParametersInfoW(SPI_SETMOUSEVANISH, 0, False, SPIF_UPDATEINIFILE | SPIF_SENDCHANGE)` 临时关闭 Windows 的“打字时隐藏鼠标指针”，退出时自动恢复；
- 对于 Windows Terminal 等终端内置的隐藏设置，建议提示用户在终端交互设置中关闭“键入时隐藏鼠标指针”，绝不在后台发送任何破坏物理手感的鼠标位移。

---

## 4. 新架构设计（参考 CampusFlow 托盘模型）

AutoTappo 计划使用 **Rust + Tauri v2 + Vanilla Web 前端** 进行重构：

```
autotappo/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs         # Tauri 应用启动入口与插件装配
│       ├── tray.rs         # 托盘图标构建、事件绑定、右键菜单
│       ├── window.rs       # 窗口生命周期管理 (按需创建 / 销毁以节约内存)
│       ├── clicker.rs      # 后台极速连点引擎与礼貌避让逻辑 (独立线程)
│       ├── decoder.rs      # 本地 XOR 统计文件解密读取
│       └── commands.rs     # IPC 接口 (供前端调用的启停、改配置、读统计)
├── web/
│   ├── index.html          # 精致看板 UI (仪表盘、档位切换、热键配置)
│   ├── style.css           # 现代化玻璃拟态 / 极简科技风
│   └── app.js              # 前端交互与 Tauri IPC 绑定
├── .gitignore
├── README.md
└── AGENTS.md
```

### 托盘与窗口生命周期核心逻辑（复用 CampusFlow 经验）
1. **按需创建与按需销毁**：
   - 默认启动或者点击托盘时调用 `window::ensure`，已存在则激活聚焦，不存在则新建；
   - **关闭窗口拦截**：监听关闭事件，关闭时**直接销毁窗口**而非仅隐藏，从而将 WebView2 占用的 200MB~300MB 内存彻底归还系统；
   - **后台守护持续常驻**：后台连点引擎在 Rust 独立原生线程中跑，窗口销毁后托盘依然常驻，内存消耗降至几兆；
2. **托盘菜单功能**：
   - `显示主窗口`
   - `启动 / 暂停连点`（显示当前状态）
   - `分隔线`
   - `完全退出`（设置退出标记后调用 `app.exit(0)`）

---

## 5. 提交与 Git 规范

- 早期探索阶段的 Python/AHK/批处理脚本已被纳入 `.gitignore`，不得提交进 Git 仓库；
- 保持 Git 提交历史清晰、原子化，遵循 Conventional Commits（例如 `feat:`, `fix:`, `docs:`, `refactor:`）。
