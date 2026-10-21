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
  libraryDocRead,
  libraryInstallLocal,
  libraryList,
  libraryRemove,
  libraryTargets,
  settingsGet,
  settingsSet,
  systemOpenPath,
  toolsList,
  toolsSetEnabled,
  updatesApply,
  updatesCheck,
  type InstallOutcome,
  type UpdateCheckOutcome,
  type UpdateOutcome,
} from "./ipc";

export const queryKeys = {
  tools: ["tools"] as const,
  library: ["library"] as const,
  targets: ["targets"] as const,
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
// 详情页：SKILL.md 预览 + 更新检查/应用
// ---------------------------------------------------------------------------

export function useSkillDoc(skillId: number) {
  return useQuery({
    queryKey: queryKeys.skillDoc(skillId),
    queryFn: () => libraryDocRead(skillId),
    enabled: Number.isFinite(skillId) && skillId > 0,
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
