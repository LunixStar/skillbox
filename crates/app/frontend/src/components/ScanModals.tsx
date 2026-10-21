/// 技能库扫描对话框（结果 / 运行中）：从 LibraryPage 拆出以守住 600 行上限。

import type { ScanProgress, ScanReport } from "../api/ipc";
import { useI18n } from "../i18n";
import { Button, EmptyState, Modal } from "./ui";

interface ScanResultModalProps {
  report: ScanReport;
  /// 已入库（library + targets 去重后）仍未登记的发现目录。
  unregistered: ScanReport["skills"];
  onClose: () => void;
  onRegister: (path: string, name: string | null) => void;
}

export function ScanResultModal({ report, unregistered, onClose, onRegister }: ScanResultModalProps) {
  const { t } = useI18n();
  return (
    <Modal open onClose={onClose} title={t("library.scanDoneTitle")} footer={<Button variant="primary" onClick={onClose}>{t("common.close")}</Button>}>
      <p>
        {t("library.scanDoneBody", {
          total: report.skills.length,
          known: report.skills.length - unregistered.length,
        })}
        {report.skills.length - unregistered.length > 0 && t("library.scanDoneKnownNote")}
      </p>
      {unregistered.length > 0 && (
        <div className="mt-3 space-y-1.5">
          <p className="text-xs text-ink-faint">{t("library.scanDoneUnregistered")}</p>
          <div className="max-h-44 space-y-1 overflow-y-auto">
            {unregistered.map((s) => (
              <div key={s.path} className="flex items-center gap-2 rounded-[8px] bg-surface px-2.5 py-1.5">
                <span className="min-w-0 flex-1 truncate text-xs text-ink">{s.name ?? s.dirName}</span>
                <Button size="sm" onClick={() => onRegister(s.path, s.name)}>
                  {t("library.register")}
                </Button>
              </div>
            ))}
          </div>
        </div>
      )}
    </Modal>
  );
}

export function ScanRunningModal({ live, onClose }: { live: ScanProgress | null; onClose: () => void }) {
  const { t } = useI18n();
  return (
    <Modal open onClose={onClose} title={t("library.scanRunningTitle")}>
      <div className="sb-pulse h-1.5 w-full overflow-hidden rounded-full bg-line">
        <div className="h-full w-1/3 rounded-full bg-primary" />
      </div>
      <p className="mt-3">
        {live
          ? t("library.scanRunningProgress", { scanned: live.scanned, found: live.found })
          : t("library.scanStarting")}
      </p>
    </Modal>
  );
}

/// 标签过滤弹层（v1：skill_tags 暂无写入来源，诚实空态）。
export function TagFilterModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useI18n();
  return (
    <Modal open={open} onClose={onClose} title={t("library.tagFilterTitle")}>
      <EmptyState title={t("library.noTagsTitle")} description={t("library.noTagsDesc")} />
    </Modal>
  );
}
