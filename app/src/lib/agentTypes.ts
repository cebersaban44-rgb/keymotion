// The connect/disconnect types of the AI agents page, exactly as the
// backend's bindings (PR #155, `app/src/bindings/Agent*.ts`) define them.
// Once that is merged, these become re-exports of the generated files and
// nothing else changes.

import type { AgentsState } from "../bindings/AgentsState";

/** A coding app Keymotion connects to. */
export type AgentApp = "claudeCode" | "codex";

/** How a coding app stands with Keymotion. */
export type AgentAppState = "notFound" | "notConnected" | "connected" | "needsApproval" | "outdated" | "unreadable";

/** Which keymotion-cli this copy of Keymotion comes with. */
export type AgentCliKind = "store" | "installed" | "dev";

/** The keymotion-cli the hooks run. */
export type AgentCli = { path: string; kind: AgentCliKind; present: boolean };

/** The GitHub copy's `bin` folder and the user's PATH. */
/** `error`: PATH could not be read (nothing is written). */
export type AgentPathInfo = { dir: string; onPath: boolean; error: string | null };

/** A coding app, as the AI agents page shows it. */
export type AgentAppStatus = {
  app: AgentApp;
  name: string;
  found: boolean;
  state: AgentAppState;
  hooks: number;
  hooksWanted: number;
  trusted: number | null;
  version: string | null;
  tooOld: boolean;
  minVersion: string | null;
  home: string;
  hooksFile: string;
  skillFile: string;
  runs: string | null;
  error: string | null;
};

/** The coding apps and the command line tool (`agents_connections`). */
export type AgentConnections = { apps: Array<AgentAppStatus>; cli: AgentCli | null; path: AgentPathInfo | null };

export type AgentFileKind = "hooks" | "skill" | "rule" | "note";
export type AgentFileAction = "create" | "update" | "remove";

/** One file connecting or disconnecting changes. */
export type AgentFileChange = {
  kind: AgentFileKind;
  action: AgentFileAction;
  path: string;
  summary: string;
  hooks: number | null;
  backup: string | null;
};

export type AgentProblemKind = "invalid" | "io" | "unsupported" | "changed" | "noCli" | "partial";
export type AgentProblem = { kind: AgentProblemKind; message: string };

/** What connecting (or disconnecting) changes for one coding app. */
export type AgentAppPlan = {
  app: AgentApp;
  name: string;
  home: string;
  changes: Array<AgentFileChange>;
  commandLine: string | null;
  needsApproval: boolean;
  warnings: Array<string>;
  problem: AgentProblem | null;
};

/** What turning agent access on would add (`agents_connect_plan`). */
export type AgentConnectPlan = {
  token: string;
  apps: Array<AgentAppPlan>;
  notFound: Array<AgentApp>;
  path: AgentPathInfo | null;
  cli: AgentCli | null;
  empty: boolean;
};

/** What came of `agents_connect`. */
export type AgentConnectOutcome = {
  applied: boolean;
  plan: AgentConnectPlan | null;
  connected: Array<AgentApp>;
  failed: Array<AgentAppPlan>;
  pathAdded: boolean;
  pathError: string | null;
  /** The connections were written but saving the settings failed. */
  saveError: string | null;
  connections: AgentConnections;
  state: AgentsState;
};

/** What came of `agents_disconnect`. */
export type AgentDisconnectOutcome = { plan: AgentAppPlan; connections: AgentConnections };
