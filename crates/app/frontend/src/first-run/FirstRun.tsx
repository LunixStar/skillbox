/// 首次运行向导（§10 五步：欢迎 → 扫描 → 选择 → 执行 → 完成）。
/// 硬约束：首次运行绝不移动 / 删除用户文件；默认「原地登记」；
/// 「统一到中央库」为可选勾选（默认关闭），统一时原目录由部署纪律
/// 移入 trash（= 操作前快照，可恢复）。

import { useEffect, useMemo, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  listenScanEvents,
  scanCancel,
  scanStart,
  scanStatus,
  type ScanProgress,
  type ScanReport,
} from "../api/ipc";
import { ipcError } from "../api/ipc";
import { useCompleteFirstRun, useDeployApply, useInstallLocal, useTools } from "../api/queries";
import { Button, Skeleton, Toggle } from "../components/ui";
import { useToast } from "../components/Toast";

type Step = "welcome" | "scanning" | "summary" | "executing" | "done";

interface ItemResult {
  name: string;
  ok: boolean;
  detail: string;
}

export default function FirstRun() {
  const [step, setStep] = useState<Step>("welcome");
  const [report, setReport] = useState<ScanReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [live, setLive] = useState<ScanProgress | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [unify, setUnify] = useState(false);
  const [execTotal, setExecTotal] = useState(0);
  const [execResults, setExecResults] = useState<ItemResult[]>([]);

  const toolsQuery = useTools();
  const install = useInstallLocal();
  const deploy = useDeployApply();
  const complete = useCompleteFirstRun();
  const toast = useToast();

  const toolNames = useMemo(() => {
    const m: Record<string, string> = {};
    for (const t of toolsQuery.data ?? []) {
      m[t.id] = t.displayName;
    }
    return m;
  }, [toolsQuery.data]);

  // ------------------------------------------------------------------
  // 扫描步骤：订阅事件 + 启动 / 附加会话；轮询兜底（事件丢失窗口）。
  // ------------------------------------------------------------------
  useEffect(() => {
    if (step !== "scanning") {
      return;
    }
    let cancelled = false;
    let off: UnlistenFn | null = null;
    let timer = 0;

    const finish = (r: ScanReport) => {
      window.clearInterval(timer);
      if (cancelled) {
        return;
      }
      setReport(r);
      setSelected(new Set(r.skills.map((s) => s.path)));
      setStep("summary");
    };
    const fail = (msg: string) => {
      window.clearInterval(timer);
      if (!cancelled) {
        setError(msg);
      }
    };

    void (async () => {
      off = await listenScanEvents({
        onProgress: (p) => setLive(p),
        onDone: finish,
        onError: fail,
      });
      if (cancelled) {
        return;
      }
      // StrictMode 重放 / 已有会话：只附加，不重复启动。
      const st = await scanStatus();
      if (cancelled) {
        return;
      }
      if (!st.running) {
        if (st.report) {
          finish(st.report);
          return;
        }
        if (st.error) {
          fail(st.error);
          return;
        }
        try {
          await scanStart();
        } catch (e) {
          fail(ipcError(e));
        }
      }
      timer = window.setInterval(() => {
        void scanStatus()
          .then((s) => {
            if (cancelled || s.running) {
              return;
            }
            if (s.report) {
              finish(s.report);
            } else if (s.error) {
              fail(s.error);
            }
          })
          .catch(() => {
            /* 轮询失败忽略，下轮再试 */
          });
      }, 1200);
    })();

    return () => {
      cancelled = true;
      off?.();
      window.clearInterval(timer);
    };
  }, [step]);

  // ------------------------------------------------------------------
  // 执行登记（按钮触发，非 effect——避免 StrictMode 双跑）。
  // ------------------------------------------------------------------
  async function runRegistration() {
    const items = (report?.skills ?? []).filter((s) => selected.has(s.path));
    setExecTotal(items.length);
    setExecResults([]);
    setStep("executing");
    const results: ItemResult[] = [];
    for (const s of items) {
      const label = s.name ?? s.dirName;
      try {
        if (unify && !s.readOnly && s.toolId) {
          // 统一到中央库：复制入中央 → 以链接替换原目录（原目录入 trash，可恢复）。
          const out = await install.mutateAsync({
            path: s.path,
            name: s.name,
            keepInPlace: false,
            dryRun: false,
          });
          if (out.skillId != null) {
            await deploy.mutateAsync({
              skillId: out.skillId,
              tools: [s.toolId],
              mode: "symlink",
              overwrite: true,
              dryRun: false,
            });
          }
          results.push({ name: label, ok: true, detail: "已统一到中央库" });
        } else {
          await install.mutateAsync({
            path: s.path,
            name: s.name,
            keepInPlace: true,
            dryRun: false,
          });
          results.push({
            name: label,
            ok: true,
            detail: s.readOnly ? "只读目录，已原地登记" : "已原地登记",
          });
        }
      } catch (e) {
        results.push({ name: label, ok: false, detail: ipcError(e) });
      }
      setExecResults([...results]);
    }
    setStep("done");
  }

  function togglePath(path: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  const foundGroups = useMemo(() => {
    const groups: { toolId: string | null; items: ScanReport["skills"] }[] = [];
    for (const s of report?.skills ?? []) {
      const last = groups[groups.length - 1];
      if (last && last.toolId === s.toolId) {
        last.items.push(s);
      } else {
        groups.push({ toolId: s.toolId, items: [s] });
      }
    }
    return groups;
  }, [report]);

  const okCount = execResults.filter((r) => r.ok).length;
  const failCount = execResults.length - okCount;

  // ------------------------------------------------------------------
  // 步骤渲染
  // ------------------------------------------------------------------
  return (
    <div className="flex flex-1 items-center justify-center overflow-y-auto bg-page px-6 py-10">
      <div className="w-full max-w-2xl">
        {step === "welcome" && (
          <div className="sb-card interactive px-10 py-12 text-center">
            <h1 className="text-2xl font-semibold text-ink">欢迎使用 SkillBox</h1>
            <p className="mt-3 text-sm leading-6 text-ink-secondary">
              SkillBox 帮你把散落在各个 AI 工具目录里的 Skill 集中登记、统一管理。
              首先扫描你机器上的 Claude Code、Codex 等工具目录，找出已有的 Skill。
            </p>
            <p className="mt-1.5 text-xs leading-5 text-ink-faint">
              首次运行只做两件事：扫描和登记。不会移动、修改或删除你的任何文件。
            </p>
            <Button
              variant="primary"
              className="mt-8 h-9 px-6"
              onClick={() => {
                setError(null);
                setLive(null);
                setStep("scanning");
              }}
            >
              开始扫描
            </Button>
          </div>
        )}

        {step === "scanning" && (
          <div className="sb-card interactive px-10 py-12">
            <h2 className="text-lg font-semibold text-ink">正在扫描…</h2>
            {error ? (
              <div className="mt-6">
                <p className="text-sm text-danger">扫描失败：{error}</p>
                <div className="mt-5 flex gap-2">
                  <Button
                    variant="primary"
                    onClick={() => {
                      setError(null);
                      setLive(null);
                    }}
                  >
                    重试
                  </Button>
                  <Button variant="ghost" onClick={() => setStep("welcome")}>
                    返回
                  </Button>
                </div>
              </div>
            ) : (
              <>
                <div className="sb-pulse mt-6 h-1.5 w-full overflow-hidden rounded-full bg-line">
                  <div className="h-full w-1/3 rounded-full bg-primary" />
                </div>
                <div className="mt-6 min-h-24 text-sm leading-7 text-ink-secondary">
                  {live === null ? (
                    <Skeleton className="h-5 w-64" />
                  ) : (
                    <>
                      <div>
                        已扫描 <span className="font-medium text-ink">{live.scanned}</span> 个目录
                        · 已发现 <span className="font-medium text-ink">{live.found}</span> 个 Skill
                      </div>
                      {Object.entries(live.foundByTool).map(([tool, n]) => (
                        <div key={tool} className="text-primary-active">
                          发现 {toolNames[tool] ?? tool} ×{n}
                        </div>
                      ))}
                      <div className="truncate text-xs text-ink-faint">正在查看 {live.current}</div>
                    </>
                  )}
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  className="mt-4"
                  onClick={() => {
                    void scanCancel().catch((e) => toast.show(ipcError(e), "danger"));
                    setStep("welcome");
                  }}
                >
                  取消扫描
                </Button>
              </>
            )}
          </div>
        )}

        {step === "summary" && report && (
          <div className="sb-card interactive px-8 py-8">
            <h2 className="text-lg font-semibold text-ink">
              扫描完成，发现 {report.skills.length} 个 Skill
            </h2>
            {(report.skipped.length > 0 || report.warnings.length > 0) && (
              <p className="mt-1 text-xs text-ink-faint">
                另跳过 {report.skipped.length} 个目录 · {report.warnings.length} 条告警 · 耗时{" "}
                {(report.durationMs / 1000).toFixed(1)}s
              </p>
            )}

            <div className="mt-5 max-h-72 space-y-4 overflow-y-auto pr-1">
              {foundGroups.length === 0 && (
                <p className="text-sm text-ink-secondary">
                  没有发现已有 Skill。你可以稍后在「技能库」手动登记。
                </p>
              )}
              {foundGroups.map((g) => (
                <div key={g.toolId ?? "central"}>
                  <div className="flex items-center justify-between">
                    <div className="text-xs font-semibold text-ink">
                      {g.toolId ? toolNames[g.toolId] ?? g.toolId : "中央库 / 未知来源"}
                      <span className="ml-1.5 font-normal text-ink-faint">×{g.items.length}</span>
                    </div>
                    <button
                      type="button"
                      className="text-xs text-primary-active hover:text-primary"
                      onClick={() =>
                        setSelected((prev) => {
                          const next = new Set(prev);
                          const allIn = g.items.every((s) => next.has(s.path));
                          for (const s of g.items) {
                            if (allIn) {
                              next.delete(s.path);
                            } else {
                              next.add(s.path);
                            }
                          }
                          return next;
                        })
                      }
                    >
                      全选 / 全不选
                    </button>
                  </div>
                  <div className="mt-1.5 space-y-1">
                    {g.items.map((s) => (
                      <label
                        key={s.path}
                        className="flex cursor-pointer items-start gap-2.5 rounded-[8px] px-2 py-1.5 hover:bg-surface"
                      >
                        <input
                          type="checkbox"
                          className="mt-1 accent-[#3B8EF0]"
                          checked={selected.has(s.path)}
                          onChange={() => togglePath(s.path)}
                        />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-sm text-ink">
                            {s.name ?? s.dirName}
                            {s.readOnly && (
                              <span className="ml-2 rounded-[6px] bg-surface px-1.5 py-0.5 text-xs text-ink-faint">
                                只读
                              </span>
                            )}
                          </span>
                          <span className="block truncate text-xs text-ink-faint">
                            {s.description ?? s.path}
                          </span>
                        </span>
                      </label>
                    ))}
                  </div>
                </div>
              ))}
            </div>

            <div className="mt-6 rounded-card border border-line bg-surface px-4 py-3">
              <Toggle
                checked={unify}
                onChange={setUnify}
                label="统一到中央库（可选）"
              />
              <p className="mt-1.5 text-xs leading-5 text-ink-faint">
                {unify
                  ? "每个勾选的 Skill 将复制进中央库，并把原目录替换为指向正本的链接；原文件会先移入回收站（可恢复）。"
                  : "默认原地登记：只记录位置，不移动、不修改任何文件。之后可随时从卡片上部署到各工具。"}
              </p>
            </div>

            <div className="mt-6 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setStep("welcome")}>
                上一步
              </Button>
              {report.skills.length === 0 ? (
                <Button variant="primary" onClick={() => setStep("done")}>
                  完成
                </Button>
              ) : (
                <Button
                  variant="primary"
                  disabled={selected.size === 0}
                  onClick={() => void runRegistration()}
                >
                  {unify ? "统一并登记" : "原地登记"} {selected.size} 个 Skill
                </Button>
              )}
            </div>
          </div>
        )}

        {step === "executing" && (
          <div className="sb-card interactive px-10 py-12">
            <h2 className="text-lg font-semibold text-ink">
              正在登记… {execResults.length} / {execTotal}
            </h2>
            <div className="mt-6 h-1.5 w-full overflow-hidden rounded-full bg-line">
              <div
                className="h-full origin-left rounded-full bg-primary transition-transform duration-200 ease-skillbox"
                style={{ transform: `scaleX(${execTotal === 0 ? 0 : execResults.length / execTotal})` }}
              />
            </div>
            <div className="mt-5 max-h-56 space-y-1 overflow-y-auto text-sm leading-6">
              {execResults.map((r, i) => (
                <div key={i} className={r.ok ? "text-ink-secondary" : "text-danger"}>
                  {r.ok ? "✓" : "✗"} {r.name} — {r.detail}
                </div>
              ))}
            </div>
          </div>
        )}

        {step === "done" && (
          <div className="sb-card interactive px-10 py-12 text-center">
            <h2 className="text-xl font-semibold text-ink">
              {failCount === 0 ? "全部完成！" : "登记完成（部分失败）"}
            </h2>
            {execResults.length > 0 && (
              <p className="mt-2 text-sm text-ink-secondary">
                成功 {okCount} 个
                {failCount > 0 && (
                  <>
                    ，失败 <span className="text-danger">{failCount}</span> 个
                  </>
                )}
                。{unify ? "已统一到中央库。" : "已原地登记，文件位置保持不变。"}
              </p>
            )}
            {failCount > 0 && (
              <div className="mt-4 max-h-40 space-y-1 overflow-y-auto text-left text-xs leading-5 text-danger">
                {execResults
                  .filter((r) => !r.ok)
                  .map((r, i) => (
                    <div key={i}>
                      {r.name}：{r.detail}
                    </div>
                  ))}
              </div>
            )}
            <Button
              variant="primary"
              className="mt-8 h-9 px-6"
              disabled={complete.isPending}
              onClick={() => complete.mutate(undefined, { onError: (e) => toast.show(ipcError(e), "danger") })}
            >
              进入 SkillBox
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
