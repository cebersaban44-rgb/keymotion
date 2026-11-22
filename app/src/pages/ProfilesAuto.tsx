// "Uygulamaya göre" on the Profiles page: rules that link a program, or
// every game, to a profile. By default only a per-key design's lighting
// changes, in RAM (nothing is written); a rule can also write the key
// mapping and macros, a limited number of times a day
// (app/src-tauri/src/autoprofile.rs).

import { AppWindow, Gamepad2, Info, Plus, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { AutoBack } from "../bindings/AutoBack";
import type { AutoRule } from "../bindings/AutoRule";
import type { ProfileInfo } from "../bindings/ProfileInfo";
import type { ProfilesState } from "../bindings/ProfilesState";
import { Toggle } from "../components/controls";
import { ProgramIcon } from "../components/ProgramIcon";
import { RULE_GAMES, followAuto, useAuto } from "../lib/autoprofile";
import { useGames } from "../lib/games";
import { useT } from "../lib/i18n";
import { profileName } from "../lib/profiles";

export function AutoProfiles({ profiles, usb }: { profiles: ProfilesState | null; usb: boolean }) {
  const t = useT();
  const s = useAuto((x) => x.state);
  const configure = useAuto((x) => x.configure);
  const games = useGames((x) => x.state);
  useEffect(() => followAuto(), []);
  const all: ProfileInfo[] = useMemo(() => (profiles ? [...profiles.profiles, profiles.factory] : []), [profiles]);
  const [adding, setAdding] = useState<string>("");
  if (!s || !profiles) return null;

  const set = (patch: Parameters<typeof configure>[0]) => configure(patch).catch(console.error);
  const rules = s.settings.rules;
  const nameOf = (id: string | null) => {
    const p = all.find((x) => x.id === id);
    return p ? profileName(p, t) : t("auto.profile.gone");
  };
  const setRule = (path: string, patch: Partial<AutoRule>) =>
    set({ rules: rules.map((r) => (r.path === path ? { ...r, ...patch } : r)) });
  const firstProfile = profiles.profiles[0]?.id ?? profiles.factory.id;

  // Programs to add: every game, the games played, the programs used lately.
  const taken = new Set(rules.map((r) => r.path.toLowerCase()));
  const choices: { path: string; name: string; game: boolean }[] = [];
  const offer = (path: string, name: string, game: boolean) => {
    if (!taken.has(path.toLowerCase()) && !choices.some((c) => c.path.toLowerCase() === path.toLowerCase())) {
      choices.push({ path, name, game });
    }
  };
  offer(RULE_GAMES, t("auto.allGames"), true);
  games?.games.filter((g) => g.played).forEach((g) => offer(g.key, g.name, true));
  s.recent.forEach((p) => offer(p.path, p.name, false));

  const status = !s.settings.enabled
    ? t("auto.status.off")
    : s.settings.paused
      ? t("auto.status.paused")
      : s.active
        ? t("auto.status.on", { profile: nameOf(s.active.profile), program: s.because ?? "" })
        : t("auto.status.none");
  const effectNote = (r: AutoRule) => {
    const p = all.find((x) => x.id === r.profile);
    return !r.writes && p?.lighting?.kind === "effect";
  };

  return (
    <section className="card flex flex-col" data-auto-profiles>
      <div className="flex flex-col gap-3 p-5">
        <div className="flex items-start justify-between gap-4">
          <span className="flex flex-col gap-1">
            <span className="text-[14px] font-semibold">{t("auto.title")}</span>
            <span className="max-w-[80ch] text-[12px] text-ink-3">{t("auto.body")}</span>
          </span>
          <Toggle checked={s.settings.enabled} onChange={(enabled) => set({ enabled, paused: false })} label={t("auto.title")} />
        </div>
        <div className="flex items-center gap-2 text-[12px]" data-auto-status>
          <span
            className={`h-2 w-2 shrink-0 rounded-full ${s.settings.enabled && !s.settings.paused ? (s.active ? "bg-ok" : "bg-signal") : "bg-ink-3"}`}
            aria-hidden
          />
          <span className="text-ink-2">{status}</span>
        </div>
        {s.settings.enabled && !usb && <p className="text-[12px] text-ink-3">{t("games.noUsb")}</p>}
      </div>

      <div className="flex flex-col gap-3 border-t border-line px-5 py-4">
        {rules.length === 0 ? (
          <p className="text-[12px] text-ink-3">{t("auto.empty")}</p>
        ) : (
          <ul className="flex flex-col divide-y divide-line rounded-lg border border-line" data-auto-rules>
            {rules.map((r) => (
              <li key={r.path} className="flex flex-col gap-2 px-3 py-2.5">
                <div className="flex flex-wrap items-center gap-3 text-[13px]">
                  {r.path === RULE_GAMES ? <Gamepad2 size={15} className="text-signal" /> : <ProgramIcon path={r.path} fallback={AppWindow} className="text-ink-3" />}
                  <span className="min-w-0 flex-1 truncate font-medium">{r.path === RULE_GAMES ? t("auto.allGames") : r.name}</span>
                  <select
                    className="h-8 max-w-[220px] rounded-lg border border-line bg-bg-raise px-2 text-[13px] outline-none focus:border-signal"
                    value={r.profile}
                    aria-label={t("auto.profile")}
                    onChange={(e) => setRule(r.path, { profile: e.target.value })}
                  >
                    {!all.some((p) => p.id === r.profile) && (
                      <option value={r.profile} disabled>
                        {t("auto.profile.gone")}
                      </option>
                    )}
                    {all.map((p) => (
                      <option key={p.id} value={p.id}>
                        {profileName(p, t)}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="rounded-md p-1.5 text-ink-3 hover:bg-surface-2 hover:text-danger"
                    aria-label={t("auto.remove")}
                    title={t("auto.remove")}
                    onClick={() => set({ rules: rules.filter((x) => x.path !== r.path) })}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
                <label className="flex items-start gap-2 text-[12px] text-ink-2">
                  <input type="checkbox" className="mt-0.5" checked={r.writes} onChange={(e) => setRule(r.path, { writes: e.target.checked })} />
                  <span className="flex flex-col">
                    <span>{t("auto.writes")}</span>
                    <span className="text-ink-3">{t("auto.writes.body", { max: s.writesMax })}</span>
                  </span>
                </label>
                {r.path === RULE_GAMES && !games?.settings.enabled && <p className="text-[12px] text-ink-3">{t("auto.needsGames")}</p>}
                {/* Any program can be a game the lists don't know: warn on every written rule. */}
                {r.writes && (
                  <p className="flex items-start gap-1.5 text-[12px] text-signal">
                    <Info size={13} className="mt-0.5 shrink-0" />
                    {t("auto.macroWarning")}
                  </p>
                )}
                {effectNote(r) && <p className="text-[12px] text-ink-3">{t("auto.effectNote")}</p>}
              </li>
            ))}
          </ul>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <select className="h-9 min-w-[240px] rounded-lg border border-line bg-bg-raise px-3 text-[13px] outline-none focus:border-signal" value={adding} aria-label={t("auto.add")} onChange={(e) => setAdding(e.target.value)}>
            <option value="">{t("auto.add.pick")}</option>
            {choices.map((c) => (
              <option key={c.path} value={c.path}>
                {c.name}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="btn-secondary"
            disabled={!adding}
            onClick={() => {
              const c = choices.find((x) => x.path === adding);
              if (!c) return;
              set({ rules: [...rules, { path: c.path, name: c.name, profile: firstProfile, writes: false }] });
              setAdding("");
            }}
          >
            <Plus size={14} />
            {t("auto.add")}
          </button>
        </div>
        <p className="text-[12px] text-ink-3">{t("auto.add.body")}</p>
      </div>

      <div className="flex flex-col gap-2 border-t border-line px-5 py-4">
        <div className="flex flex-wrap items-center gap-3 text-[13px]" data-auto-back>
          <label className="flex flex-wrap items-center gap-3">
            <span className="font-medium">{t("auto.back")}</span>
            <select
              className="h-8 max-w-[260px] rounded-lg border border-line bg-bg-raise px-2 text-[13px] outline-none focus:border-signal"
              value={s.settings.back}
              onChange={(e) => {
                const back = e.target.value as AutoBack;
                set(back === "profile" && !s.settings.base ? { back, base: firstProfile } : { back });
              }}
            >
              <option value="previous">{t("auto.back.previous")}</option>
              <option value="profile">{t("auto.back.profile")}</option>
              <option value="stay">{t("auto.back.stay")}</option>
            </select>
          </label>
          {s.settings.back === "profile" && (
            <select
              className="h-8 max-w-[240px] rounded-lg border border-line bg-bg-raise px-2 text-[13px] outline-none focus:border-signal"
              value={s.settings.base ?? ""}
              aria-label={t("auto.back.pick")}
              onChange={(e) => set({ base: e.target.value })}
            >
              {!all.some((p) => p.id === s.settings.base) && (
                <option value={s.settings.base ?? ""} disabled>
                  {t("auto.profile.gone")}
                </option>
              )}
              {all.map((p) => (
                <option key={p.id} value={p.id}>
                  {profileName(p, t)}
                </option>
              ))}
            </select>
          )}
        </div>
        <p className="text-[12px] text-ink-3">{t(`auto.back.body.${s.settings.back}`)}</p>
        <p className="text-[12px] text-ink-3" data-auto-writes>
          {t("auto.count", { n: s.writesToday, max: s.writesMax })}
        </p>
        {s.lastError && <p className="text-[12px] text-danger">{s.lastError === "daily limit reached" ? t("auto.limit") : t("auto.error")}</p>}
      </div>
    </section>
  );
}
