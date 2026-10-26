//! BDD 验收测试：自动开箱决策引擎（纯逻辑，无 Win32 依赖）
//!
//! 需求来源：SPEC.md REQ-BOX 组（倒计时感知调度 + 领取确认/回滚状态机）。
//! 命名约定：given_<前置状态>_when_<触发动作>_then_<期望结果>。

use autotappo::box_logic::{
    countdown_wait_ms, decide, parse_countdown_text, Decision, Phase, ScanOutcome,
    ANIMATING_WAIT_MS, BACKOFF_MS, COUNTDOWN_CAP_MS, MIN_SCAN_GAP_MS, SLOW_POLL_MS,
    VERIFY_DELAY_MS, WAKE_GRACE_MS,
};

// ---------------- 倒计时文本解析（读取桌宠 reward-countdown 的 aria-label） ----------------

#[test]
fn given_mmss_text_when_parsed_then_returns_total_seconds() {
    // Given 官方气泡展示的合法倒计时 "12:34"
    // When 解析文本
    let got = parse_countdown_text("12:34");
    // Then 返回 754 秒
    assert_eq!(got, Some(754));
}

#[test]
fn given_zero_countdown_when_parsed_then_returns_zero() {
    // Given 倒计时恰好走到 "00:00"
    // When 解析
    let got = parse_countdown_text("00:00");
    // Then 返回 0（应立即进入唤醒窗口）
    assert_eq!(got, Some(0));
}

#[test]
fn given_surrounding_spaces_when_parsed_then_is_tolerant() {
    // Given 文本两端带空白（UIA 取值常见）
    // When 解析
    let got = parse_countdown_text("  07:05  ");
    // Then 仍返回 425 秒
    assert_eq!(got, Some(425));
}

#[test]
fn given_malformed_text_when_parsed_then_returns_none() {
    // Given 非倒计时文本（aria-label 为空 / 未登录 / 乱码）
    // When 解析
    // Then 一律返回 None，不得误触发定时唤醒
    assert_eq!(parse_countdown_text(""), None);
    assert_eq!(parse_countdown_text("abc"), None);
    assert_eq!(parse_countdown_text("1:23"), None); // 分钟必须两位
    assert_eq!(parse_countdown_text("12:3"), None); // 秒必须两位
    assert_eq!(parse_countdown_text("12:60"), None); // 秒越界
}

#[test]
fn given_impossible_long_countdown_when_parsed_then_returns_none() {
    // Given 超过 31 分钟的异常文本（宝箱周期最长 30 分钟）
    // When 解析 "99:00"（5940 秒）
    // Then 判定为脏数据返回 None
    assert_eq!(parse_countdown_text("99:00"), None);
}

// ---------------- 倒计时唤醒调度 ----------------

#[test]
fn given_countdown_seconds_when_scheduled_then_wakes_after_grace() {
    // Given 倒计时还剩 5 秒
    // When 计算下次唤醒
    let wait = countdown_wait_ms(5);
    // Then 在 5 秒 + 600ms 宽限后唤醒（文本为向上取整，需等它真正归零）
    assert_eq!(wait, 5_000 + WAKE_GRACE_MS);
}

#[test]
fn given_zero_countdown_when_scheduled_then_respects_minimum_scan_gap() {
    // Given 倒计时已经归零
    // When 计算唤醒
    let wait = countdown_wait_ms(0);
    // Then 至少间隔 1 秒，避免高频空转
    assert_eq!(wait, MIN_SCAN_GAP_MS);
}

#[test]
fn given_huge_countdown_when_scheduled_then_is_capped_for_resync() {
    // Given 倒计时长达 2000 秒（33 分钟以上，理论不应出现但需防御）
    // When 计算唤醒
    let wait = countdown_wait_ms(2000);
    // Then 最多 10 分钟必须重新读一次倒计时校准时钟（防文本停更/系统休眠漂移）
    assert_eq!(wait, COUNTDOWN_CAP_MS);
}

// ---------------- 决策状态机：监测阶段 ----------------

#[test]
fn given_watching_and_ready_chest_when_decided_then_claims_and_enters_verify() {
    // Given 监测中，扫描发现 1 个就绪宝箱
    // When 决策
    let d: Decision = decide(Phase::Watching, ScanOutcome::Ready(1), 1_000_000);
    // Then 立即领取：计数 +1，进入待确认阶段，3.5 秒后复查
    assert!(d.claim);
    assert_eq!(d.opens_delta, 1);
    assert_eq!(
        d.next_phase,
        Phase::PendingVerify { attempted_at_ms: 1_000_000, ready_seen: 1 }
    );
    assert_eq!(d.next_scan_in_ms, VERIFY_DELAY_MS);
}

#[test]
fn given_watching_and_countdown_when_decided_then_schedules_precise_wake() {
    // Given 监测中，读到最早槽位还剩 480 秒（8 分钟，未超 10 分钟校准上限）
    // When 决策
    let d = decide(Phase::Watching, ScanOutcome::Countdown(480), 0);
    // Then 不领取，精确休眠到就绪后 600ms 宽限点（替代旧的 10 秒盲轮询）
    assert!(!d.claim);
    assert_eq!(d.opens_delta, 0);
    assert_eq!(d.next_phase, Phase::Watching);
    assert_eq!(d.next_scan_in_ms, 480_000 + WAKE_GRACE_MS);
}

#[test]
fn given_watching_and_mid_length_countdown_when_decided_then_resyncs_in_chunks() {
    // Given 监测中，读到 754 秒（12.5 分钟，超过 10 分钟校准上限）
    // When 决策
    let d = decide(Phase::Watching, ScanOutcome::Countdown(754), 0);
    // Then 先睡到上限，醒来重读倒计时校准（防文本停更/系统休眠时钟漂移）
    assert_eq!(d.next_scan_in_ms, COUNTDOWN_CAP_MS);
}

#[test]
fn given_watching_and_nothing_visible_when_decided_then_slow_polls() {
    // Given 监测中但未登录 / 桌宠未运行 / 倒计时未展示
    // When 决策
    let d = decide(Phase::Watching, ScanOutcome::Nothing, 0);
    // Then 每 60 秒慢轮询一次，把空闲 CPU 占用压到最低
    assert_eq!(d.next_scan_in_ms, SLOW_POLL_MS);
    assert!(!d.claim);
}

#[test]
fn given_watching_and_claim_animating_when_decided_then_backs_off_briefly() {
    // Given 扫描只看到开箱动画中的按钮（--press/--open 变体）
    // When 决策
    let d = decide(Phase::Watching, ScanOutcome::Animating, 0);
    // Then 不碰它，2.5 秒后再看
    assert!(!d.claim);
    assert_eq!(d.next_scan_in_ms, ANIMATING_WAIT_MS);
}

// ---------------- 决策状态机：领取确认阶段 ----------------

#[test]
fn given_pending_verify_and_button_still_there_when_decided_then_rolls_back_and_backs_off() {
    // Given 领取前只看到 1 个就绪按钮，3.5 秒复查时它仍以初始形态存在
    // When 决策
    let phase = Phase::PendingVerify { attempted_at_ms: 1_000, ready_seen: 1 };
    let d = decide(phase, ScanOutcome::Ready(1), 5_000);
    // Then 判定本次触发未生效（点数不足/未登录）：回滚计数并进入 5 分钟冷却
    assert!(!d.claim);
    assert_eq!(d.opens_delta, -1);
    assert_eq!(d.next_phase, Phase::BackoffUntil { until_ms: 5_000 + BACKOFF_MS });
    assert_eq!(d.next_scan_in_ms, BACKOFF_MS);
}

#[test]
fn given_pending_verify_and_second_slot_ready_when_decided_then_chains_next_claim() {
    // Given 领取前看到 2 个就绪宝箱，复查时剩 1 个（第一个已成功入袋）
    // When 决策
    let phase = Phase::PendingVerify { attempted_at_ms: 1_000, ready_seen: 2 };
    let d = decide(phase, ScanOutcome::Ready(1), 5_000);
    // Then 第一次领取确认成功（计数保留），链式领取第二个
    assert!(d.claim);
    assert_eq!(d.opens_delta, 1);
    assert_eq!(
        d.next_phase,
        Phase::PendingVerify { attempted_at_ms: 5_000, ready_seen: 1 }
    );
    assert_eq!(d.next_scan_in_ms, VERIFY_DELAY_MS);
}

#[test]
fn given_pending_verify_and_slot_moved_to_countdown_when_decided_then_confirms_success() {
    // Given 复查时按钮已被倒计时气泡取代（领取成功、下一轮 30 分钟计时开始，剩 300 秒）
    // When 决策
    let phase = Phase::PendingVerify { attempted_at_ms: 1_000, ready_seen: 1 };
    let d = decide(phase, ScanOutcome::Countdown(300), 5_000);
    // Then 确认成功，转入监测阶段并按新倒计时精准休眠
    assert!(!d.claim);
    assert_eq!(d.opens_delta, 0);
    assert_eq!(d.next_phase, Phase::Watching);
    assert_eq!(d.next_scan_in_ms, 300_000 + WAKE_GRACE_MS);
}

#[test]
fn given_pending_verify_and_nothing_when_decided_then_confirms_success_and_slow_polls() {
    // Given 复查时宝箱 UI 完全消失（如桌宠被老板键隐藏）
    // When 决策
    let phase = Phase::PendingVerify { attempted_at_ms: 1_000, ready_seen: 1 };
    let d = decide(phase, ScanOutcome::Nothing, 5_000);
    // Then 视为领取成功，转慢轮询
    assert_eq!(d.opens_delta, 0);
    assert_eq!(d.next_phase, Phase::Watching);
    assert_eq!(d.next_scan_in_ms, SLOW_POLL_MS);
}

#[test]
fn given_pending_verify_and_still_animating_when_decided_then_waits_patiently() {
    // Given 复查时按钮仍在开箱动画中
    // When 决策
    let phase = Phase::PendingVerify { attempted_at_ms: 1_000, ready_seen: 1 };
    let d = decide(phase, ScanOutcome::Animating, 5_000);
    // Then 保持待确认状态不动，2.5 秒后再次复查
    assert!(!d.claim);
    assert_eq!(d.opens_delta, 0);
    assert_eq!(d.next_phase, phase);
    assert_eq!(d.next_scan_in_ms, ANIMATING_WAIT_MS);
}

// ---------------- 决策状态机：冷却阶段 ----------------

#[test]
fn given_backoff_not_expired_when_decided_then_sleeps_remaining_time() {
    // Given 冷却中且未到期
    // When 决策
    let phase = Phase::BackoffUntil { until_ms: 100_000 };
    let d = decide(phase, ScanOutcome::Nothing, 40_000);
    // Then 只睡剩余的 60 秒
    assert!(!d.claim);
    assert_eq!(d.next_phase, phase);
    assert_eq!(d.next_scan_in_ms, 60_000);
}

#[test]
fn given_backoff_expired_and_ready_when_decided_then_claims_again() {
    // Given 冷却期结束，宝箱仍在（此时点数可能已攒够）
    // When 决策
    let phase = Phase::BackoffUntil { until_ms: 100_000 };
    let d = decide(phase, ScanOutcome::Ready(1), 100_000);
    // Then 重新尝试领取
    assert!(d.claim);
    assert_eq!(d.opens_delta, 1);
    assert_eq!(
        d.next_phase,
        Phase::PendingVerify { attempted_at_ms: 100_000, ready_seen: 1 }
    );
}

#[test]
fn given_backoff_expired_and_countdown_when_decided_then_returns_to_watching() {
    // Given 冷却期结束，按钮已被倒计时气泡取代（说明冷却期间宝箱已成功开掉）
    // When 决策
    let phase = Phase::BackoffUntil { until_ms: 100_000 };
    let d = decide(phase, ScanOutcome::Countdown(120), 100_000);
    // Then 回到监测阶段，按倒计时精准唤醒
    assert!(!d.claim);
    assert_eq!(d.opens_delta, 0);
    assert_eq!(d.next_phase, Phase::Watching);
    assert_eq!(d.next_scan_in_ms, 120_000 + WAKE_GRACE_MS);
}

// ---------------- 防御性边界 ----------------

#[test]
fn given_zero_ready_count_when_decided_then_treated_as_nothing() {
    // Given 扫描结果异常地携带 Ready(0)
    // When 决策
    let d = decide(Phase::Watching, ScanOutcome::Ready(0), 0);
    // Then 等同于 Nothing，慢轮询，绝不发起无意义领取
    assert!(!d.claim);
    assert_eq!(d.next_scan_in_ms, SLOW_POLL_MS);
}
