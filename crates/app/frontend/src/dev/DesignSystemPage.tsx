import { useState, type ReactNode } from "react";
import HighlightBar from "../components/HighlightBar";
import {
  Badge,
  Button,
  Card,
  Drawer,
  EmptyState,
  Modal,
  ProgressBar,
  Skeleton,
  Tag,
  Toggle,
  Tooltip,
} from "../components/ui";

interface Swatch {
  name: string;
  hex: string;
  cssVar: string;
  light?: boolean;
}

const palette: Swatch[] = [
  { name: "主色蓝", hex: "#3B8EF0", cssVar: "--sb-primary" },
  { name: "主色蓝 hover", hex: "#2F7FD8", cssVar: "--sb-primary-hover" },
  { name: "主色蓝 按下", hex: "#256FC4", cssVar: "--sb-primary-active" },
  { name: "淡蓝底（选中/标签）", hex: "#E8F2FF", cssVar: "--sb-primary-soft", light: true },
  { name: "高光青", hex: "#56C8F0", cssVar: "--sb-highlight" },
  { name: "页面底色", hex: "#FFFFFF", cssVar: "--sb-page", light: true },
  { name: "内容区", hex: "#F6F8FB", cssVar: "--sb-surface", light: true },
  { name: "边框", hex: "#E3E9F0", cssVar: "--sb-line", light: true },
  { name: "文本 主", hex: "#1B2430", cssVar: "--sb-ink" },
  { name: "文本 次", hex: "#5A6675", cssVar: "--sb-ink-secondary" },
  { name: "文本 弱", hex: "#8A94A3", cssVar: "--sb-ink-faint" },
  { name: "危险", hex: "#E5544B", cssVar: "--sb-danger" },
  { name: "成功", hex: "#2FA46A", cssVar: "--sb-success" },
];

function Section({ title, note, children }: { title: string; note?: string; children: ReactNode }) {
  return (
    <section className="mb-8">
      <h2 className="mb-1 text-base font-semibold text-ink">{title}</h2>
      {note && <p className="mb-3 text-xs leading-5 text-ink-faint">{note}</p>}
      <div className="space-y-3">{children}</div>
    </section>
  );
}

export default function DesignSystemPage() {
  const [toggleOn, setToggleOn] = useState(true);
  const [toggleOff, setToggleOff] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [progress, setProgress] = useState(60);
  const [tags, setTags] = useState(["cli", "git", "文档"]);

  return (
    <div className="flex h-screen flex-col bg-surface">
      <HighlightBar />
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[880px] px-6 py-8">
          <h1 className="text-lg font-semibold text-ink">设计系统 · §11.1 / §11.2</h1>
          <p className="mt-1 text-xs leading-5 text-ink-faint">
            PlainCraftLauncher2 式简洁白蓝。正文 14px；圆角 卡片 10px / 按钮 8px / 标签 999px；
            动画只动 transform 与 opacity，微交互 120ms、弹层 200ms、cubic-bezier(.2,.8,.2,1)。
          </p>

          <Section title="色板" note="全部色值以 index.css :root 的 CSS 变量为正本，Tailwind theme 经 var() 引用。">
            <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
              {palette.map((s) => (
                <Card key={s.cssVar} interactive={false} className="overflow-hidden">
                  <div
                    className="h-10 w-full border-b border-line"
                    style={{ background: s.hex }}
                  />
                  <div className="px-2.5 py-2">
                    <div className="text-xs font-medium text-ink">{s.name}</div>
                    <div className={`font-mono text-[11px] ${s.light ? "text-ink-secondary" : "text-ink-faint"}`}>
                      {s.hex}
                    </div>
                  </div>
                </Card>
              ))}
            </div>
          </Section>

          <Section title="排版">
            <Card interactive={false} className="space-y-2 p-5">
              <div className="text-lg font-semibold text-ink">页面标题 18px semibold</div>
              <div className="text-base font-medium text-ink">小节标题 16px medium</div>
              <div className="text-sm text-ink">正文 14px —— -apple-system / Segoe UI / Microsoft YaHei / PingFang SC</div>
              <div className="text-sm text-ink-secondary">次要文本 14px —— #5A6675</div>
              <div className="text-xs text-ink-faint">弱化说明 12px —— #8A94A3</div>
            </Card>
          </Section>

          <Section title="顶部高光条" note="2px 蓝青渐变，6s 循环横向流动，纯 CSS（translateX）。">
            <Card interactive={false} className="overflow-hidden p-0">
              <HighlightBar />
              <div className="px-5 py-4 text-sm text-ink-secondary">
                页面顶部常驻同款；prefers-reduced-motion 时静止。
              </div>
            </Card>
          </Section>

          <Section title="卡片两种状态" note="默认态 vs 悬浮态（translateY(-2px) + 深影 + conic-gradient 蓝色描边）并排。">
            <div className="grid grid-cols-2 gap-4">
              <Card className="p-5">
                <div className="text-sm font-medium text-ink">默认态</div>
                <p className="mt-1 text-xs leading-5 text-ink-faint">
                  白底 · 圆角 10px · 1px 边框 #E3E9F0 · 阴影 0 1px 3px rgba(16,40,80,.06)
                </p>
              </Card>
              <Card interactive={false} className="sb-card--lifted p-5">
                <div className="text-sm font-medium text-ink">悬浮态（强制展示）</div>
                <p className="mt-1 text-xs leading-5 text-ink-faint">
                  translateY(-2px) + 阴影加深 + 一圈极淡 conic-gradient 蓝色描边
                </p>
              </Card>
            </div>
          </Section>

          <Section title="Button" note="四变体 × 两尺寸；按压 scale(.97)，120ms。">
            <Card interactive={false} className="space-y-3 p-5">
              <div className="flex flex-wrap items-center gap-2">
                <Button variant="primary">主操作</Button>
                <Button variant="secondary">次操作</Button>
                <Button variant="ghost">幽灵</Button>
                <Button variant="danger">危险操作</Button>
                <Button variant="primary" disabled>禁用</Button>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <Button variant="primary" size="sm">小号主操作</Button>
                <Button variant="secondary" size="sm">小号次操作</Button>
                <Button variant="ghost" size="sm">小号幽灵</Button>
                <Button variant="danger" size="sm">小号危险</Button>
              </div>
            </Card>
          </Section>

          <Section title="Badge / Tag" note="Badge：状态徽章（带可选圆点）；Tag：标签（999px 胶囊，可移除）。">
            <Card interactive={false} className="space-y-3 p-5">
              <div className="flex flex-wrap items-center gap-2">
                <Badge tone="blue" dot>已部署</Badge>
                <Badge tone="neutral">未部署</Badge>
                <Badge tone="success" dot>健康</Badge>
                <Badge tone="danger" dot>高风险</Badge>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {tags.map((t) => (
                  <Tag key={t} onRemove={() => setTags(tags.filter((x) => x !== t))}>{t}</Tag>
                ))}
                {tags.length === 0 && <span className="text-xs text-ink-faint">标签已全部移除</span>}
                <Button size="sm" variant="ghost" onClick={() => setTags(["cli", "git", "文档"])}>重置</Button>
              </div>
            </Card>
          </Section>

          <Section title="Toggle">
            <Card interactive={false} className="flex flex-wrap items-center gap-6 p-5">
              <Toggle checked={toggleOn} onChange={setToggleOn} label="启用扫描缓存" />
              <Toggle checked={toggleOff} onChange={setToggleOff} label="默认关闭" />
              <Toggle checked={true} onChange={() => {}} disabled label="禁用态" />
            </Card>
          </Section>

          <Section title="Modal / Drawer" note="backdrop-filter: blur(8px) 玻璃拟态；进出 200ms（opacity + transform）。">
            <Card interactive={false} className="flex flex-wrap gap-2 p-5">
              <Button variant="primary" onClick={() => setModalOpen(true)}>打开弹层</Button>
              <Button variant="primary" onClick={() => setDrawerOpen(true)}>打开抽屉</Button>
            </Card>
          </Section>

          <Section title="Tooltip" note="纯 CSS group-hover，120ms opacity。">
            <Card interactive={false} className="p-5">
              <Tooltip content="部署到 Claude Code">
                <Button variant="secondary">悬停我</Button>
              </Tooltip>
            </Card>
          </Section>

          <Section title="EmptyState">
            <Card interactive={false} className="p-2">
              <EmptyState
                title="还没有任何技能"
                description="先运行一次扫描，把各 agent 目录里已有的 skills 收进中央库。"
                action={<Button variant="primary" size="sm">开始扫描</Button>}
              />
            </Card>
          </Section>

          <Section title="ProgressBar" note="fill 用 scaleX(transform) 表达进度，不动画 width。">
            <Card interactive={false} className="space-y-4 p-5">
              <div className="flex items-center gap-3">
                <ProgressBar value={progress} className="flex-1" />
                <span className="w-10 text-right font-mono text-xs text-ink-secondary">{progress}%</span>
                <Button size="sm" onClick={() => setProgress(Math.min(100, progress + 10))}>+10</Button>
              </div>
              <ProgressBar value={100} tone="success" />
              <ProgressBar value={30} tone="danger" />
            </Card>
          </Section>

          <Section title="Skeleton" note="sb-pulse 呼吸（opacity 1.6s）。">
            <Card interactive={false} className="w-full max-w-[320px] space-y-2.5 p-5">
              <div className="flex items-center gap-3">
                <Skeleton circle className="h-9 w-9 shrink-0" />
                <div className="flex-1 space-y-1.5">
                  <Skeleton className="h-3.5 w-2/3" />
                  <Skeleton className="h-3 w-1/3" />
                </div>
              </div>
              <Skeleton className="h-3 w-full" />
              <Skeleton className="h-3 w-5/6" />
              <Skeleton className="h-3 w-2/3" />
            </Card>
          </Section>

          <Section title="错峰 fade-up 列表" note="列表项间隔 20ms 依次进入（--sb-i），最多 10 项后同时出现。">
            <Card interactive={false} className="divide-y divide-line p-0">
              {Array.from({ length: 10 }, (_, i) => (
                <div
                  key={i}
                  className="sb-fade-up flex items-center justify-between px-5 py-2.5"
                  style={{ "--sb-i": i } as React.CSSProperties}
                >
                  <span className="text-sm text-ink">列表项 {i + 1}</span>
                  <Badge tone={i % 3 === 0 ? "blue" : "neutral"}>{i % 3 === 0 ? "git" : "local"}</Badge>
                </div>
              ))}
            </Card>
          </Section>
        </div>
      </div>

      <Modal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        title="删除技能"
        footer={
          <>
            <Button onClick={() => setModalOpen(false)}>取消</Button>
            <Button variant="danger" onClick={() => setModalOpen(false)}>移入回收站</Button>
          </>
        }
      >
        确定要把 <span className="font-medium text-ink">web-search</span> 移入回收站吗？
        所有 agent 内的部署链接会一并撤销，可在回收站中恢复。
      </Modal>

      <Drawer open={drawerOpen} onClose={() => setDrawerOpen(false)} title="技能详情">
        <div className="space-y-3">
          <div className="text-sm font-medium text-ink">web-search</div>
          <p>让 agent 联网检索并引用来源的技能。</p>
          <div className="flex gap-2">
            <Badge tone="blue" dot>git</Badge>
            <Badge tone="success" dot>已部署 ×3</Badge>
          </div>
        </div>
      </Drawer>
    </div>
  );
}
