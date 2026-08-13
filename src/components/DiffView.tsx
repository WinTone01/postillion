import { useState } from "react";
import { ChevronDownIcon, FilePlus2Icon, FilePenLineIcon } from "lucide-react";

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";
import { baseName, type DiffResult } from "@/lib/diff";

interface Props {
  filePath: string;
  diff: DiffResult;
  isNewFile: boolean;
  /** Araç hâlâ çalışıyorsa başlıkta belirtiliyor. */
  pending?: boolean;
  /** "Önceki" hâl bilinmiyor — tüm satırlar ekleme olarak gösteriliyor. */
  baselineUnknown?: boolean;
}

/**
 * Kod değişikliğini açılır diff olarak gösterir.
 *
 * Ham JSON yerine bu: Edit çağrılarının girdisi `old_string`/`new_string`
 * olduğu için JSON görünümünde neyin değiştiğini okumak imkansızdı.
 */
export default function DiffView({
  filePath,
  diff,
  isNewFile,
  pending,
  baselineUnknown,
}: Props) {
  // Küçük değişiklikler açık başlasın; büyükleri kapalı, listeyi boğmasın.
  const [open, setOpen] = useState(diff.rows.length <= 24);

  const Icon = isNewFile ? FilePlus2Icon : FilePenLineIcon;
  // Üç farklı durum, üç farklı dürüst etiket.
  const action = baselineUnknown ? "Wrote" : isNewFile ? "Created" : "Updated";

  return (
    <Collapsible
      className="not-prose mb-3 w-full overflow-hidden rounded-xl border bg-card"
      onOpenChange={setOpen}
      open={open}
    >
      <CollapsibleTrigger className="flex w-full items-center gap-2 px-3 py-2.5 text-left transition-colors hover:bg-accent/40">
        <Icon className="size-4 shrink-0 text-muted-foreground" />

        <span className="text-muted-foreground text-sm">{action}</span>
        <span className="truncate font-medium text-sm">{baseName(filePath)}</span>

        {/* Ekleme/silme sayacı — dosya adının hemen yanında, git alışkanlığı. */}
        <span className="shrink-0 font-mono text-xs tabular-nums">
          {diff.additions > 0 && (
            <span className="text-success">+{diff.additions}</span>
          )}
          {diff.additions > 0 && diff.deletions > 0 && " "}
          {diff.deletions > 0 && (
            <span className="text-destructive">-{diff.deletions}</span>
          )}
        </span>

        {pending && (
          <span className="shrink-0 rounded-full bg-primary/10 px-2 py-0.5 text-[10px] text-primary">
            çalışıyor
          </span>
        )}

        <ChevronDownIcon
          className={cn(
            "ml-auto size-4 shrink-0 text-muted-foreground transition-transform",
            open && "rotate-180",
          )}
        />
      </CollapsibleTrigger>

      <CollapsibleContent>
        <div className="border-t">
          <p className="truncate bg-muted/40 px-3 py-1.5 font-mono text-[11px] text-muted-foreground">
            {filePath}
          </p>

          <div className="max-h-[420px] overflow-auto">
            <table className="w-full border-collapse font-mono text-[12px] leading-[1.55]">
              <tbody>
                {diff.rows.map((row, index) => (
                  <tr
                    className={cn(
                      row.type === "add" && "bg-success/12",
                      row.type === "remove" && "bg-destructive/12",
                    )}
                    key={index}
                  >
                    {/* Satır numaraları seçilemez: kopyalarken koda karışmasın. */}
                    <td className="w-10 select-none border-r px-2 text-right text-[10px] text-muted-foreground/70 tabular-nums">
                      {row.oldLine ?? ""}
                    </td>
                    <td className="w-10 select-none border-r px-2 text-right text-[10px] text-muted-foreground/70 tabular-nums">
                      {row.newLine ?? ""}
                    </td>
                    <td
                      className={cn(
                        "w-4 select-none pl-2 text-center",
                        row.type === "add" && "text-success",
                        row.type === "remove" && "text-destructive",
                      )}
                    >
                      {row.type === "add" ? "+" : row.type === "remove" ? "-" : ""}
                    </td>
                    <td className="whitespace-pre-wrap break-all py-px pr-3 pl-1">
                      {row.text || " "}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {baselineUnknown && (
            <p className="border-t bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
              Bu yazım tamamlandığı için dosyanın önceki hâli geri getirilemiyor;
              tüm satırlar yeni olarak gösteriliyor.
            </p>
          )}

          {diff.truncated && (
            <p className="border-t bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
              Değişiklik çok büyük; satır eşleştirme atlandı, bloklar olduğu gibi
              gösteriliyor.
            </p>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
