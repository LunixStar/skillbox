// 生成 1024x1024 应用图标源图（蓝→青渐变圆角方块，零依赖：仅 Node 内置 zlib）。
// 产物 tools/icon-1024.png 交给 `npx tauri icon` 生成全套平台图标。
"use strict";
const zlib = require("node:zlib");
const fs = require("node:fs");
const path = require("node:path");

const W = 1024;
const H = 1024;
const R = 200; // 圆角半径

// ---- CRC32 ----
const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

// ---- 像素 ----
const raw = Buffer.alloc(H * (1 + W * 4));
for (let y = 0; y < H; y++) {
  const rowStart = y * (1 + W * 4);
  raw[rowStart] = 0; // PNG filter: None
  for (let x = 0; x < W; x++) {
    const off = rowStart + 1 + x * 4;
    const cx = Math.min(Math.max(x, R), W - R);
    const cy = Math.min(Math.max(y, R), H - R);
    const dx = x - cx;
    const dy = y - cy;
    const inside = dx * dx + dy * dy <= R * R;
    if (inside) {
      // 品牌渐变 #3B8EF0 -> #56C8F0（对角线方向）
      const t = (x + y) / (W + H);
      raw[off] = Math.round(0x3b + (0x56 - 0x3b) * t);
      raw[off + 1] = Math.round(0x8e + (0xc8 - 0x8e) * t);
      raw[off + 2] = Math.round(0xf0 + (0xf0 - 0xf0) * t);
      raw[off + 3] = 255;
    } else {
      raw[off] = 0;
      raw[off + 1] = 0;
      raw[off + 2] = 0;
      raw[off + 3] = 0;
    }
  }
}

// ---- PNG 组包 ----
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const out = path.join(__dirname, "icon-1024.png");
fs.writeFileSync(out, png);
console.log(`written: ${out} (${png.length} bytes)`);
