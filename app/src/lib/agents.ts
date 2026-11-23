// The AI agents page's link to the backend: the agents' settings, lights,
// sessions and log (`agents_*` commands and the `agents` event), and the
// coding apps it connects (Claude Code, Codex).
//
// Browser mock (development only): `?agents=off` (default: access off, both
// apps found, not connected), `?agents=on` (on and connected, Codex waiting
// for its approval), `?agents=none` (neither app on this computer),
// `?agents=stale` (the first approval finds a changed file and shows the
// plan again).

import { create } from "zustand";
import type { AgentLook } from "../bindings/AgentLook";
import type { AgentSettings } from "../bindings/AgentSettings";
import type { AgentsState } from "../bindings/AgentsState";
import type {
  AgentApp,
  AgentAppPlan,
  AgentAppStatus,
  AgentConnectOutcome,
  AgentConnectPlan,
  AgentConnections,
  AgentDisconnectOutcome,
  AgentFileAction,
} from "./agentTypes";
import { inTauri, invoke, listen } from "./ipc";

// ------------------------------------------------------------------ calls

export function getAgents(): Promise<AgentsState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.state());
  return invoke<AgentsState>("agents_state");
}

export function saveAgents(settings: AgentSettings): Promise<AgentsState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.save(settings));
  return invoke<AgentsState>("agents_save", { settings });
}

/** "Try on the keyboard": one look for a few seconds (works with access off). */
export function tryAgentLook(look: AgentLook, keys?: string): Promise<AgentsState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.state());
  return invoke<AgentsState>("agents_try", { look, keys: keys ?? null });
}

export function clearAgentLights(): Promise<AgentsState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.clear());
  return invoke<AgentsState>("agents_clear_all");
}

export function undoAgentChange(seq: number): Promise<AgentsState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.undo(seq));
  return invoke<AgentsState>("agents_undo", { seq });
}

export function getAgentConnections(): Promise<AgentConnections> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.connections());
  return invoke<AgentConnections>("agents_connections");
}

export function agentConnectPlan(apps?: AgentApp[]): Promise<AgentConnectPlan> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.plan(apps));
  return invoke<AgentConnectPlan>("agents_connect_plan", { apps: apps ?? null });
}

export function agentConnect(token: string, apps?: AgentApp[]): Promise<AgentConnectOutcome> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.connect(token, apps));
  return invoke<AgentConnectOutcome>("agents_connect", { token, apps: apps ?? null });
}

export function agentDisconnectPlan(app: AgentApp): Promise<AgentAppPlan> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.disconnectPlan(app));
  return invoke<AgentAppPlan>("agents_disconnect_plan", { app });
}

export function agentDisconnect(app: AgentApp): Promise<AgentDisconnectOutcome> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mock.disconnect(app));
  return invoke<AgentDisconnectOutcome>("agents_disconnect", { app });
}

// ------------------------------------------------------------------ store

interface AgentsStore {
  state: AgentsState | null;
  connections: AgentConnections | null;
  load: () => Promise<void>;
  loadConnections: () => Promise<void>;
  /** Save a change to the settings (shown at once, the backend answers with the same). */
  configure: (patch: Partial<AgentSettings>) => Promise<void>;
  set: (state: AgentsState) => void;
  setConnections: (c: AgentConnections) => void;
}

export const useAgents = create<AgentsStore>((set, get) => ({
  state: null,
  connections: null,
  load: async () => set({ state: await getAgents() }),
  loadConnections: async () => set({ connections: await getAgentConnections() }),
  configure: async (patch) => {
    const s = get().state;
    if (!s) return;
    const settings = { ...s.settings, ...patch };
    set({ state: { ...s, settings } });
    set({ state: await saveAgents(settings) });
  },
  set: (state) => set({ state }),
  setConnections: (connections) => set({ connections }),
}));

/** Follow the backend: the agents change lights and settings by themselves. */
export function followAgents(): () => void {
  if (import.meta.env.DEV && !inTauri) {
    mock.emit = (s) => useAgents.setState({ state: s });
    return () => {
      mock.emit = null;
    };
  }
  const off = listen<AgentsState>("agents", (e) => useAgents.setState({ state: e.payload }));
  return () => {
    off.then((f) => f()).catch(console.error);
  };
}

// ------------------------------------------------------------- the mock

const mock = (() => {
  const param = typeof window !== "undefined" ? new URLSearchParams(window.location.search).get("agents") : null;
  const on = param === "on";
  const none = param === "none";
  let stale = param === "stale";
  const home = "C:\\Users\\you";
  const look = (color: [number, number, number], effect: AgentLook["effect"], durationS: number, untilAnswered = false): AgentLook => ({
    color,
    effect,
    speed: 3,
    durationS,
    untilAnswered,
  });
  let settings: AgentSettings = {
    enabled: on,
    screen: false,
    focus: false,
    profiles: false,
    agentsMayEdit: true,
    hideInGame: true,
    screenPerHour: 4,
    signals: {
      done: look([63, 207, 142], "waveThenSolid", 120),
      attention: look([255, 138, 61], "flashThenPulse", 1800, true),
      error: look([242, 84, 91], "blinkTwiceThenSolid", 600),
      busy: look([255, 255, 255], "off", 60),
      info: look([76, 157, 255], "wave", 10),
    },
    usageLimit: { on: true, look: look([167, 110, 255], "pulse", 1800) },
    sessions: { on: true, color: [255, 255, 255] },
    away: { on: true, afterS: 300 },
    unanswered: { on: true, everyS: 300, upToS: 1800 },
    progress: { keys: "fRow", color: [76, 157, 255] },
    longTask: { on: true, afterS: 600, showS: 300 },
    night: { on: false, fromMin: 23 * 60, toMin: 8 * 60, mode: "dim" },
    helper: { on: false, color: [76, 157, 255] },
  };
  const now = Date.now();
  let log: AgentsState["log"] = on
    ? [
        {
          seq: 3,
          atMs: now - 6 * 60_000,
          source: "claude-code:3f9a0c1d",
          command: "settings_set",
          error: null,
          id: null,
          changes: [{ key: "signals.done.color", before: "#3fcf8e", after: "#4c9dff" }],
          undone: false,
          kept: [],
        },
        { seq: 2, atMs: now - 13 * 60_000, source: "claude-code:3f9a0c1d", command: "signal", error: null, id: "claude-code:3f9a0c1d:done", changes: [], undone: false, kept: [] },
        { seq: 1, atMs: now - 20 * 60_000, source: "codex:91be", command: "signal", error: null, id: "codex:91be:attention", changes: [], undone: false, kept: [] },
      ]
    : [];
  const sessions: AgentsState["sessions"] = on
    ? [
        { number: 1, source: "claude-code:3f9a0c1d", agent: "claude-code", label: "keymotion", waiting: false },
        { number: 2, source: "codex:91be", agent: "codex", label: "site", waiting: true },
      ]
    : [];
  const apps: Record<AgentApp, AgentAppStatus> = {
    claudeCode: {
      app: "claudeCode",
      name: "Claude Code",
      found: !none,
      state: none ? "notFound" : on ? "connected" : "notConnected",
      hooks: on ? 16 : 0,
      hooksWanted: 16,
      trusted: null,
      version: none ? null : "2.1.140",
      tooOld: false,
      minVersion: "2.1.139",
      home: `${home}\\.claude`,
      hooksFile: `${home}\\.claude\\settings.json`,
      skillFile: `${home}\\.claude\\skills\\keymotion\\SKILL.md`,
      runs: on ? `${home}\\AppData\\Local\\Programs\\Keymotion\\bin\\keymotion-cli.exe` : null,
      error: null,
    },
    codex: {
      app: "codex",
      name: "Codex",
      found: !none,
      state: none ? "notFound" : on ? "needsApproval" : "notConnected",
      hooks: on ? 10 : 0,
      hooksWanted: 10,
      trusted: on ? 0 : null,
      version: null,
      tooOld: false,
      minVersion: null,
      home: `${home}\\.codex`,
      hooksFile: `${home}\\.codex\\hooks.json`,
      skillFile: `${home}\\.agents\\skills\\keymotion\\SKILL.md`,
      runs: on ? `${home}\\AppData\\Local\\Programs\\Keymotion\\bin\\keymotion-cli.exe` : null,
      error: null,
    },
  };
  const cli = { path: `${home}\\AppData\\Local\\Programs\\Keymotion\\bin\\keymotion-cli.exe`, kind: "installed" as const, present: true };
  let path = { dir: `${home}\\AppData\\Local\\Programs\\Keymotion\\bin`, onPath: on, error: null as string | null };

  const state = (): AgentsState => ({
    settings,
    signals: [],
    sessions,
    log,
    server: { running: true, error: null, connections: 0 },
    lights: "shown",
  });
  const connections = (): AgentConnections => ({ apps: [apps.claudeCode, apps.codex], cli, path });
  const appPlan = (app: AgentApp, remove = false): AgentAppPlan => {
    const a = apps[app];
    const action: AgentFileAction = remove ? "remove" : "create";
    const changes =
      app === "claudeCode"
        ? [
            { kind: "hooks" as const, action: remove ? ("update" as const) : ("update" as const), path: a.hooksFile, summary: `${remove ? "remove" : "add"} 16 hooks that run keymotion-cli`, hooks: 16, backup: remove ? null : `${a.hooksFile}.keymotion-backup` },
            { kind: "skill" as const, action, path: a.skillFile, summary: `${remove ? "remove" : "add"} the keymotion skill`, hooks: null, backup: null },
          ]
        : [
            { kind: "hooks" as const, action, path: a.hooksFile, summary: `${remove ? "remove" : "add"} 10 hooks that run keymotion-cli`, hooks: 10, backup: null },
            { kind: "skill" as const, action, path: a.skillFile, summary: `${remove ? "remove" : "add"} the keymotion skill`, hooks: null, backup: null },
            { kind: "rule" as const, action, path: `${a.home}\\rules\\keymotion.rules`, summary: `${remove ? "remove" : "add"} a rule that lets keymotion-cli run outside the sandbox`, hooks: null, backup: null },
            { kind: "note" as const, action, path: `${a.home}\\keymotion\\may-start.json`, summary: "Keymotion's own note", hooks: null, backup: null },
          ];
    return {
      app,
      name: a.name,
      home: a.home,
      changes,
      commandLine: `"${cli.path}" hook ${app === "claudeCode" ? "claude-code" : "codex"}`,
      needsApproval: app === "codex" && !remove,
      warnings: [],
      problem: null,
    };
  };
  const plan = (want?: AgentApp[]): AgentConnectPlan => {
    const asked = (want ?? (["claudeCode", "codex"] as AgentApp[])).filter((a) => apps[a].found && apps[a].state !== "connected" && apps[a].state !== "needsApproval");
    return {
      token: `mock-${Date.now()}`,
      apps: asked.map((a) => appPlan(a)),
      notFound: (["claudeCode", "codex"] as AgentApp[]).filter((a) => !apps[a].found),
      path: path.onPath ? null : path,
      cli,
      empty: asked.length === 0 && path.onPath,
    };
  };
  const api = {
    emit: null as ((s: AgentsState) => void) | null,
    state,
    connections,
    plan,
    save(next: AgentSettings) {
      settings = next;
      return state();
    },
    clear() {
      return state();
    },
    undo(n: number) {
      log = log.map((e) => (e.seq === n ? { ...e, undone: true } : e));
      return state();
    },
    connect(_token: string, want?: AgentApp[]): AgentConnectOutcome {
      if (stale) {
        stale = false;
        return { applied: false, plan: plan(want), connected: [], failed: [], pathAdded: false, pathError: null, saveError: null, connections: connections(), state: state() };
      }
      const p = plan(want);
      for (const a of p.apps) {
        apps[a.app] = { ...apps[a.app], state: a.app === "codex" ? "needsApproval" : "connected", hooks: apps[a.app].hooksWanted, trusted: a.app === "codex" ? 0 : null, runs: cli.path };
      }
      const pathAdded = !path.onPath;
      path = { ...path, onPath: true };
      settings = { ...settings, enabled: true };
      return { applied: true, plan: null, connected: p.apps.map((a) => a.app), failed: [], pathAdded, pathError: null, saveError: null, connections: connections(), state: state() };
    },
    disconnectPlan(app: AgentApp) {
      return appPlan(app, true);
    },
    disconnect(app: AgentApp): AgentDisconnectOutcome {
      const p = appPlan(app, true);
      apps[app] = { ...apps[app], state: "notConnected", hooks: 0, trusted: null, runs: null };
      return { plan: p, connections: connections() };
    },
  };
  return api;
})();
