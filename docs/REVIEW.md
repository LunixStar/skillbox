##v1.0 demo已发现的问题：
1.大量按钮UI出现显示问题，鼠标未悬停在按钮上方时，部分按钮直接“隐身”，轮廓与文字都直接变成背景色，用户不知道按钮在哪里
2.能够识别的agent和CLI实在太少，workbuddy、devecocode等agent完全无法识别，完全无法像skills-manager那样识别六七十种agent
3.点击“打开本地库”等按钮时，无法正确弹出文件资源管理器，严重业务逻辑失误
4.“工具”页面内各agent无图标，视觉上不舒服
5.当前体检的三个情况均为误报，需要根据本机实际情况排查问题原因
6.“技能库”页面技能条目的加载动画有误，只为卡片排列制作了动画，条目视图未制作动画，且从条目视图切换为卡片排列无动画
7.为了保证视觉的统一性，每一页都应当具有和“体检”页一样的1动画加载效果
8.左下角的“打开中央库”与设置中的打开中央库功能冗余，可以考虑删去左下角按钮

---

## 修复记录（v1.1，2026-11）

| # | 问题 | 根因 | 修复 | 状态 |
|---|------|------|------|------|
| 1 | 按钮悬停前“隐身” | `tailwind.config.cjs` 主题色误写为 `primary: {...}`，`DEFAULT` 缺失，`bg-primary`/`text-primary` 等全部失效 | `DEFAULT:` 恢复 | ✅ |
| 2 | 可识别 agent 太少（19 个） | v1.0 声明表仅 19 项，且 6 项路径为待复核猜测 | 以 skills-manager（MIT）56 代理表为基准扩至 56 个（CLI 31 / IDE 8 / 插件 8 / TUI 3 / 桌面助手 6）+ 本机实证 DevEco Code 双根；`.config/` 前缀双候选展开（字面 `~/.config` + 平台配置根）；重叠 6 工具改用 SM 实证路径。详见 `docs/adapters.md` | ✅ |
| 3 | “打开本地库”无法弹出资源管理器 | Windows 下 shell `open` 命令走不通 | 引入 `tauri-plugin-opener`（`open_path` / `open_url`） | ✅ |
| 4 | 工具页无图标 | v1.0 无图标资源 | 打包 46 个品牌图标（来自 skills-manager，`frontend/public/agent-icons/`）+ `ToolIcon` 组件（缺失时 hue 字母回退），工具页已检测/未检测两行均接入 | ✅ |
| 5 | 体检三项均误报 | `inspect_disk` 把点目录（`.git` 等）与无 SKILL.md 的目录当 skill 统计；reconcile 把未纳管真实 skill 目录判为 Warning | doctor 跳过点目录与无 SKILL.md 目录；reconcile 降为 Info | ✅ |
| 6 | 技能库条目视图无动画 | 三视图分支缺 key / 动画类 | 视图分支补 `view-grid` / `view-vlist` / `view-list` key 与对应动画 | ✅ |
| 7 | 各页缺少与体检页一致的入场动画 | 仅 DoctorPage / LibraryPage 有 `sb-fade-up` 阶梯动画 | Overview / Tools / Toolbox / Settings / SkillDetail 五页补齐 `sb-card sb-fade-up` + `--sb-i` 阶梯延迟 | ✅ |
| 8 | 左下角“打开中央库”与设置冗余 | 历史遗留 | 删除侧栏底部按钮 | ✅ |

---

## v1.1体验反馈
1.“工具”页内DevEco Code、WorkBuddy、Zcode、DeepSeek Harness、MiniMax Code、Zed、Gitlab Duo、EasyClaw、QClaw、AutoClaw等均无SVG图像，而Trae CN的图标也不是官方最新的LOGO
2.“技能库”页面的一键部署到其他模型的功能的交互逻辑太差，且ui超出卡片范围，需要更舒适有效的交互逻辑
3.应当将窗口上边的“最小化、全屏/退出全屏、关闭”栏目融入整个画面的渲染，去边框化