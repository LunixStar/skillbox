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
import { useI18n } from "../i18n";

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
  const { t } = useI18n();

  const toolNames = useMemo(() => {
    const m: Record<string, string> = {};
    for (const x of toolsQuery.data ?? []) {
      m[x.id] = x.displayName;
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
          results.push({ name: label, ok: true, detail: t("firstrun.resultUnified") });
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
            detail: s.readOnly ? t("firstrun.resultReadOnly") : t("firstrun.resultInPlace"),
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
            <h1 className="text-2xl font-semibold text-ink">{t("firstrun.welcomeTitle")}</h1>
            <p className="mt-3 text-sm leading-6 text-ink-secondary">{t("firstrun.welcomeP1")}</p>
            <p className="mt-1.5 text-xs leading-5 text-ink-faint">{t("firstrun.welcomeP2")}</p>
            <Button
              variant="primary"
              className="mt-8 h-9 px-6"
              onClick={() => {
                setError(null);
                setLive(null);
                setStep("scanning");
              }}
            >
              {t("firstrun.startScan")}
            </Button>
          </div>
        )}

        {step === "scanning" && (
          <div className="sb-card interactive px-10 py-12">
            <h2 className="text-lg font-semibold text-ink">{t("firstrun.scanningTitle")}</h2>
            {error ? (
              <div className="mt-6">
                <p className="text-sm text-danger">{t("common.scanFailed", { msg: error })}</p>
                <div className="mt-5 flex gap-2">
                  <Button
                    variant="primary"
                    onClick={() => {
                      setError(null);
                      setLive(null);
                    }}
                  >
                    {t("common.retry")}
                  </Button>
                  <Button variant="ghost" onClick={() => setStep("welcome")}>
                    {t("common.back")}
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
                      <div>{t("firstrun.scanProgress", { scanned: live.scanned, found: live.found })}</div>
                      {Object.entries(live.foundByTool).map(([tool, n]) => (
                        <div key={tool} className="text-primary-active">
                          {t("firstrun.scanFoundTool", { tool: toolNames[tool] ?? tool, n })}
                        </div>
                      ))}
                      <div className="truncate text-xs text-ink-faint">{t("firstrun.scanCurrent", { path: live.current })}</div>
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
                  {t("firstrun.cancelScan")}
                </Button>
              </>
            )}
          </div>
        )}

        {step === "summary" && report && (
          <div className="sb-card interactive px-8 py-8">
            <h2 className="text-lg font-semibold text-ink">
              {t("firstrun.summaryTitle", { n: report.skills.length })}
            </h2>
            {(report.skipped.length > 0 || report.warnings.length > 0) && (
              <p className="mt-1 text-xs text-ink-faint">
                {t("firstrun.summaryMeta", {
                  skipped: report.skipped.length,
                  warnings: report.warnings.length,
                  sec: (report.durationMs / 1000).toFixed(1),
                })}
              </p>
            )}

            <div className="mt-5 max-h-72 space-y-4 overflow-y-auto pr-1">
              {foundGroups.length === 0 && (
                <p className="text-sm text-ink-secondary">{t("firstrun.summaryNone")}</p>
              )}
              {foundGroups.map((g) => (
                <div key={g.toolId ?? "central"}>
                  <div className="flex items-center justify-between">
                    <div className="text-xs font-semibold text-ink">
                      {g.toolId ? toolNames[g.toolId] ?? g.toolId : t("firstrun.centralGroup")}
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
                      {t("firstrun.toggleAll")}
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
                                {t("common.readOnly")}
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
              <Toggle checked={unify} onChange={setUnify} label={t("firstrun.unifyTitle")} />
              <p className="mt-1.5 text-xs leading-5 text-ink-faint">
                {unify ? t("firstrun.unifyOn") : t("firstrun.unifyOff")}
              </p>
            </div>

            <div className="mt-6 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setStep("welcome")}>
                {t("firstrun.prevStep")}
              </Button>
              {report.skills.length === 0 ? (
                <Button variant="primary" onClick={() => setStep("done")}>
                  {t("firstrun.done")}
                </Button>
              ) : (
                <Button
                  variant="primary"
                  disabled={selected.size === 0}
                  onClick={() => void runRegistration()}
                >
                  {(unify ? t("firstrun.unifyAndRegister") : t("firstrun.registerInPlace")) +
                    t("firstrun.skillsCount", { n: selected.size })}
                </Button>
              )}
            </div>
          </div>
        )}

        {step === "executing" && (
          <div className="sb-card interactive px-10 py-12">
            <h2 className="text-lg font-semibold text-ink">
              {t("firstrun.execTitle", { done: execResults.length, total: execTotal })}
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
              {failCount === 0 ? t("firstrun.doneAllTitle") : t("firstrun.donePartialTitle")}
            </h2>
            {execResults.length > 0 && (
              <p className="mt-2 text-sm text-ink-secondary">
                {t("firstrun.doneOkCount", { n: okCount })}
                {failCount > 0 && <span className="text-danger">{t("firstrun.doneFailCount", { n: failCount })}</span>}
                {unify ? t("firstrun.doneUnifiedNote") : t("firstrun.doneInPlaceNote")}
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
              {t("firstrun.enterApp")}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
