/// i18n 一致性校验（§12 Phase 5 DoD：中英切换无漏翻）：
/// 1) zh-CN.json 与 en.json 的 key 集合必须完全一致（双向）；
/// 2) 源码中 t("...") 字面量 key 必须存在于词典；
/// 3) 词典中未被源码引用的 key 仅告警（供清理参考，不失败）。
/// 用法：node scripts/check-i18n.mjs（package.json: npm run check:i18n）

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const srcDir = join(root, "src");

function loadDict(name) {
  return JSON.parse(readFileSync(join(srcDir, "i18n", name), "utf8"));
}

function collectFiles(dir, out) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      collectFiles(full, out);
    } else if (/\.(tsx?|jsx?)$/.test(entry)) {
      out.push(full);
    }
  }
  return out;
}

const zh = loadDict("zh-CN.json");
const en = loadDict("en.json");
const zhKeys = new Set(Object.keys(zh));
const enKeys = new Set(Object.keys(en));

let failures = 0;

for (const k of zhKeys) {
  if (!enKeys.has(k)) {
    console.error(`[missing-en] ${k}`);
    failures += 1;
  }
}
for (const k of enKeys) {
  if (!zhKeys.has(k)) {
    console.error(`[missing-zh] ${k}`);
    failures += 1;
  }
}

// 源码引用扫描：t("key") / t('key')，忽略非常量/拼接调用。
const usedKeys = new Set();
const files = collectFiles(srcDir, []);
const tCall = /\bt\(\s*(["'])((?:[^\\'"\n]|\\.)*)\1/g;
for (const file of files) {
  const text = readFileSync(file, "utf8");
  let m;
  while ((m = tCall.exec(text)) !== null) {
    usedKeys.add(m[2]);
    if (!zhKeys.has(m[2])) {
      console.error(`[undefined-key] ${m[2]}  (${file.replace(root, "")})`);
      failures += 1;
    }
  }
}

for (const k of zhKeys) {
  if (!usedKeys.has(k)) {
    console.warn(`[unused-key] ${k}`);
  }
}

const pluralWarn = [];
console.log(
  `i18n check: zh=${zhKeys.size} en=${enKeys.size} used=${usedKeys.size} failures=${failures}`,
);
for (const w of pluralWarn) {
  console.warn(w);
}
process.exit(failures === 0 ? 0 : 1);
