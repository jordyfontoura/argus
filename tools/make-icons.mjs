// Generates the app icons (a conic-gradient ring, the circle from the approved mock).
// Usage: node tools/make-icons.mjs   -> writes src-tauri/icons/{icon.ico,32x32.png,128x128.png,icon.png}
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "src-tauri", "icons");
mkdirSync(out, { recursive: true });

const STOPS = [
  [0x4c, 0xc9, 0xf0], // cpu
  [0xb3, 0x88, 0xff], // gpu
  [0x5e, 0xea, 0xd4], // mem
  [0x4c, 0xc9, 0xf0],
];

function ringColor(angle01) {
  const seg = angle01 * (STOPS.length - 1);
  const i = Math.min(Math.floor(seg), STOPS.length - 2);
  const t = seg - i;
  return STOPS[i].map((c, k) => Math.round(c + (STOPS[i + 1][k] - c) * t));
}

function render(size) {
  const px = Buffer.alloc(size * size * 4);
  const c = (size - 1) / 2;
  const outer = size * 0.46;
  const inner = size * 0.2;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const dx = x - c;
      const dy = y - c;
      const r = Math.hypot(dx, dy);
      // 1px smooth edge on both the outer and inner radius.
      const cover = Math.max(0, Math.min(1, outer - r + 0.5)) * Math.max(0, Math.min(1, r - inner + 0.5));
      const angle = (Math.atan2(dy, dx) + Math.PI) / (2 * Math.PI);
      const [R, G, B] = ringColor(angle);
      const o = (y * size + x) * 4;
      px[o] = R;
      px[o + 1] = G;
      px[o + 2] = B;
      px[o + 3] = Math.round(cover * 255);
    }
  }
  return px;
}

const CRC = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
function png(size) {
  const raw = render(size);
  const rows = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) {
    rows[y * (size * 4 + 1)] = 0;
    raw.copy(rows, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(rows)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function ico(sizes) {
  const images = sizes.map((s) => png(s));
  const head = Buffer.alloc(6 + 16 * sizes.length);
  head.writeUInt16LE(1, 2); // type: icon
  head.writeUInt16LE(sizes.length, 4);
  let offset = head.length;
  sizes.forEach((s, i) => {
    const e = 6 + 16 * i;
    head[e] = s >= 256 ? 0 : s;
    head[e + 1] = s >= 256 ? 0 : s;
    head.writeUInt16LE(1, e + 4); // planes
    head.writeUInt16LE(32, e + 6); // bpp
    head.writeUInt32LE(images[i].length, e + 8);
    head.writeUInt32LE(offset, e + 12);
    offset += images[i].length;
  });
  return Buffer.concat([head, ...images]);
}

writeFileSync(join(out, "32x32.png"), png(32));
writeFileSync(join(out, "128x128.png"), png(128));
writeFileSync(join(out, "icon.png"), png(256));
writeFileSync(join(out, "icon.ico"), ico([16, 32, 48, 64, 256]));
console.log("icons written to", out);
