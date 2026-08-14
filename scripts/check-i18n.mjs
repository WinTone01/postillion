#!/usr/bin/env node
/**
 * Çeviri sözlüğünü kaynakla karşılaştırır.
 *
 * Anahtar olarak Türkçe kaynak metnin kendisi kullanılıyor (bkz. `i18n.ts`),
 * yani metni değiştirmek çeviriyi sessizce düşürür. Bu betik tam da onu
 * yakalıyor: kaynakta olup sözlükte olmayan ve sözlükte olup kaynakta
 * kullanılmayan anahtarlar.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const SRC = "src";
const DICT = "src/lib/i18n-en.ts";

/** `t("…")` çağrıları; satır sonu ve girinti araya girebiliyor. */
const CALL = /\bt\(\s*(?:\/\*[^]*?\*\/\s*)?"((?:[^"\\]|\\.)*)"/g;

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...walk(path));
    else if (/\.tsx?$/.test(path)) out.push(path);
  }
  return out;
}

const used = new Set();
for (const file of walk(SRC)) {
  // Sözlüğün ve modülün kendisi taranmıyor.
  if (file.includes("i18n")) continue;
  const source = readFileSync(file, "utf8");
  for (const match of source.matchAll(CALL)) used.add(match[1]);
}

// `i18n.ts` içindeki göreli zaman anahtarları da kullanımda sayılıyor.
const runtime = readFileSync("src/lib/i18n.ts", "utf8");
for (const match of runtime.matchAll(CALL)) used.add(match[1]);

const dict = readFileSync(DICT, "utf8");
const defined = new Set();
// Anahtar ya tırnaklı ya da düz bir tanımlayıcı olarak yazılabiliyor.
for (const match of dict.matchAll(/^\s{2}(?:"((?:[^"\\]|\\.)*)"|([\p{L}][\p{L}\p{N}_]*)):/gmu)) {
  defined.add(match[1] ?? match[2]);
}

const missing = [...used].filter((key) => !defined.has(key)).sort();
const unused = [...defined].filter((key) => !used.has(key)).sort();

for (const key of missing) console.log(`eksik   ${JSON.stringify(key)}`);
for (const key of unused) console.log(`fazla   ${JSON.stringify(key)}`);

console.log(
  `\n${used.size} anahtar kullanılıyor, ${defined.size} tanımlı — ` +
    `${missing.length} eksik, ${unused.length} fazla`,
);

process.exit(missing.length > 0 ? 1 : 0);
