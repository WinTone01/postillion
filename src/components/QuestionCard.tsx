import { useMemo, useState } from "react";
import { CheckIcon, MessageCircleQuestionIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

export interface QuestionOption {
  label: string;
  description?: string;
}

export interface Question {
  question: string;
  header?: string;
  multiSelect?: boolean;
  options: QuestionOption[];
}

interface Props {
  questions: Question[];
  /** Cevaplanmışsa gösterilecek özet; kart salt okunur olur. */
  answered?: string | null;
  onSubmit: (answers: Record<string, string>) => void;
}

/** Serbest metin seçeneği; Claude Code'un kendi arayüzünde de var. */
const OTHER = "__other__";

/**
 * `AskUserQuestion` için seçim arayüzü.
 *
 * Araç bize `can_use_tool` kontrol isteği olarak geliyor ve sorular girdinin
 * içinde. Cevap, kontrol cevabının `message` alanıyla geri gönderiliyor —
 * headless modda tool_result'a ulaşan tek kanal o (ölçüldü).
 */
export default function QuestionCard({ questions, answered, onSubmit }: Props) {
  const [picked, setPicked] = useState<Record<number, Set<string>>>({});
  const [custom, setCustom] = useState<Record<number, string>>({});

  /**
   * Gönderildikten sonra kart kendi kendine kapanır.
   *
   * Yukarıdaki durum da güncelleniyor ama ona bağlı kalmak kartı iki kez
   * gönderilebilir bırakıyordu; karar burada da tutuluyor.
   */
  const [sent, setSent] = useState<string | null>(null);

  const complete = useMemo(
    () =>
      questions.every((_, i) => {
        const set = picked[i];
        if (!set || set.size === 0) return false;
        // "Diğer" seçiliyse metin de dolu olmalı.
        if (set.has(OTHER) && !(custom[i] ?? "").trim()) return false;
        return true;
      }),
    [questions, picked, custom],
  );

  function toggle(index: number, label: string, multi: boolean) {
    setPicked((prev) => {
      const current = new Set(prev[index] ?? []);
      if (multi) {
        if (current.has(label)) current.delete(label);
        else current.add(label);
      } else {
        current.clear();
        current.add(label);
      }
      return { ...prev, [index]: current };
    });
  }

  function submit() {
    if (sent !== null) return;

    const answers: Record<string, string> = {};
    questions.forEach((q, i) => {
      const labels = [...(picked[i] ?? [])].map((l) =>
        l === OTHER ? (custom[i] ?? "").trim() : l,
      );
      answers[q.question] = labels.filter(Boolean).join(",");
    });

    setSent(Object.values(answers).join(" · "));
    onSubmit(answers);
  }

  const settled = answered ?? sent;

  if (settled) {
    return (
      <div className="not-prose mb-4 rounded-xl border bg-card px-4 py-3">
        <div className="flex items-center gap-2 text-muted-foreground text-xs">
          <CheckIcon className="size-3.5 text-success" />
          {t("Cevaplandı")}
        </div>
        <p className="mt-1 text-sm">{settled}</p>
      </div>
    );
  }

  return (
    <div className="not-prose mb-4 space-y-4 rounded-xl border border-primary/30 bg-card p-4">
      <div className="flex items-center gap-2">
        <MessageCircleQuestionIcon className="size-4 text-primary" />
        <span className="font-medium text-sm">
          {questions.length > 1
            ? t("{n} soru", { n: questions.length })
            : t("Bir sorusu var")}
        </span>
      </div>

      {questions.map((q, i) => {
        const multi = q.multiSelect === true;
        const set = picked[i] ?? new Set<string>();

        return (
          <div className="space-y-2" key={i}>
            <div>
              {q.header && (
                <span className="mb-1 inline-block rounded bg-muted px-1.5 py-px text-[10px] text-muted-foreground uppercase tracking-wide">
                  {q.header}
                </span>
              )}
              <p className="font-medium text-sm">{q.question}</p>
              {multi && (
                <p className="text-[11px] text-muted-foreground">
                  {t("Birden fazla seçebilirsiniz")}
                </p>
              )}
            </div>

            <div className="space-y-1.5">
              {q.options.map((option) => {
                const active = set.has(option.label);
                return (
                  <button
                    className={cn(
                      "w-full rounded-lg border px-3 py-2 text-left transition-colors",
                      active
                        ? "border-primary bg-primary/10"
                        : "hover:border-foreground/25 hover:bg-accent/40",
                    )}
                    key={option.label}
                    onClick={() => toggle(i, option.label, multi)}
                    type="button"
                  >
                    <div className="flex items-start gap-2">
                      <span
                        className={cn(
                          "mt-0.5 grid size-4 shrink-0 place-items-center border",
                          multi ? "rounded" : "rounded-full",
                          active ? "border-primary bg-primary" : "border-muted-foreground/40",
                        )}
                      >
                        {active && (
                          <CheckIcon className="size-3 text-primary-foreground" />
                        )}
                      </span>
                      <div className="min-w-0">
                        <p className="font-medium text-sm">{option.label}</p>
                        {option.description && (
                          <p className="text-[11.5px] text-muted-foreground leading-snug">
                            {option.description}
                          </p>
                        )}
                      </div>
                    </div>
                  </button>
                );
              })}

              {/* Hazır seçenekler yetmiyorsa kullanıcı kendi cevabını yazabilmeli. */}
              <button
                className={cn(
                  "w-full rounded-lg border px-3 py-2 text-left transition-colors",
                  set.has(OTHER)
                    ? "border-primary bg-primary/10"
                    : "hover:border-foreground/25 hover:bg-accent/40",
                )}
                onClick={() => toggle(i, OTHER, multi)}
                type="button"
              >
                <span className="text-muted-foreground text-sm">{t("Başka…")}</span>
              </button>

              {set.has(OTHER) && (
                <Input
                  autoFocus
                  className="text-sm"
                  onChange={(e) => setCustom((prev) => ({ ...prev, [i]: e.target.value }))}
                  placeholder={t("Kendi cevabınızı yazın")}
                  value={custom[i] ?? ""}
                />
              )}
            </div>
          </div>
        );
      })}

      <Button disabled={!complete || sent !== null} onClick={submit} size="sm">
        {t("Gönder")}
      </Button>
    </div>
  );
}

/** Cevapları Claude'un beklediği metin biçimine çevirir. */
export function formatAnswers(answers: Record<string, string>): string {
  const parts = Object.entries(answers)
    .map(([question, answer]) => `"${question}"="${answer}"`)
    .join(", ");
  return `The user answered: ${parts}. Read the answers carefully — they may request clarification, changes, or that you not proceed — and follow what they actually say.`;
}
