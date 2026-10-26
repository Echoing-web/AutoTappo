//! 自动开箱决策引擎（纯逻辑，零 Win32 依赖，可完整单元/集成测试）
//!
//! 输入：一轮 UIA 扫描结果 + 当前阶段；输出：是否领取、计数增减、下一阶段与下次扫描时间。
//! 设计目标（SPEC.md REQ-BOX / REQ-PERF）：
//! 1. 宝箱状态自动检测——读取桌宠倒计时气泡 aria-label，精准定时唤醒，就绪后 ~1 秒内领取；
//! 2. 低 CPU——用「读倒计时 → 长休眠」替代高频盲轮询，空闲时最多 60 秒一次扫描。

/// 无任何宝箱 UI 可见时的慢轮询间隔（未登录 / 桌宠未运行 / 倒计时未展示）
pub const SLOW_POLL_MS: u64 = 60_000;
/// 倒计时唤醒的最大休眠上限：超过则重新读取倒计时校准（防文本停更 / 系统休眠时钟漂移）
pub const COUNTDOWN_CAP_MS: u64 = 600_000;
/// 按钮处于开箱动画（--press/--open）时的让位等待
pub const ANIMATING_WAIT_MS: u64 = 2_500;
/// 领取触发后的复查等待：需覆盖官方按压缩放 + 服务端往返
pub const VERIFY_DELAY_MS: u64 = 3_500;
/// 领取未生效（点数不足 / 未登录）后的冷却期，避免在桌宠侧刷错误气泡
pub const BACKOFF_MS: u64 = 5 * 60 * 1000;
/// 倒计时文本为向上取整秒数，唤醒额外宽限，确保按钮已真正出现
pub const WAKE_GRACE_MS: u64 = 600;
/// 任何相邻两次扫描的最小间隔，杜绝高频空转
pub const MIN_SCAN_GAP_MS: u64 = 1_000;
/// 倒计时文本的合理性上限：宝箱周期最长 30 分钟，超过视为脏数据
const COUNTDOWN_SANITY_SECS: u32 = 31 * 60;

/// 一轮扫描对宝箱槽位状态的概括
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanOutcome {
    /// 发现 n 个可领取的就绪按钮（类名精确等于 reward-btn 且未禁用）
    Ready(u32),
    /// 只看到开箱动画中的按钮变体（--press / --open），此刻不要碰
    Animating,
    /// 最早就绪的槽位还剩 s 秒（来自 reward-countdown 的 aria-label）
    Countdown(u32),
    /// 无任何宝箱 UI 可见
    Nothing,
}

/// 决策引擎所处阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 常规监测
    Watching,
    /// 已触发一次领取，等待复查确认是否真正生效
    PendingVerify {
        attempted_at_ms: u64,
        /// 触发时可见的就绪按钮数，用于区分「已入袋」与「未生效」
        ready_seen: u32,
    },
    /// 领取未生效后的冷却
    BackoffUntil { until_ms: u64 },
}

/// 一次决策的全部输出
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    /// 是否立即触发一次 UIA Invoke 领取
    pub claim: bool,
    /// 开箱计数增减：+1 预记领取、-1 复查失败回滚、0 无变化
    pub opens_delta: i32,
    /// 状态迁移
    pub next_phase: Phase,
    /// 距下次扫描的毫秒数
    pub next_scan_in_ms: u64,
}

/// 解析桌宠倒计时气泡文本（"MM:SS"），非法/异常返回 None
pub fn parse_countdown_text(text: &str) -> Option<u32> {
    let t = text.trim();
    let mut parts = t.split(':');
    let (mm, ss) = (parts.next()?, parts.next()?);
    if parts.next().is_some() || mm.len() != 2 || ss.len() != 2 {
        return None;
    }
    if !mm.bytes().all(|b| b.is_ascii_digit()) || !ss.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let secs = mm.parse::<u32>().ok()? * 60 + ss.parse::<u32>().ok()?;
    if ss.parse::<u32>().ok()? >= 60 || secs > COUNTDOWN_SANITY_SECS {
        return None;
    }
    Some(secs)
}

/// 按剩余秒数计算下次唤醒间隔：到点 + 宽限；归零时受最小扫描间隔保护；超长封顶强制校准
pub fn countdown_wait_ms(secs: u32) -> u64 {
    (secs as u64 * 1000 + WAKE_GRACE_MS)
        .min(COUNTDOWN_CAP_MS)
        .max(MIN_SCAN_GAP_MS)
}

/// 纯函数决策：给定阶段与扫描结果，产出动作、计数增减与下次扫描时间
pub fn decide(phase: Phase, scan: ScanOutcome, now_ms: u64) -> Decision {
    // Ready(0) 视为异常输入，按 Nothing 处理
    let scan = match scan {
        ScanOutcome::Ready(0) => ScanOutcome::Nothing,
        other => other,
    };

    match phase {
        Phase::Watching => match scan {
            ScanOutcome::Ready(n) => Decision {
                claim: true,
                opens_delta: 1,
                next_phase: Phase::PendingVerify { attempted_at_ms: now_ms, ready_seen: n },
                next_scan_in_ms: VERIFY_DELAY_MS,
            },
            ScanOutcome::Countdown(s) => wait_decision(Phase::Watching, countdown_wait_ms(s)),
            ScanOutcome::Animating => {
                wait_decision(Phase::Watching, ANIMATING_WAIT_MS)
            }
            ScanOutcome::Nothing => wait_decision(Phase::Watching, SLOW_POLL_MS),
        },

        Phase::PendingVerify { attempted_at_ms: _, ready_seen } => match scan {
            // 就绪按钮数量减少 → 上一箱已入袋；仍有剩 → 链式领取下一个
            ScanOutcome::Ready(m) if m < ready_seen => Decision {
                claim: true,
                opens_delta: 1,
                next_phase: Phase::PendingVerify { attempted_at_ms: now_ms, ready_seen: m },
                next_scan_in_ms: VERIFY_DELAY_MS,
            },
            // 数量未减少 → 上一次触发未生效：回滚计数并冷却
            ScanOutcome::Ready(_) => Decision {
                claim: false,
                opens_delta: -1,
                next_phase: Phase::BackoffUntil { until_ms: now_ms + BACKOFF_MS },
                next_scan_in_ms: BACKOFF_MS,
            },
            // 按钮被倒计时气泡取代 / 全部消失 → 领取成功确认
            ScanOutcome::Countdown(s) => {
                wait_decision(Phase::Watching, countdown_wait_ms(s))
            }
            ScanOutcome::Nothing => wait_decision(Phase::Watching, SLOW_POLL_MS),
            ScanOutcome::Animating => wait_decision(phase, ANIMATING_WAIT_MS),
        },

        Phase::BackoffUntil { until_ms } => {
            if now_ms < until_ms {
                wait_decision(phase, until_ms - now_ms)
            } else {
                match scan {
                    ScanOutcome::Ready(n) => Decision {
                        claim: true,
                        opens_delta: 1,
                        next_phase: Phase::PendingVerify { attempted_at_ms: now_ms, ready_seen: n },
                        next_scan_in_ms: VERIFY_DELAY_MS,
                    },
                    ScanOutcome::Countdown(s) => {
                        wait_decision(Phase::Watching, countdown_wait_ms(s))
                    }
                    ScanOutcome::Animating => {
                        wait_decision(Phase::Watching, ANIMATING_WAIT_MS)
                    }
                    ScanOutcome::Nothing => wait_decision(Phase::Watching, SLOW_POLL_MS),
                }
            }
        }
    }
}

fn wait_decision(next_phase: Phase, next_scan_in_ms: u64) -> Decision {
    Decision { claim: false, opens_delta: 0, next_phase, next_scan_in_ms }
}
