import { Construction, Lock } from "lucide-react";
import type { Feature } from "../bindings/Feature";
import { featureKey, useT, type MessageKey } from "../lib/i18n";
import { useActiveKeyboard } from "../lib/store";

/** Placeholder for sections whose protocol is still being verified. */
export function ComingSoon({ title, features }: { title: MessageKey; features: Feature[] }) {
  const t = useT();
  const kb = useActiveKeyboard();
  const locked = kb ? features.filter((f) => !kb.features.includes(f)) : features;
  return (
    <div className="flex flex-col gap-6">
      <header>
        <div className="eyebrow mb-2">{t("soon.title")}</div>
        <h1 className="font-display text-[40px] leading-none font-semibold tracking-[-0.035em]">{t(title)}</h1>
      </header>
      <section className="card flex max-w-[720px] gap-5 p-6">
        <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl bg-surface-2 text-signal">
          <Construction size={20} />
        </div>
        <div className="flex flex-col gap-4">
          <p className="text-[13.5px] text-ink-2">{t("soon.body")}</p>
          <ul className="flex flex-wrap gap-2">
            {features.map((f) => (
              <li key={f} className="flex items-center gap-1.5 rounded-full border border-line px-3 py-1 text-[12px]">
                {locked.includes(f) && <Lock size={11} className="text-ink-3" />}
                {t(featureKey(f))}
              </li>
            ))}
          </ul>
          {locked.length > 0 && <div className="num text-[11.5px] text-ink-3">{t("soon.needsUsb")}</div>}
        </div>
      </section>
    </div>
  );
}
