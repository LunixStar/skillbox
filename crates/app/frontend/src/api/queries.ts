/// TanStack Query 数据层：查询键 + hooks/mutations。
/// 规则：任何改动库 / 部署 / 工具 / 设置的 mutation 必须失效相关查询，
/// 禁止 useState + useEffect 手动刷新。

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import {
  deployApply,
  deployRevoke,
  costReport,
  doctorFix,
  doctorRun,
  libraryDocRead,
  libraryDocValidate,
  libraryDocWrite,
  libraryInstallLocal,
  libraryList,
  libraryRemove,
  libraryTargets,
  securityReport,
  securityScan,
  settingsGet,
  settingsSet,
  systemOpenPath,
  toolsList,
  toolsSetEnabled,
  updatesApply,
  updatesCheck,
  type DocCheckReport,
  type DocWriteOutcome,
  type InstallOutcome,
  type UpdateCheckOutcome,
  type UpdateOutcome,
} from "./ipc";

export const queryKeys = {
  tools: ["tools"] as const,
  library: ["library"] as const,
  targets: ["targets"] as const,
  /// 体检报告（doctor 只读；修复成功后由页面失效重查）。
  doctor: ["doctor"] as const,
  /// 单个 skill 的安全扫描结果（后端另有缓存，扫描幂等且廉价）。
  security: (skillId: number) => ["security", skillId] as const,
  /// 全库安全概览（后端合并缓存）。
  securityOverview: ["securityOverview"] as const,
  /// 上下文成本报告（§9.2）。
  cost: ["cost"] as const,
  firstRunDone: ["settings", "firstRunDone"] as const,
  /// 详情页 SKILL.md 预览（按 skill id 缓存）。
  skillDoc: (skillId: number) => ["skillDoc", skillId] as const,
};

export const FIRST_RUN_KEY = "first_run.done";

// ---------------------------------------------------------------------------
// 查询
// ---------------------------------------------------------------------------

export function useTools() {
  return useQuery({ queryKey: queryKeys.tools, queryFn: toolsList });
}

export function useLibrary() {
  return useQuery({ queryKey: queryKeys.library, queryFn: libraryList });
}

export function useTargets() {
  return useQuery({ queryKey: queryKeys.targets, queryFn: libraryTargets });
}

export function useFirstRunDone() {
  return useQuery({
    queryKey: queryKeys.firstRunDone,
    queryFn: () => settingsGet(FIRST_RUN_KEY),
    // 首启开关只有向导完成时才写；避免路由切换反复读库。
    staleTime: Infinity,
  });
}

// ---------------------------------------------------------------------------
// mutations（统一失效库 / 部署相关查询）
// ---------------------------------------------------------------------------

function useInvalidatingMutation<TVars>(
  fn: (vars: TVars) => Promise<unknown>,
): UseMutationResult<unknown, Error, TVars> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.library });
      void qc.invalidateQueries({ queryKey: queryKeys.targets });
    },
  });
}

export function useInstallLocal() {
  return useInvalidatingMutation<Parameters<typeof libraryInstallLocal>[0]>(libraryInstallLocal) as UseMutationResult<InstallOutcome, Error, Parameters<typeof libraryInstallLocal>[0]>;
}

export function useRemoveSkill() {
  return useInvalidatingMutation<Parameters<typeof libraryRemove>[0]>(libraryRemove);
}

export function useDeployApply() {
  return useInvalidatingMutation<Parameters<typeof deployApply>[0]>(deployApply);
}

export function useDeployRevoke() {
  return useInvalidatingMutation<Parameters<typeof deployRevoke>[0]>(deployRevoke);
}

export function useSetToolEnabled() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { toolId: string; enabled: boolean }) =>
      toolsSetEnabled({ ...vars, dryRun: false }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.tools });
    },
  });
}

export function useCompleteFirstRun() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => settingsSet({ key: FIRST_RUN_KEY, value: "true", dryRun: false }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.firstRunDone });
      void qc.invalidateQueries({ queryKey: queryKeys.library });
      void qc.invalidateQueries({ queryKey: queryKeys.targets });
    },
  });
}

export function useOpenPath() {
  return useMutation({ mutationFn: (id: string) => systemOpenPath(id) });
}

// ---------------------------------------------------------------------------
// 体检：doctor 只读查询 + 逐项修复（dry-run 预览 → 确认执行）
// ---------------------------------------------------------------------------

export function useDoctorReport() {
  return useQuery({ queryKey: queryKeys.doctor, queryFn: doctorRun });
}

export function useDoctorFix() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { items: string[]; confirmed: boolean; dryRun: boolean }) => doctorFix(vars),
    onSuccess: (_data, vars) => {
      // 真实执行（非 dry-run）才失效库/部署/工具；dry-run 零写入无需刷新。
      if (!vars.dryRun) {
        void qc.invalidateQueries({ queryKey: queryKeys.library });
        void qc.invalidateQueries({ queryKey: queryKeys.targets });
        void qc.invalidateQueries({ queryKey: queryKeys.tools });
      }
    },
  });
}

// ---------------------------------------------------------------------------
// 详情页：SKILL.md 预览 + 更新检查/应用
// ---------------------------------------------------------------------------

export function useSkillDoc(skillId: number) {
  return useQuery({
    queryKey: queryKeys.skillDoc(skillId),
    queryFn: () => libraryDocRead(skillId),
    enabled: Number.isFinite(skillId) && skillId > 0,
  });
}

/// 编辑器实时校验（§9.3）：对任意文本做静态检查，零写盘。
/// 高频调用（防抖后仍每键级），不走 react-query 缓存，页面自管节流。
export function docValidate(skillId: number, contents: string): Promise<DocCheckReport> {
  return libraryDocValidate(skillId, contents);
}

/// 保存 SKILL.md：后端先校验（错误拒绝）→ 写 .bak → 写入 → 单 skill
/// 增量重扫；成功后统一失效预览 / 库 / 安全扫描 / 成本估算。
export function useDocSave() {
  const qc = useQueryClient();
  return useMutation<DocWriteOutcome, Error, { skillId: number; contents: string }>({
    mutationFn: (vars) => libraryDocWrite({ ...vars, dryRun: false }),
    onSuccess: (_data, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.skillDoc(vars.skillId) });
      void qc.invalidateQueries({ queryKey: queryKeys.library });
      void qc.invalidateQueries({ queryKey: queryKeys.security(vars.skillId) });
      void qc.invalidateQueries({ queryKey: queryKeys.securityOverview });
      void qc.invalidateQueries({ queryKey: queryKeys.cost });
    },
  });
}

export function useUpdateCheck() {
  return useMutation<UpdateCheckOutcome, Error, number>({
    mutationFn: (skillId) => updatesCheck(skillId),
  });
}

export function useUpdateApply() {
  const qc = useQueryClient();
  return useMutation<UpdateOutcome, Error, { skillId: number; dryRun: boolean }>({
    mutationFn: ({ skillId, dryRun }) => updatesApply(skillId, dryRun),
    onSuccess: (_data, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.skillDoc(vars.skillId) });
      void qc.invalidateQueries({ queryKey: queryKeys.library });
      void qc.invalidateQueries({ queryKey: queryKeys.targets });
    },
  });
}

// ---------------------------------------------------------------------------
// 专业面板（§9）：安全扫描（只提示不阻断）与上下文成本
// ---------------------------------------------------------------------------

/// 单个 skill 的安全扫描：打开卡片 / 详情页时懒加载触发，
/// TanStack 缓存 + 后端缓存双保险，重复访问零额外开销。
export function useSecurityReport(skillId: number) {
  return useQuery({
    queryKey: queryKeys.security(skillId),
    queryFn: () => securityScan(skillId),
    // 扫描是纯静态读取；会话内短时间内视为最新。
    staleTime: 5 * 60_000,
    enabled: Number.isFinite(skillId) && skillId > 0,
  });
}

/// 全库安全概览（工具箱顶部徽章）。
export function useSecurityOverview() {
  return useQuery({
    queryKey: queryKeys.securityOverview,
    queryFn: securityReport,
    // 部署 / 删除 / 更新会改变 skill 集合，统一在 mutation 成功后失效。
    staleTime: 60_000,
  });
}

/// 上下文成本报告（§9.2；估算值）。
export function useCostReport() {
  return useQuery({
    queryKey: queryKeys.cost,
    queryFn: costReport,
    staleTime: 60_000,
  });
}
