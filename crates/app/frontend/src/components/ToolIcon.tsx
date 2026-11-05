/// 工具品牌图标（REVIEW #4）：优先渲染打包的品牌图标资源
///（来源 skills-manager public/agent-icons，MIT License），
/// 资产缺失时回退为按 id 着色的首字母色块。

const ASSETS: Record<string, string> = {
  adal: "/agent-icons/adal.png",
  amp: "/agent-icons/amp.svg",
  antigravity: "/agent-icons/antigravity.png",
  augment: "/agent-icons/augment.svg",
  bob: "/agent-icons/bob.png",
  claude_code: "/agent-icons/claude_code.svg",
  cline: "/agent-icons/cline.png",
  codebuddy: "/agent-icons/codebuddy.svg",
  codex: "/agent-icons/codex.svg",
  command_code: "/agent-icons/command_code.svg",
  continue: "/agent-icons/continue.png",
  cortex: "/agent-icons/cortex.png",
  crush: "/agent-icons/crush.png",
  cursor: "/agent-icons/cursor.png",
  deepagents: "/agent-icons/deepagents.png",
  droid: "/agent-icons/droid.svg",
  firebender: "/agent-icons/firebender.svg",
  gemini_cli: "/agent-icons/gemini_cli.svg",
  github_copilot: "/agent-icons/github_copilot.png",
  goose: "/agent-icons/goose.png",
  grok: "/agent-icons/grok.svg",
  hermes: "/agent-icons/hermes.png",
  iflow: "/agent-icons/iflow.png",
  junie: "/agent-icons/junie.png",
  kilo_code: "/agent-icons/kilo_code.svg",
  kimi: "/agent-icons/kimi.svg",
  kiro: "/agent-icons/kiro.svg",
  kode: "/agent-icons/kode.png",
  mcpjam: "/agent-icons/mcpjam.png",
  mistral_vibe: "/agent-icons/mistral_vibe.svg",
  mux: "/agent-icons/mux.png",
  neovate: "/agent-icons/neovate.png",
  oh_my_pi: "/agent-icons/oh_my_pi.svg",
  openclaw: "/agent-icons/openclaw.svg",
  opencode: "/agent-icons/opencode.png",
  openhands: "/agent-icons/openhands.png",
  pi: "/agent-icons/pi.svg",
  pochi: "/agent-icons/pochi.png",
  qoder: "/agent-icons/qoder.svg",
  qwen_code: "/agent-icons/qwen_code.png",
  replit: "/agent-icons/replit.png",
  roo_code: "/agent-icons/roo_code.svg",
  trae: "/agent-icons/trae.svg",
  warp: "/agent-icons/warp.svg",
  windsurf: "/agent-icons/windsurf.svg",
  zencoder: "/agent-icons/zencoder.png",
};

/// 图标 id → 资产 id 别名（合并声明工具共用品牌资产）。
const ALIASES: Record<string, string> = {
  omp_agent: "oh_my_pi",
};

function assetOf(icon: string): string | undefined {
  const key = ALIASES[icon] ?? icon;
  return ASSETS[key];
}

/// 字母回退头像的确定性色相（同 id 恒定，避免列表刷新跳色）。
function hueOf(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) {
    h = (h * 31 + id.charCodeAt(i)) % 360;
  }
  return h;
}

interface ToolIconProps {
  /** 工具图标 id（适配表 `icon` 字段） */
  icon: string;
  /** 展示名（回退头像取首字母） */
  name: string;
  /** 边长（px），默认 32 */
  size?: number;
  /** 弱化样式（未检测区） */
  muted?: boolean;
}

export default function ToolIcon({ icon, name, size = 32, muted = false }: ToolIconProps) {
  const asset = assetOf(icon);
  if (asset) {
    return (
      <img
        src={asset}
        alt=""
        draggable={false}
        width={size}
        height={size}
        style={{ width: size, height: size }}
        className={`shrink-0 rounded-[8px] object-contain ${muted ? "opacity-70" : ""}`}
      />
    );
  }
  const hue = hueOf(icon || name);
  return (
    <span
      aria-hidden
      style={{
        width: size,
        height: size,
        background: muted ? undefined : `hsl(${hue} 70% 92%)`,
        color: `hsl(${hue} 55% ${muted ? 55 : 38}%)`,
      }}
      className={`flex shrink-0 select-none items-center justify-center rounded-[8px] text-sm font-semibold ${
        muted ? "bg-surface" : ""
      }`}
    >
      {name.slice(0, 1).toUpperCase()}
    </span>
  );
}
