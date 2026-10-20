export type Schedule =
  | { kind: "interval"; everySecs: number }
  | { kind: "once"; runAt: number };

export type Task = {
  id: string;
  name: string;
  prompt: string;
  schedule: Schedule;
  nextRunAt: number | null;
  enabled: boolean;
  createdAt: number;
  lastRunAt: number | null;
};

export type TaskDraft = { name: string; prompt: string; schedule: Schedule };

export type Fire = {
  id: string;
  taskId: string;
  firedAt: number;
  conversationId: string | null;
  status: string;
  error: string | null;
};

export const MIN_INTERVAL_SECS = 60;

const UNIT_SECS = { minutes: 60, hours: 3600, days: 86400 } as const;
export type IntervalUnit = keyof typeof UNIT_SECS;

/** 数量 + 单位换成秒；数量不是正整数时返回 null。 */
export function intervalSecs(
  amount: number,
  unit: IntervalUnit,
): number | null {
  if (!Number.isInteger(amount) || amount <= 0) return null;
  return amount * UNIT_SECS[unit];
}

/** 把秒数拆成最大的整除单位；无法整除分钟时会四舍五入，可能改变原间隔。 */
export function splitInterval(secs: number): {
  amount: number;
  unit: IntervalUnit;
} {
  for (const unit of ["days", "hours"] as const) {
    if (secs % UNIT_SECS[unit] === 0) {
      return { amount: secs / UNIT_SECS[unit], unit };
    }
  }
  return { amount: Math.max(1, Math.round(secs / 60)), unit: "minutes" };
}

/** `datetime-local` 的值（本地时间）转毫秒时间戳；格式不对返回 null。 */
export function localToMs(value: string): number | null {
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? null : ms;
}
