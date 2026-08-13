/**
 * Satır bazlı diff — Edit/Write araç çağrılarını okunur biçimde göstermek için.
 *
 * Strateji: önce ortak baş ve sonu kırp, sonra kalan orta parçaya LCS uygula.
 * Kırpma önemli çünkü Claude'un düzenlemeleri genelde dosyanın küçük bir
 * bölümüne dokunuyor; ham LCS O(n·m) ve büyük dosyada arayüzü dondururdu.
 */

export type DiffRowType = "context" | "add" | "remove";

export interface DiffRow {
  type: DiffRowType;
  /** Eski dosyadaki satır numarası (eklemelerde yok). */
  oldLine?: number;
  /** Yeni dosyadaki satır numarası (silmelerde yok). */
  newLine?: number;
  text: string;
}

export interface DiffResult {
  rows: DiffRow[];
  additions: number;
  deletions: number;
  /** Çok büyük girdilerde LCS atlandı; satırlar blok halinde gösteriliyor. */
  truncated: boolean;
}

/** Bu eşiğin üstünde LCS yerine blok karşılaştırma kullanılıyor. */
const LCS_LIMIT = 600;

function splitLines(text: string): string[] {
  if (text === "") return [];
  // Sondaki tek newline sahte bir boş satır üretiyor.
  return text.replace(/\n$/, "").split("\n");
}

export function diffLines(oldText: string, newText: string): DiffResult {
  const oldLines = splitLines(oldText);
  const newLines = splitLines(newText);

  // Ortak baş.
  let start = 0;
  while (
    start < oldLines.length &&
    start < newLines.length &&
    oldLines[start] === newLines[start]
  ) {
    start++;
  }

  // Ortak son.
  let endOld = oldLines.length;
  let endNew = newLines.length;
  while (
    endOld > start &&
    endNew > start &&
    oldLines[endOld - 1] === newLines[endNew - 1]
  ) {
    endOld--;
    endNew--;
  }

  const midOld = oldLines.slice(start, endOld);
  const midNew = newLines.slice(start, endNew);

  const rows: DiffRow[] = [];
  let additions = 0;
  let deletions = 0;
  let truncated = false;

  // Değişimden önceki birkaç satır bağlam olarak gösteriliyor.
  const contextBefore = Math.max(0, start - 3);
  for (let i = contextBefore; i < start; i++) {
    rows.push({ type: "context", oldLine: i + 1, newLine: i + 1, text: oldLines[i] });
  }

  if (midOld.length * midNew.length > LCS_LIMIT * LCS_LIMIT) {
    // Çok büyük: satır satır eşleştirmeye çalışmadan blok göster.
    truncated = true;
    midOld.forEach((text, i) => {
      rows.push({ type: "remove", oldLine: start + i + 1, text });
      deletions++;
    });
    midNew.forEach((text, i) => {
      rows.push({ type: "add", newLine: start + i + 1, text });
      additions++;
    });
  } else {
    const ops = lcsDiff(midOld, midNew);
    let oldCursor = start;
    let newCursor = start;

    for (const op of ops) {
      if (op.type === "context") {
        rows.push({
          type: "context",
          oldLine: ++oldCursor,
          newLine: ++newCursor,
          text: op.text,
        });
      } else if (op.type === "remove") {
        rows.push({ type: "remove", oldLine: ++oldCursor, text: op.text });
        deletions++;
      } else {
        rows.push({ type: "add", newLine: ++newCursor, text: op.text });
        additions++;
      }
    }
  }

  // Değişimden sonraki birkaç satır.
  const contextAfter = Math.min(oldLines.length, endOld + 3);
  for (let i = endOld; i < contextAfter; i++) {
    rows.push({
      type: "context",
      oldLine: i + 1,
      newLine: i - endOld + endNew + 1,
      text: oldLines[i],
    });
  }

  return { rows, additions, deletions, truncated };
}

interface Op {
  type: DiffRowType;
  text: string;
}

/** Klasik LCS tablosu; yalnızca kırpılmış orta parçaya uygulanıyor. */
function lcsDiff(a: string[], b: string[]): Op[] {
  const n = a.length;
  const m = b.length;

  // (n+1)×(m+1) tablo.
  const table: number[][] = Array.from({ length: n + 1 }, () =>
    new Array<number>(m + 1).fill(0),
  );

  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i][j] =
        a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }

  const ops: Op[] = [];
  let i = 0;
  let j = 0;

  while (i < n && j < m) {
    if (a[i] === b[j]) {
      ops.push({ type: "context", text: a[i] });
      i++;
      j++;
    } else if (table[i + 1][j] >= table[i][j + 1]) {
      ops.push({ type: "remove", text: a[i] });
      i++;
    } else {
      ops.push({ type: "add", text: b[j] });
      j++;
    }
  }
  while (i < n) ops.push({ type: "remove", text: a[i++] });
  while (j < m) ops.push({ type: "add", text: b[j++] });

  return ops;
}

export interface ToolDiff {
  filePath: string;
  diff: DiffResult;
  isNewFile: boolean;
  /**
   * Karşılaştırılacak "önceki" hâl bilinmiyor.
   *
   * Tamamlanmış `Write` çağrılarında böyle: dosya çoktan üzerine yazıldığı için
   * diskteki içerik artık *sonraki* sürüm. Bu durumda tüm satırlar ekleme
   * olarak gösteriliyor ve arayüz bunu açıkça belirtiyor.
   */
  baselineUnknown: boolean;
}

/**
 * Araç girdisinden diff çıkarır; kod değiştirmeyen araçlarda `null`.
 *
 * `baseline` yalnızca `Write` için anlamlı ve yalnızca yazım henüz
 * gerçekleşmemişken doğru olur.
 */
export function diffFromToolInput(
  toolName: string,
  input: unknown,
  baseline?: { content: string | null; exists: boolean } | null,
): ToolDiff | null {
  if (!input || typeof input !== "object") return null;
  const record = input as Record<string, unknown>;

  const filePath = typeof record.file_path === "string" ? record.file_path : "";
  if (!filePath) return null;

  if (toolName === "Write" && typeof record.content === "string") {
    // Taban biliniyorsa gerçek diff; bilinmiyorsa tamamı ekleme.
    if (baseline) {
      return {
        filePath,
        diff: diffLines(baseline.content ?? "", record.content),
        isNewFile: !baseline.exists,
        baselineUnknown: false,
      };
    }

    return {
      filePath,
      diff: diffLines("", record.content),
      isNewFile: false,
      baselineUnknown: true,
    };
  }

  if (
    toolName === "Edit" &&
    typeof record.old_string === "string" &&
    typeof record.new_string === "string"
  ) {
    return {
      filePath,
      diff: diffLines(record.old_string, record.new_string),
      isNewFile: false,
      baselineUnknown: false,
    };
  }

  return null;
}

/** `/home/x/Projects/foo/src/a.ts` → `a.ts` */
export function baseName(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] || path;
}
