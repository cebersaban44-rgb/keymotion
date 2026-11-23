// Tools › AI agents (design A+E, notes §12.4): the agent access switch and
// its connect steps (1 Things to connect › 2 Done, Cancel on the first),
// the coding apps and how they stand, how the keyboard tells you (the
// five ready-made signals), what else agents may do, the extra signals
// A–H, whether agents may change these settings, the session numbers and
// the recent actions with Undo.

import {
  ArrowRight,
  Bot,
  Check,
  ChevronDown,
  CircleAlert,
  Gamepad2,
  Layers,
  LightbulbOff,
  Monitor,
  Settings2,
  Timer,
  Undo2,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { AgentEffect } from "../bindings/AgentEffect";
import type { AgentLook } from "../bindings/AgentLook";
import type { AgentLogEntry } from "../bindings/AgentLogEntry";
import type { AgentSessionInfo } from "../bindings/AgentSessionInfo";
import type { AgentSettings } from "../bindings/AgentSettings";
import type { AgentSignalLooks } from "../bindings/AgentSignalLooks";
import { ColorPicker, InfoBar, Row, Rows, Toggle } from "../components/controls";
import type { AgentApp } from "../bindings/AgentApp";
import type { AgentAppPlan } from "../bindings/AgentAppPlan";
import type { AgentAppStatus } from "../bindings/AgentAppStatus";
import type { AgentConnectOutcome } from "../bindings/AgentConnectOutcome";
import type { AgentConnectPlan } from "../bindings/AgentConnectPlan";
import type { AgentFileChange } from "../bindings/AgentFileChange";
import {
  agentConnect,
  agentConnectPlan,
  agentDisconnect,
  agentDisconnectPlan,
  clearAgentLights,
  followAgents,
  tryAgentLook,
  undoAgentChange,
  useAgents,
} from "../lib/agents";
import { friendlyError } from "../lib/errors";
import { clockTime, intlTag, useT, type MessageKey, type Translate } from "../lib/i18n";
import { colorNameKey } from "../lib/keyColor";
import { useApp } from "../lib/store";

type Rgb = [number, number, number];
type SignalId = keyof AgentSignalLooks;

const SIGNALS: SignalId[] = ["done", "attention", "error", "busy", "info"];
const EFFECTS: AgentEffect[] = ["solid", "pulse", "blink", "wave", "flash", "waveThenSolid", "flashThenPulse", "blinkTwiceThenSolid", "off"];
/** Durations offered, in seconds (the one set now is added when it is not here). */
const DURATIONS = [10, 30, 60, 120, 300, 600, 1800, 3600, 7200];

const hex = (c: Rgb) => `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
const rgb = (h: string): Rgb => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;

/** "2 minutes", "2 dakika", "2 минуты", "2分钟", in the interface language. */
function useSpan() {
  const locale = useApp((s) => s.locale);
  return (seconds: number) => {
    const [n, unit] = seconds % 3600 === 0 ? [seconds / 3600, "hour"] : seconds % 60 === 0 ? [seconds / 60, "minute"] : [seconds, "second"];
    return new Intl.NumberFormat(intlTag(locale), { style: "unit", unit, unitDisplay: "long" }).format(n);
  };
}

/** "claude-code:3f9a" → "Claude Code". */
function sourceName(source: string, t: Translate) {
  const base = source.split(":")[0] ?? source;
  if (base === "claude-code") return "Claude Code";
  if (base === "codex") return "Codex";
  if (base === "cli") return t("ag.src.cli");
  return base;
}

export function Agents() {
  const t = useT();
  const state = useAgents((s) => s.state);
  const connections = useAgents((s) => s.connections);
  const configure = useAgents((s) => s.configure);
  const [flow, setFlow] = useState<Flow | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    useAgents.getState().load().catch(console.error);
    useAgents.getState().loadConnections().catch(console.error);
    return followAgents();
  }, []);

  if (!state) return null;
  const s = state.settings;
  const set = (patch: Partial<AgentSettings>) => configure(patch).catch((e) => setError(friendlyError(e, t)));

  /** Turning access on shows what will be connected first; off is at once. */
  const toggleAccess = async (on: boolean) => {
    setError(null);
    if (!on) {
      setFlow(null);
      return set({ enabled: false });
    }
    try {
      const plan = await agentConnectPlan();
      if (plan.empty) {
        const outcome = await agentConnect(plan.token);
        finishConnect(outcome, undefined);
      } else setFlow({ kind: "connect", step: 1, plan, changed: false });
    } catch (e) {
      setError(friendlyError(e, t));
    }
  };
  const finishConnect = (outcome: AgentConnectOutcome, apps: AgentApp[] | undefined) => {
    if (!outcome.applied && outcome.plan) {
      setFlow({ kind: "connect", step: 1, plan: outcome.plan, apps, changed: true });
      return;
    }
    useAgents.getState().set(outcome.state);
    useAgents.getState().setConnections(outcome.connections);
    setFlow({ kind: "connect", step: 2, outcome, apps });
  };
  const connectOne = async (app: AgentApp) => {
    setError(null);
    try {
      setFlow({ kind: "connect", step: 1, plan: await agentConnectPlan([app]), apps: [app], changed: false });
    } catch (e) {
      setError(friendlyError(e, t));
    }
  };
  const disconnectOne = async (app: AgentApp) => {
    setError(null);
    try {
      setFlow({ kind: "disconnect", step: 1, app, plan: await agentDisconnectPlan(app) });
    } catch (e) {
      setError(friendlyError(e, t));
    }
  };

  const lightsNote: MessageKey | null =
    !s.enabled || state.lights === "shown"
      ? null
      : (`ag.lights.${state.lights}` as MessageKey);

  return (
    <div className="flex max-w-[1000px] flex-col gap-6" data-agents>
      <header>
        <h1 className="page-title">{t("nav.agents")}</h1>
        <p className="page-desc">{t("ag.desc")}</p>
      </header>

      <div className="flex flex-col gap-2">
        <div className="rows">
          <Row icon={Bot} title={t("ag.access")} desc={t("ag.access.desc")} data="access">
            <Toggle state checked={s.enabled} onChange={(on) => void toggleAccess(on)} label={t("ag.access")} />
          </Row>
        </div>
        {flow && (
          <ConnectFlow
            flow={flow}
            onCancel={() => setFlow(null)}
            onDone={() => setFlow(null)}
            onApprove={async () => {
              try {
                if (flow.kind === "connect" && flow.step === 1) finishConnect(await agentConnect(flow.plan.token, flow.apps), flow.apps);
                else if (flow.kind === "disconnect" && flow.step === 1) {
                  const outcome = await agentDisconnect(flow.app);
                  useAgents.getState().setConnections(outcome.connections);
                  setFlow({ kind: "disconnect", step: 2, app: flow.app, plan: outcome.plan });
                }
              } catch (e) {
                setError(friendlyError(e, t));
              }
            }}
          />
        )}
        {error && <InfoBar tone="error" title={error} live />}
        {state.server.error && <InfoBar tone="error" title={t("ag.server.error", { message: state.server.error })} />}
        {lightsNote && <InfoBar tone="warn" title={t(lightsNote)} data="lights" />}
      </div>

      {connections && (
        <Rows title={t("ag.apps")} data="apps">
          {connections.apps.map((a) => (
            <AppRow key={a.app} app={a} access={s.enabled} busy={!!flow} onConnect={() => void connectOne(a.app)} onDisconnect={() => void disconnectOne(a.app)} />
          ))}
        </Rows>
      )}

      <Rows title={t("ag.signals")} data="signals">
        {SIGNALS.map((id) => (
          <SignalRow key={id} id={id} look={s.signals[id]} onChange={(look) => set({ signals: { ...s.signals, [id]: look } })} />
        ))}
      </Rows>

      <Rows title={t("ag.more")} data="more">
        <PermissionRow icon={Monitor} title={t("ag.screen")} desc={t("ag.screen.desc", { count: s.screenPerHour })} on={s.screen} set={(screen) => set({ screen })} />
        <PermissionRow icon={Timer} title={t("ag.focus")} desc={t("ag.focus.desc")} on={s.focus} set={(focus) => set({ focus })} />
        <PermissionRow icon={Layers} title={t("ag.profiles")} desc={t("ag.profiles.desc")} on={s.profiles} set={(profiles) => set({ profiles })} />
        <PermissionRow icon={Gamepad2} title={t("ag.hideInGame")} desc={t("ag.hideInGame.desc")} on={s.hideInGame} set={(hideInGame) => set({ hideInGame })} />
      </Rows>

      <Extras settings={s} set={set} />

      <div className="rows">
        <Row icon={Settings2} title={t("ag.mayEdit")} desc={t("ag.mayEdit.desc")} data="may-edit">
          <Toggle state checked={s.agentsMayEdit} onChange={(agentsMayEdit) => set({ agentsMayEdit })} label={t("ag.mayEdit")} />
        </Row>
      </div>

      <Rows title={t("ag.sessions")} data="sessions">
        {state.sessions.length === 0 ? (
          <Row title={<span className="text-ink-2">{t("ag.sessions.none")}</span>} />
        ) : (
          state.sessions.map((x) => (
            <Row
              key={x.source}
              lead={<NumberTile n={x.number} />}
              title={x.label ?? t("ag.sessions.unnamed")}
              desc={[sourceName(x.source, t), x.number == null ? t("ag.sessions.noNumber") : null].filter(Boolean).join(" · ")}
            >
              {x.waiting && (
                <span className="flex items-center gap-2 text-[13px]">
                  <span className="h-2 w-2 rounded-full bg-warn" aria-hidden />
                  {t("ag.sessions.waiting")}
                </span>
              )}
            </Row>
          ))
        )}
      </Rows>

      <Rows
        title={t("ag.log")}
        aside={
          <button
            type="button"
            className="btn-secondary"
            onClick={() => clearAgentLights().then(useAgents.getState().set, (e) => setError(friendlyError(e, t)))}
            data-agents-clear
          >
            <LightbulbOff size={15} strokeWidth={1.6} aria-hidden />
            {t("ag.clearAll")}
          </button>
        }
        data="log"
      >
        {state.log.length === 0 ? (
          <Row title={<span className="text-ink-2">{t("ag.log.none")}</span>} />
        ) : (
          state.log.slice(0, 20).map((e) => <LogRow key={e.seq} entry={e} settings={s} sessions={state.sessions} onError={(m) => setError(m)} />)
        )}
      </Rows>
    </div>
  );
}

// -------------------------------------------------------- connect steps

type Flow =
  | { kind: "connect"; step: 1; plan: AgentConnectPlan; apps?: AgentApp[]; changed: boolean }
  | { kind: "connect"; step: 2; outcome: AgentConnectOutcome; apps?: AgentApp[] }
  | { kind: "disconnect"; step: 1 | 2; app: AgentApp; plan: AgentAppPlan };

/** The steps over the page's rows: what will change, then what did. */
function ConnectFlow({ flow, onCancel, onApprove, onDone }: { flow: Flow; onCancel: () => void; onApprove: () => Promise<void>; onDone: () => void }) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [details, setDetails] = useState(false);
  const one = flow.kind === "connect" && flow.apps?.length === 1;
  const appName = flow.kind === "disconnect" ? flow.plan.name : flow.kind === "connect" && flow.step === 1 && one ? (flow.plan.apps[0]?.name ?? "") : "";
  const title =
    flow.kind === "disconnect"
      ? t("ag.disconnect.title", { name: appName })
      : one && flow.step === 1
        ? t("ag.flow.connectTitle", { name: appName })
        : t("ag.flow.title");
  const approve = async () => {
    setBusy(true);
    await onApprove();
    setBusy(false);
  };

  let body: ReactNode;
  if (flow.kind === "connect" && flow.step === 1) {
    const plan = flow.plan;
    body = (
      <>
        {flow.changed && <InfoBar tone="warn" title={t("ag.flow.changed")} />}
        <p className="text-ink-2">{t("ag.flow.body")}</p>
        {plan.empty && <p>{t("ag.flow.nothing")}</p>}
        {plan.apps.map((a) => (
          <PlanApp key={a.app} plan={a} details={details} />
        ))}
        {plan.notFound.map((app) => (
          <p key={app} className="flex items-center gap-2 text-ink-2">
            <span className="h-2 w-2 rounded-full shadow-[inset_0_0_0_1.5px_var(--ink-3)]" aria-hidden />
            {t("ag.line.notFound", { name: app === "claudeCode" ? "Claude Code" : "Codex" })}
          </p>
        ))}
        {plan.path && !plan.path.onPath && (
          <div className="flex flex-col">
            <span className="font-semibold">keymotion-cli</span>
            <ul className="ml-4 list-disc text-ink-2">
              <li>{t("ag.line.path")}</li>
            </ul>
            {plan.path.error && <p className="ml-4 text-danger">{t("ag.done.pathError", { message: plan.path.error })}</p>}
            {details && (
              <div className="mt-1 ml-4 text-[12px] text-ink-2 select-text">
                <Detail label={t("ag.details.path")} value={plan.path.dir} />
              </div>
            )}
          </div>
        )}
      </>
    );
  } else if (flow.kind === "connect") {
    const o = flow.outcome;
    body = (
      <ul className="flex flex-col gap-2">
        <li className="flex items-start gap-2 font-semibold">
          <Check size={16} className="mt-0.5 text-ok" aria-hidden />
          {t("ag.flow.done")}
        </li>
        {o.connected.map((app) => {
          const name = o.connections.apps.find((a) => a.app === app)?.name ?? app;
          const waits = o.connections.apps.find((a) => a.app === app)?.state === "needsApproval";
          return (
            <li key={app} className="flex items-start gap-2">
              <Check size={16} className="mt-0.5 text-ok" aria-hidden />
              {t(waits ? "ag.done.needsApproval" : "ag.done.connected", { name })}
            </li>
          );
        })}
        {o.failed.map((f) => (
          <li key={f.app} className="flex items-start gap-2 text-danger">
            <CircleAlert size={16} className="mt-0.5 shrink-0" aria-hidden />
            <span>
              {t("ag.done.failed", { name: f.name })} {f.problem && problemText(f, t)}
            </span>
          </li>
        ))}
        {o.pathAdded && (
          <li className="flex items-start gap-2">
            <Check size={16} className="mt-0.5 text-ok" aria-hidden />
            {t("ag.done.path")}
          </li>
        )}
        {o.saveError && (
          <li className="flex items-start gap-2 text-danger">
            <CircleAlert size={16} className="mt-0.5 shrink-0" aria-hidden />
            {t("ag.done.saveError", { message: o.saveError })}
          </li>
        )}
        {o.pathError && (
          <li className="flex items-start gap-2 text-danger">
            <CircleAlert size={16} className="mt-0.5 shrink-0" aria-hidden />
            {t("ag.done.pathError", { message: o.pathError })}
          </li>
        )}
      </ul>
    );
  } else if (flow.step === 1) {
    body = (
      <>
        <p className="text-ink-2">{t("ag.disconnect.body")}</p>
        <PlanApp plan={flow.plan} details={details} />
      </>
    );
  } else {
    body = flow.plan.problem ? (
      <p className="flex items-start gap-2 text-danger">
        <CircleAlert size={16} className="mt-0.5 shrink-0" aria-hidden />
        {problemText(flow.plan, t)}
      </p>
    ) : (
      <p className="flex items-start gap-2">
        <Check size={16} className="mt-0.5 text-ok" aria-hidden />
        {t("ag.disconnect.done", { name: flow.plan.name })}
      </p>
    );
  }

  const step = flow.step;
  return (
    <section className="dlg !shadow-none" aria-label={title} data-agents-flow={`${flow.kind}-${step}`}>
      <div className="flex flex-col gap-3.5 p-5 text-[14px]">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <h2 className="subtitle">{title}</h2>
          <ol className="flex items-center gap-2 text-[13px]" aria-label={title}>
            {([flow.kind === "disconnect" ? "ag.flow.step1.remove" : "ag.flow.step1", "ag.flow.step2"] as const).map((key, i) => (
              <li key={key} className="flex items-center gap-2" aria-current={step === i + 1 ? "step" : undefined}>
                {i > 0 && <span className="text-ink-3">›</span>}
                <span
                  className={`grid h-5 w-5 place-items-center rounded-full text-[11px] font-semibold ${
                    step === i + 1 ? "bg-signal text-signal-ink" : step > i + 1 ? "bg-ok text-white" : "shadow-[inset_0_0_0_1px_var(--ctl-strong)] text-ink-2"
                  }`}
                >
                  {step > i + 1 ? <Check size={12} strokeWidth={3} aria-hidden /> : i + 1}
                </span>
                <span className={step === i + 1 ? "font-semibold" : "text-ink-2"}>{t(key)}</span>
              </li>
            ))}
          </ol>
        </div>
        {body}
      </div>
      <div className="dlg-foot !justify-start !py-4">
        {step === 1 ? (
          <>
            <button type="button" className="btn-primary" disabled={busy} onClick={() => void approve()} data-agents-approve>
              {flow.kind === "disconnect"
                ? t("ag.disconnect.approve")
                : one
                  ? t("ag.flow.approveConnect")
                  : t("ag.flow.approve")}
            </button>
            <button type="button" className="btn-secondary" disabled={busy} onClick={onCancel}>
              {t("unsaved.cancel")}
            </button>
            <button type="button" className="btn-subtle ml-auto !min-w-0" aria-expanded={details} onClick={() => setDetails((d) => !d)}>
              {t("ag.flow.details")}
              <ChevronDown size={14} className={`transition-transform ${details ? "rotate-180" : ""}`} aria-hidden />
            </button>
          </>
        ) : (
          <button type="button" className="btn-primary" onClick={onDone} data-agents-done>
            {t("ag.flow.ok")}
          </button>
        )}
      </div>
    </section>
  );
}

function problemText(plan: AgentAppPlan, t: Translate): string {
  const p = plan.problem;
  if (!p) return "";
  if (p.kind === "unsupported") return t("ag.problem.unsupported", { name: plan.name, min: p.message.match(/\d+\.\d+\.\d+/)?.[0] ?? "" });
  if (p.kind === "invalid") return t("ag.problem.invalid");
  if (p.kind === "partial") return t("ag.problem.partial");
  return t("ag.problem.other", { message: p.message });
}

/** What a file Keymotion adds, changes or takes away does, in plain words
 *  (its name and the hooks' count are in Details). */
function fileLine(c: AgentFileChange, t: Translate): string | null {
  const remove = c.action === "remove" || /^remove/i.test(c.summary);
  if (c.kind === "hooks") return t(remove ? "ag.line.removeHooks" : "ag.line.addHooks");
  if (c.kind === "skill") return t(remove ? "ag.line.removeSkill" : "ag.line.addSkill");
  if (c.kind === "rule") return t(remove ? "ag.line.removeRule" : "ag.line.addRule");
  return null;
}

/** A file under Details: what it is and where. */
function fileDetail(c: AgentFileChange, t: Translate): string {
  if (c.kind === "hooks") return t("ag.details.hooks", { count: c.hooks ?? 0 });
  if (c.kind === "skill") return t("ag.details.skill");
  if (c.kind === "rule") return t("ag.details.rule");
  return t("ag.details.note");
}

/** A label in the interface font, then a path or command line in the code font. */
function Detail({ label, value }: { label: string; value: string }) {
  const locale = useApp((s) => s.locale);
  return (
    <span className="block">
      {label}
      {locale === "zh-CN" ? "" : " "}
      <code className="mono break-all">{value}</code>
    </span>
  );
}

function PlanApp({ plan, details }: { plan: AgentAppPlan; details: boolean }) {
  const t = useT();
  const lines = plan.changes.map((c) => fileLine(c, t)).filter((l): l is string => !!l);
  return (
    <div className="flex flex-col" data-plan-app={plan.app}>
      <span className="font-semibold">{plan.name}</span>
      {plan.problem ? (
        <p className="ml-4 text-danger">{problemText(plan, t)}</p>
      ) : (
        <ul className="ml-4 list-disc text-ink-2">
          {lines.map((l) => (
            <li key={l}>{l}</li>
          ))}
          {plan.needsApproval && <li>{t("ag.line.codexTrust")}</li>}
        </ul>
      )}
      {details && (
        <div className="mt-1.5 ml-4 flex flex-col gap-1 text-[12px] text-ink-2 select-text">
          {plan.changes.map((c) => (
            <span key={c.path} className="flex flex-col">
              <Detail label={fileDetail(c, t)} value={c.path} />
              {c.backup && <Detail label={t("ag.details.backup")} value={c.backup} />}
            </span>
          ))}
          {plan.commandLine && <Detail label={t("ag.details.runs")} value={plan.commandLine} />}
          {plan.warnings.map((w) => (
            <span key={w} lang="en">
              {w}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

// -------------------------------------------------------------- the apps

const STATE_DOT: Record<string, string> = {
  connected: "bg-ok",
  needsApproval: "bg-warn",
  outdated: "bg-warn",
  unreadable: "bg-danger",
};

function AppRow({
  app,
  access,
  busy,
  onConnect,
  onDisconnect,
}: {
  app: AgentAppStatus;
  access: boolean;
  busy: boolean;
  onConnect: () => void;
  onDisconnect: () => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const label = app.tooOld ? t("ag.state.tooOld") : t(`ag.state.${app.state}` as MessageKey);
  const linked = app.state === "connected" || app.state === "needsApproval" || app.state === "outdated";
  return (
    <Expander
      open={open}
      onToggle={() => setOpen((o) => !o)}
      label={app.name}
      head={
        <>
          <span className="row-icon">
            <span className="grid h-8 w-8 place-items-center rounded-md border border-line bg-[var(--subtle-sel)] text-[13px] font-semibold">
              {app.app === "claudeCode" ? "CC" : "Cx"}
            </span>
          </span>
          <span className="row-text">
            <span className="row-title">{app.name}</span>
            <span className="row-desc">{t(app.app === "claudeCode" ? "ag.app.claudeCode.desc" : "ag.app.codex.desc")}</span>
          </span>
          <span className="row-end">
            <span className="flex items-center gap-2 text-[13px]" data-app-state={app.state}>
              <span className={`h-2 w-2 rounded-full ${STATE_DOT[app.state] ?? "shadow-[inset_0_0_0_1.5px_var(--ink-3)]"}`} aria-hidden />
              {label}
            </span>
            {access && app.found && !app.tooOld && (app.state === "notConnected" || app.state === "outdated") && (
              <button type="button" className="btn-secondary" disabled={busy} onClick={onConnect}>
                {t(app.state === "outdated" ? "ag.reconnect" : "ag.connect")}
              </button>
            )}
          </span>
        </>
      }
    >
      <div className="flex flex-col gap-1 text-[12px] text-ink-2 select-text">
        {app.version && <span>{t("ag.details.version", { version: app.version })}</span>}
        <Detail label={t("ag.details.folder")} value={app.home} />
        {app.found && <code className="mono break-all">{app.hooksFile}</code>}
        {app.found && <code className="mono break-all">{app.skillFile}</code>}
        {app.runs && <Detail label={t("ag.details.runs")} value={app.runs} />}
        {app.error && <span className="text-danger">{app.error}</span>}
      </div>
      {linked && (
        <div className="mt-3">
          <button type="button" className="btn-secondary is-danger" disabled={busy} onClick={onDisconnect}>
            {t("ag.disconnect")}
          </button>
        </div>
      )}
    </Expander>
  );
}

/** A row that opens its settings under it (Windows' expander). */
function Expander({
  open,
  onToggle,
  label,
  head,
  children,
  data,
}: {
  open: boolean;
  onToggle: () => void;
  label: string;
  head: ReactNode;
  children: ReactNode;
  data?: string;
}) {
  const t = useT();
  return (
    <div className="flex flex-col" data-expander={data}>
      <div className={`row ${open ? "rounded-b-none" : ""}`}>
        {head}
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={open}
          aria-label={t("ag.x.settings", { name: label })}
          className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-ink-2 hover:bg-[var(--subtle-hover)]"
        >
          <ChevronDown size={16} className={`transition-transform ${open ? "rotate-180" : ""}`} aria-hidden />
        </button>
      </div>
      {open && (
        <div className="rounded-b-[4px] border border-t-0 border-line bg-surface px-[18px] py-4 pl-[54px]" data-expander-body>
          {children}
        </div>
      )}
    </div>
  );
}

// --------------------------------------------------------------- signals

/** Effect and duration lists for a look, and "Try on the keyboard". */
function LookControls({ look, onChange, until, tryable = true }: { look: AgentLook; onChange: (l: AgentLook) => void; until?: boolean; tryable?: boolean }) {
  const t = useT();
  const span = useSpan();
  const off = look.effect === "off";
  const options = [...new Set([...DURATIONS, look.durationS])].sort((a, b) => a - b);
  const [tried, setTried] = useState(false);
  return (
    <>
      <select
        value={look.effect}
        aria-label={t("ag.effect")}
        onChange={(e) => onChange({ ...look, effect: e.target.value as AgentEffect })}
        className="h-8 w-[230px] max-w-full rounded-md border border-line bg-bg-raise px-2.5 text-[13px]"
      >
        {EFFECTS.map((fx) => (
          <option key={fx} value={fx}>
            {t(`ag.fx.${fx}` as MessageKey)}
          </option>
        ))}
      </select>
      <select
        value={until && look.untilAnswered ? "until" : String(look.durationS)}
        disabled={off}
        aria-label={t("ag.duration")}
        onChange={(e) =>
          onChange(
            e.target.value === "until"
              ? { ...look, untilAnswered: true, durationS: 1800 }
              : { ...look, untilAnswered: false, durationS: Number(e.target.value) },
          )
        }
        className="h-8 w-[160px] max-w-full rounded-md border border-line bg-bg-raise px-2.5 text-[13px] disabled:opacity-45"
      >
        {off && <option value={String(look.durationS)}>—</option>}
        {until && <option value="until">{t("ag.dur.untilAnswered")}</option>}
        {options.map((d) => (
          <option key={d} value={String(d)}>
            {span(d)}
          </option>
        ))}
      </select>
      {tryable && (
        <button
          type="button"
          className="btn-secondary"
          disabled={off}
          onClick={() => {
            setTried(true);
            tryAgentLook(look).then(useAgents.getState().set, console.error);
            window.setTimeout(() => setTried(false), 1500);
          }}
        >
          {tried && <Check size={14} aria-hidden />}
          {t("ag.try")}
        </button>
      )}
    </>
  );
}

/** A colour dot that opens the app's colour grid (as on the Lighting page)
 *  in a flyout under it. */
function ColorDot({ color, onChange, label, hollow }: { color: Rgb; onChange: (c: Rgb) => void; label: string; hollow?: boolean }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        box.current?.querySelector<HTMLButtonElement>("button")?.focus();
      }
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", key);
    window.requestAnimationFrame(() => box.current?.querySelector<HTMLElement>('[role="radio"][aria-checked="true"], [role="radio"]')?.focus());
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", key);
    };
  }, [open]);
  return (
    <span ref={box} className="relative block">
      <button
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={label}
        title={label}
        onClick={() => setOpen((o) => !o)}
        className="block h-[14px] w-[14px] rounded-full transition-transform hover:scale-125"
        style={
          hollow
            ? { boxShadow: "inset 0 0 0 1.5px var(--ink-3), 0 0 0 3px var(--subtle-sel)" }
            : { background: hex(color), boxShadow: "0 0 0 3px var(--subtle-sel)" }
        }
        data-color-dot
      />
      {open && (
        <div role="dialog" aria-label={label} className="flyout absolute top-6 -left-2 z-30 w-[292px] p-3.5" data-color-flyout>
          <ColorPicker label={label} value={hex(color)} onChange={(h) => onChange(rgb(h))} />
        </div>
      )}
    </span>
  );
}

function SignalRow({ id, look, onChange }: { id: SignalId; look: AgentLook; onChange: (l: AgentLook) => void }) {
  const t = useT();
  const name = t(`ag.sig.${id}` as MessageKey);
  const off = look.effect === "off";
  const colorKey = colorNameKey(look.color);
  const head = off ? t("ag.fx.off") : colorKey ? t(colorKey) : hex(look.color);
  return (
    <Row
      className="flex-wrap"
      lead={<ColorDot color={look.color} hollow={off} onChange={(color) => onChange({ ...look, color })} label={t("ag.color.of", { name })} />}
      title={name}
      desc={`${head} · ${t(`ag.sig.${id}.desc` as MessageKey)}`}
      data={`signal-${id}`}
    >
      <LookControls look={look} onChange={onChange} until={id === "attention"} />
    </Row>
  );
}

function PermissionRow({ icon, title, desc, on, set }: { icon: LucideIcon; title: string; desc: string; on: boolean; set: (v: boolean) => void }) {
  return (
    <Row icon={icon} title={title} desc={desc}>
      <Toggle state checked={on} onChange={set} label={title} />
    </Row>
  );
}

// ------------------------------------------------------------ A–H extras

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 text-[14px]">
      <span>{label}</span>
      {children}
    </label>
  );
}

function MinutesSelect({ value, minutes, onChange, label }: { value: number; minutes: number[]; onChange: (s: number) => void; label: string }) {
  const span = useSpan();
  const options = [...new Set([...minutes.map((m) => m * 60), value])].sort((a, b) => a - b);
  return (
    <select
      value={String(value)}
      aria-label={label}
      onChange={(e) => onChange(Number(e.target.value))}
      className="h-8 w-[160px] rounded-md border border-line bg-bg-raise px-2.5 text-[13px]"
    >
      {options.map((s) => (
        <option key={s} value={String(s)}>
          {span(s)}
        </option>
      ))}
    </select>
  );
}

const pad = (n: number) => String(n).padStart(2, "0");
const toTime = (min: number) => `${pad(Math.floor(min / 60))}:${pad(min % 60)}`;
const fromTime = (v: string) => {
  const [h, m] = v.split(":").map(Number);
  return (h ?? 0) * 60 + (m ?? 0);
};

function Extras({ settings: s, set }: { settings: AgentSettings; set: (p: Partial<AgentSettings>) => void }) {
  const t = useT();
  const [open, setOpen] = useState<string | null>(null);
  const toggle = (id: string) => () => setOpen((o) => (o === id ? null : id));
  const extra = (id: string, title: MessageKey, on: boolean | null, setOn: ((v: boolean) => void) | null, body: ReactNode) => (
    <Expander
      key={id}
      open={open === id}
      onToggle={toggle(id)}
      label={t(title)}
      data={`extra-${id}`}
      head={
        <>
          <span className="row-icon text-[12px] font-semibold text-ink-2" aria-hidden>
            {id}
          </span>
          <span className="row-text">
            <span className="row-title">{t(title)}</span>
            <span className="row-desc">{t(`${title}.desc` as MessageKey)}</span>
          </span>
          {on !== null && setOn && (
            <span className="row-end">
              <Toggle state checked={on} onChange={setOn} label={t(title)} />
            </span>
          )}
        </>
      }
    >
      <div className="flex max-w-[640px] flex-col gap-3">{body}</div>
    </Expander>
  );
  const color = (value: Rgb, onChange: (c: Rgb) => void) => (
    <div className="flex flex-col gap-2 text-[14px]">
      <span>{t("ag.x.color")}</span>
      <ColorPicker inline label={t("ag.x.color")} value={hex(value)} onChange={(h) => onChange(rgb(h))} />
    </div>
  );
  return (
    <Rows title={t("ag.extra")} data="extras">
      {extra(
        "A",
        "ag.x.usage",
        s.usageLimit.on,
        (on) => set({ usageLimit: { ...s.usageLimit, on } }),
        <>
          {color(s.usageLimit.look.color, (c) => set({ usageLimit: { ...s.usageLimit, look: { ...s.usageLimit.look, color: c } } }))}
          <div className="flex flex-wrap items-center gap-2">
            <LookControls look={s.usageLimit.look} onChange={(look) => set({ usageLimit: { ...s.usageLimit, look } })} />
          </div>
        </>,
      )}
      {extra("B", "ag.x.sessions", s.sessions.on, (on) => set({ sessions: { ...s.sessions, on } }), color(s.sessions.color, (c) => set({ sessions: { ...s.sessions, color: c } })))}
      {extra(
        "C",
        "ag.x.away",
        s.away.on,
        (on) => set({ away: { ...s.away, on } }),
        <Field label={t("ag.x.away.after")}>
          <MinutesSelect label={t("ag.x.away.after")} value={s.away.afterS} minutes={[1, 2, 5, 10, 15, 30, 60]} onChange={(afterS) => set({ away: { ...s.away, afterS } })} />
        </Field>,
      )}
      {extra(
        "D",
        "ag.x.unanswered",
        s.unanswered.on,
        (on) => set({ unanswered: { ...s.unanswered, on } }),
        <>
          <Field label={t("ag.x.unanswered.every")}>
            <MinutesSelect label={t("ag.x.unanswered.every")} value={s.unanswered.everyS} minutes={[1, 2, 5, 10, 15, 30]} onChange={(everyS) => set({ unanswered: { ...s.unanswered, everyS } })} />
          </Field>
          <Field label={t("ag.x.unanswered.upTo")}>
            <MinutesSelect label={t("ag.x.unanswered.upTo")} value={s.unanswered.upToS} minutes={[5, 10, 15, 30, 60, 120]} onChange={(upToS) => set({ unanswered: { ...s.unanswered, upToS } })} />
          </Field>
        </>,
      )}
      {extra(
        "E",
        "ag.x.progress",
        null,
        null,
        <>
          <Field label={t("ag.x.progress.keys")}>
            <select
              value={s.progress.keys}
              aria-label={t("ag.x.progress.keys")}
              onChange={(e) => set({ progress: { ...s.progress, keys: e.target.value as AgentSettings["progress"]["keys"] } })}
              className="h-8 w-[160px] rounded-md border border-line bg-bg-raise px-2.5 text-[13px]"
            >
              <option value="fRow">{t("ag.x.progress.fRow")}</option>
              <option value="numbers">{t("ag.x.progress.numbers")}</option>
            </select>
          </Field>
          {color(s.progress.color, (c) => set({ progress: { ...s.progress, color: c } }))}
        </>,
      )}
      {extra(
        "F",
        "ag.x.longTask",
        s.longTask.on,
        (on) => set({ longTask: { ...s.longTask, on } }),
        <>
          <Field label={t("ag.x.longTask.after")}>
            <MinutesSelect label={t("ag.x.longTask.after")} value={s.longTask.afterS} minutes={[1, 5, 10, 15, 30, 60, 120, 240]} onChange={(afterS) => set({ longTask: { ...s.longTask, afterS } })} />
          </Field>
          <Field label={t("ag.x.longTask.show")}>
            <MinutesSelect label={t("ag.x.longTask.show")} value={s.longTask.showS} minutes={[1, 2, 5, 10, 30]} onChange={(showS) => set({ longTask: { ...s.longTask, showS } })} />
          </Field>
        </>,
      )}
      {extra(
        "G",
        "ag.x.night",
        s.night.on,
        (on) => set({ night: { ...s.night, on } }),
        <>
          <Field label={t("ag.x.night.from")}>
            <input
              type="time"
              value={toTime(s.night.fromMin)}
              onChange={(e) => e.target.value && set({ night: { ...s.night, fromMin: fromTime(e.target.value) } })}
              className="h-8 w-[160px] rounded-md border border-line bg-bg-raise px-2.5 text-[13px]"
            />
          </Field>
          <Field label={t("ag.x.night.to")}>
            <input
              type="time"
              value={toTime(s.night.toMin)}
              onChange={(e) => e.target.value && set({ night: { ...s.night, toMin: fromTime(e.target.value) } })}
              className="h-8 w-[160px] rounded-md border border-line bg-bg-raise px-2.5 text-[13px]"
            />
          </Field>
          <Field label={t("ag.x.night.mode")}>
            <select
              value={s.night.mode}
              aria-label={t("ag.x.night.mode")}
              onChange={(e) => set({ night: { ...s.night, mode: e.target.value as AgentSettings["night"]["mode"] } })}
              className="h-8 w-[160px] rounded-md border border-line bg-bg-raise px-2.5 text-[13px]"
            >
              <option value="dim">{t("ag.x.night.dim")}</option>
              <option value="off">{t("ag.x.night.off")}</option>
            </select>
          </Field>
        </>,
      )}
      {extra("H", "ag.x.helper", s.helper.on, (on) => set({ helper: { ...s.helper, on } }), color(s.helper.color, (c) => set({ helper: { ...s.helper, color: c } })))}
    </Rows>
  );
}

// ------------------------------------------------- sessions and the log

function NumberTile({ n }: { n: number | null }) {
  return (
    <span className="grid h-8 w-8 place-items-center rounded-md border border-line bg-[var(--subtle-sel)] text-[14px] font-semibold tabular-nums">
      {n ?? "–"}
    </span>
  );
}

/** The light or event an entry was about: the id's last part
 *  (`claude-code:3f9a:done` → `done`). */
const EVENT_OF = (id: string | null) => id?.split(":").pop() ?? null;

const EVENT_NAME: Record<string, MessageKey> = {
  done: "ag.sig.done",
  attention: "ag.sig.attention",
  error: "ag.sig.error",
  busy: "ag.sig.busy",
  info: "ag.sig.info",
  usage_limit: "ag.x.usage",
  helper: "ag.x.helper",
  answered: "ag.ev.answered",
  session_start: "ag.ev.sessionStart",
  session_end: "ag.ev.sessionEnd",
  interrupted: "ag.ev.interrupted",
};

const COMMAND_NAME: Record<string, MessageKey> = {
  light: "ag.cmd.light",
  clear: "ag.cmd.clear",
  progress: "ag.x.progress",
};

/** Why an agent's request was not carried out, in plain words. */
const ERROR_CODES = new Set([
  "internal",
  "bad_argument",
  "bad_color",
  "bad_duration",
  "bad_effect",
  "bad_keys",
  "unknown_key",
  "bad_signal",
  "bad_source",
  "bad_id",
  "bad_value",
  "bad_text",
  "text_too_long",
  "too_large",
  "app_not_running",
  "no_keyboard",
  "needs_usb",
  "unsupported",
  "pipe_not_trusted",
  "rate_limited",
  "budget_exhausted",
  "busy_flash_write",
  "timeout",
  "connection_lost",
  "protocol_mismatch",
  "unknown_command",
  "bad_request",
  "bad_response",
  "agent_access_off",
  "permission_off",
  "write_flash_flag_missing",
  "write_blocked",
]);

/** A refusal in plain words; a code a newer keymotion-cli adds falls back to "Not done.". */
function errorText(code: string, t: Translate): string {
  if (!ERROR_CODES.has(code)) return t("ag.err.other");
  return t(`ag.err.${code.replace(/_(\w)/g, (_, c: string) => c.toUpperCase())}` as MessageKey);
}

type ValueKind = "color" | "effect" | "duration" | "bool" | "speed" | "time" | "keys" | "nightMode" | "number";

/** The looks an agent may change, by key prefix. */
const LOOK_PREFIX: Record<string, MessageKey> = {
  "signals.done": "ag.sig.done",
  "signals.attention": "ag.sig.attention",
  "signals.error": "ag.sig.error",
  "signals.busy": "ag.sig.busy",
  "signals.info": "ag.sig.info",
  usage_limit: "ag.x.usage",
};
const LOOK_FIELD: Record<string, [MessageKey, ValueKind]> = {
  color: ["ag.field.color", "color"],
  effect: ["ag.field.effect", "effect"],
  speed: ["ag.field.speed", "speed"],
  duration: ["ag.field.duration", "duration"],
  until_answered: ["ag.field.untilAnswered", "bool"],
};
/** Every other setting the engine logs (config.rs): its section, its own name (none for a switch) and its kind. */
const OTHER_KEYS: Record<string, [MessageKey, MessageKey | null, ValueKind]> = {
  enabled: ["ag.access", null, "bool"],
  "permissions.screen": ["ag.screen", null, "bool"],
  "permissions.focus": ["ag.focus", null, "bool"],
  "permissions.profiles": ["ag.profiles", null, "bool"],
  agents_may_edit: ["ag.mayEdit", null, "bool"],
  hide_in_game: ["ag.hideInGame", null, "bool"],
  "screen.per_hour": ["ag.screen", "ag.field.perHour", "number"],
  "usage_limit.on": ["ag.x.usage", null, "bool"],
  "sessions.on": ["ag.x.sessions", null, "bool"],
  "sessions.color": ["ag.x.sessions", "ag.field.color", "color"],
  "away.on": ["ag.x.away", null, "bool"],
  "away.after": ["ag.x.away", "ag.x.away.after", "duration"],
  "unanswered.on": ["ag.x.unanswered", null, "bool"],
  "unanswered.every": ["ag.x.unanswered", "ag.x.unanswered.every", "duration"],
  "unanswered.up_to": ["ag.x.unanswered", "ag.x.unanswered.upTo", "duration"],
  "progress.keys": ["ag.x.progress", "ag.x.progress.keys", "keys"],
  "progress.color": ["ag.x.progress", "ag.field.color", "color"],
  "long_task.on": ["ag.x.longTask", null, "bool"],
  "long_task.after": ["ag.x.longTask", "ag.x.longTask.after", "duration"],
  "long_task.show": ["ag.x.longTask", "ag.x.longTask.show", "duration"],
  "night.on": ["ag.x.night", null, "bool"],
  "night.from": ["ag.x.night", "ag.x.night.from", "time"],
  "night.to": ["ag.x.night", "ag.x.night.to", "time"],
  "night.mode": ["ag.x.night", "ag.x.night.mode", "nightMode"],
  "helper.on": ["ag.x.helper", null, "bool"],
  "helper.color": ["ag.x.helper", "ag.field.color", "color"],
};

/** A setting key as people read it, and the kind of its values. */
function settingName(key: string, t: Translate): { label: string; kind: ValueKind | null } {
  const dot = key.lastIndexOf(".");
  const look = LOOK_PREFIX[key.slice(0, dot)];
  const field = LOOK_FIELD[key.slice(dot + 1)];
  if (look && field) {
    const name = key.startsWith("signals.") ? t("ag.log.signal", { name: t(look) }) : t(look);
    return { label: `${name} · ${t(field[0])}`, kind: field[1] };
  }
  const other = OTHER_KEYS[key];
  if (other) return { label: other[1] ? `${t(other[0])} · ${t(other[1])}` : t(other[0]), kind: other[2] };
  return { label: t("ag.log.otherSetting"), kind: null };
}

/** "2m", "30s", "1h", "500ms" → seconds. */
function seconds(text: string): number | null {
  const m = /^(\d+)(ms|s|m|h)$/.exec(text.trim());
  if (!m) return null;
  const n = Number(m[1]);
  return m[2] === "h" ? n * 3600 : m[2] === "m" ? n * 60 : m[2] === "s" ? n : Math.max(1, Math.round(n / 1000));
}

/** One value of a setting, as the page names it elsewhere. */
function SettingValue({ kind, value }: { kind: ValueKind | null; value: string }) {
  const t = useT();
  const span = useSpan();
  if (kind === "color" && /^#[0-9a-f]{6}$/i.test(value)) {
    const name = colorNameKey(rgb(value.toLowerCase()));
    return (
      <span className="inline-flex items-center gap-1.5" title={value}>
        <span className="h-3 w-3 rounded-full shadow-[0_0_0_1px_var(--line-strong)]" style={{ background: value }} aria-hidden />
        {name ? t(name) : value}
      </span>
    );
  }
  let text = value;
  if (kind === "effect") {
    const camel = value.replace(/_(\w)/g, (_, c: string) => c.toUpperCase());
    if ((EFFECTS as string[]).includes(camel)) text = t(`ag.fx.${camel}` as MessageKey);
  } else if (kind === "duration") {
    const s = seconds(value);
    if (s != null) text = span(s);
  } else if (kind === "bool") {
    if (value === "true" || value === "false") text = t(value === "true" ? "overview.on" : "overview.off");
  } else if (kind === "speed") {
    text = `${value}/5`;
  } else if (kind === "keys") {
    if (value === "f-row") text = t("ag.x.progress.fRow");
    else if (value === "numbers") text = t("ag.x.progress.numbers");
  } else if (kind === "nightMode") {
    if (value === "dim") text = t("ag.x.night.dim");
    else if (value === "off") text = t("ag.x.night.off");
  }
  return <span>{text}</span>;
}

/** A look as one line: "Green · Wave, then steady · 2 minutes". */
function lookLine(look: AgentLook, t: Translate, span: (s: number) => string): string {
  if (look.effect === "off") return t("ag.fx.off");
  const color = colorNameKey(look.color);
  return [color ? t(color) : hex(look.color), t(`ag.fx.${look.effect}` as MessageKey), look.untilAnswered ? t("ag.dur.untilAnswered") : span(look.durationS)].join(" · ");
}

function LogRow({
  entry: e,
  settings,
  sessions,
  onError,
}: {
  entry: AgentLogEntry;
  settings: AgentSettings;
  sessions: AgentSessionInfo[];
  onError: (m: string) => void;
}) {
  const t = useT();
  const span = useSpan();
  const locale = useApp((s) => s.locale);
  // Two sessions of one app are told apart by their project folder (or number).
  const session = sessions.find((x) => x.source === e.source);
  const tag = session?.label ?? (session?.number != null ? String(session.number) : null);
  const who = tag ? `${sourceName(e.source, t)} (${tag})` : sourceName(e.source, t);
  const event = EVENT_OF(e.id);
  const settingsChange = e.changes.length > 0;
  const signal = event && (SIGNALS as string[]).includes(event) ? (event as SignalId) : null;
  const look = signal ? settings.signals[signal] : event === "usage_limit" ? settings.usageLimit.look : null;
  const what = event && EVENT_NAME[event] ? t(EVENT_NAME[event]) : COMMAND_NAME[e.command] ? t(COMMAND_NAME[e.command]!) : t("ag.cmd.other");
  const title = settingsChange
    ? e.changes.length === 1
      ? t("ag.log.settings", { source: who })
      : t("ag.log.settingsMany", { source: who, count: e.changes.length })
    : `${who} · ${what}`;
  const desc = (
    <>
      {settingsChange &&
        e.changes.map((c) => {
          const { label, kind } = settingName(c.key, t);
          return (
            <span key={c.key} className="flex flex-wrap items-center gap-x-1.5" data-log-change={c.key}>
              <span>
                {label}
                {locale === "zh-CN" ? "：" : ":"}
              </span>
              <SettingValue kind={kind} value={c.before} />
              <ArrowRight size={12} aria-label="→" />
              <SettingValue kind={kind} value={c.after} />
            </span>
          );
        })}
      {!settingsChange && look && !e.error && <span className="block">{lookLine(look, t, span)}</span>}
      {e.error && <span className="block">{errorText(e.error, t)}</span>}
      {e.kept.length > 0 && (
        <span className="block">{t("ag.log.kept", { keys: e.kept.map((k) => settingName(k, t).label).join(", ") })}</span>
      )}
    </>
  );
  const lead = look ? (
    <span className="block h-[14px] w-[14px] rounded-full shadow-[0_0_0_3px_var(--subtle-sel)]" style={{ background: hex(look.color) }} aria-hidden />
  ) : e.error ? (
    <CircleAlert size={20} strokeWidth={1.4} className="text-danger" aria-hidden />
  ) : (
    <Settings2 size={20} strokeWidth={1.4} aria-hidden />
  );
  return (
    <Row lead={lead} title={title} desc={desc} data={`log-${e.seq}`}>
      <span className="num text-[12px] text-ink-2">{clockTime(e.atMs, locale)}</span>
      {settingsChange &&
        (e.undone ? (
          <span className="text-[13px] text-ink-2">{t("ag.undone")}</span>
        ) : (
          <button
            type="button"
            className="btn-secondary"
            onClick={() => undoAgentChange(e.seq).then(useAgents.getState().set, (x) => onError(friendlyError(x, t)))}
          >
            <Undo2 size={15} strokeWidth={1.6} aria-hidden />
            {t("ag.undo")}
          </button>
        ))}
    </Row>
  );
}
