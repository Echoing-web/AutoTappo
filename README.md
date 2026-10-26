# AutoTappo 🐾

> 《鸣潮》官方桌宠 WuwaTappo 超轻量、纯静默、高拟真后台连点与数据看板

[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D4?logo=windows)](https://github.com/)
[![Tech](https://img.shields.io/badge/Tech-Rust%20%7C%20Tauri%20v2-orange?logo=rust)](https://tauri.app/)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

你是否希望在电脑前全神贯注地写代码、查文档、刷视频或打游戏的同时，桌宠也在桌面后台源源不断地为你刷取点击数？

**AutoTappo** 专为鸣潮官方桌面宠物 **WuwaTappo** 量身定制：**全后台静默运行、绝对零鼠标干涉、智能打字避让、托盘极简常驻。**

---

## ✨ 核心特性

- 🔕 **纯后台零感运行**
  采用 Windows 底层保留的 `F21~F24` 顶层幻影功能键（Phantom Keys）。不在任何文本框产生文字，不抢占窗口焦点，不移动真实鼠标光标。
- 🤝 **智能礼貌避让 (Polite Pausing)**
  内置毫秒级物理输入感知：
  - **打字与输入法保护**：你在敲击键盘或按 `Shift` 切换中英文时，脚本毫秒级静默避让，绝不卡键、吞字；
  - **鼠标划词保护**：按住鼠标左键/右键选中文字或拖拽窗口时立即暂停，绝不打断鼠标手势。
- 🛡️ **官方热键避让保护**
  通过逆向官方桌宠源码，完美避开原作者硬编码的系统热键：
  - 避开 `F8`（官方切屏热键）
  - 避开 `F9`（官方老板键隐藏）
  - 避开 `F7`（官方关闭游戏模式热键）
  - 采用独立的 `F6` / `F10` 启动与暂停。
- 📊 **本地数据实时解密**
  深度破译桌宠本地存储的 XOR 加密数据文件（`cumulative_click`），实时看板展示桌宠入账的有效总点击数。
- 🖥️ **轻量系统托盘守护 (Tauri v2 + Rust)**
  参考现代桌面小程序架构（借鉴 [CampusFlow](https://github.com/To0Ru/CampusFlow) 托盘模型）：
  - 点击关闭按钮时**销毁前端窗口归还 WebView2 内存**，仅保留几兆大小的 Rust 托盘守护；
  - 随时双击托盘图标秒级唤醒控制面板。

---

## 🏗️ 架构与技术选型

本项目全新重构版本基于 **Rust (Tauri v2)** 构建，兼顾极速编译、小巧体积与极佳的系统资源控制：

| 模块 | 技术选型 | 说明 |
| :--- | :--- | :--- |
| **应用宿主** | **Tauri v2 (Rust)** | 原生 Windows 跨平台宿主，体积小、安全、无多余运行时 |
| **托盘模型** | **Tauri TrayIcon + 动态生命周期** | 支持最小化到右下角托盘，销毁窗口释放 WebView 内存 |
| **输入引擎** | **Win32 API (winapi / windows-rs)** | 零延迟幻影键注入与多重拟真抖动调度 |
| **前端界面** | **Vanilla HTML5 / CSS3 / ES6** | 零臃肿依赖、零 node_modules 膨胀，编译速度秒级 |

---

## 📖 官方桌宠技术机制探秘

通过反编译与逆向分析官方 `WuwaTappo.exe`，我们梳理出了以下核心交互逻辑：

```
[ 用户物理动作 / 虚拟幻影键 F21~F24 ]
                │
                ▼
   [ Windows WH_KEYBOARD_LL / WH_MOUSE_LL ] (Rust rdev 钩子)
                │
                ▼
      [ Tauri WebView 事件分发 ]
                ├── key-press   ──> 拍桌手势
                └── key-release ──> 计数器 +1 (满 999 浮现气泡)
```

1. **官方按键防刷逻辑**：
   - 官方监听器对按住单键不放做了去重（`Pr.has(e)`），长按同一键仅算 1 次；
   - 必须在按键之间产生**交替击键**并触发**按键松开（key-release）**，才能持续累加点击数。
2. **数据加密**：
   - 本地数据存储在 `%APPDATA%\com.kuro.wuwatappo.cn\` 目录下，采用固定异或密钥混淆。本程序内置对应解码模块。

---

## 🗺️ 开发路线图 (Roadmap)

- [x] 完成官方桌宠底层逻辑逆向、XOR 算法破解与热键冲突排查
- [x] 完成 Python 概念验证与全场景礼貌避让逻辑
- [ ] 构建 `src-tauri` 原生工程与轻量托盘守护框架
- [ ] 移植输入模拟与物理避让引擎至 Rust 原生线程
- [ ] 构建极简看板 UI（展示当前点击数、速度档位调节、运行状态）
- [ ] GitHub Actions 自动化多平台/Windows 单文件免安装 EXE 打包发布

---

## 🤝 协作与贡献

如果你是 AI 智能体或开发者，请在开展工作前阅读：
- **[AGENTS.md](./AGENTS.md)**：包含详细的逆向上下文、避坑规范与模块设计。

---

## ⚖️ 免责声明 (Disclaimer)

本项目仅供个人学习、技术逆向研究与自动化技术探索使用，不修改官方客户端内存，不提供任何联机破坏性功能。所有与《鸣潮》相关的素材、形象版权均归库洛游戏（Kuro Games）所有。
