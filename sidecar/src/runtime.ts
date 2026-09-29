// ── Which Node is running, and whether the SQLite driver matches it ───────────
//
// better-sqlite3 is a native addon compiled for one Node ABI. The shell
// launches the sidecar with whatever `node` it resolves, so the installed app
// can end up on a Node whose ABI the shipped binding was not built for. Node
// then refuses the addon with ERR_DLOPEN_FAILED and a message naming both
// ABIs, and only navdata is lost. This module turns that into something a
// person can act on: the Node that is running, the ABI the driver wants, and
// the Node release that provides it.
//
// Everything here is computed from numbers. Node's own message starts with the
// full install path of the addon, so no text from it is ever passed on.

import type { RuntimeStatus } from './protocol';

/** Node's ABI registry, release lines only: NODE_MODULE_VERSION to major. */
export const NODE_MAJOR_BY_ABI: Readonly<Record<number, number>> = {
  108: 18,
  111: 19,
  115: 20,
  120: 21,
  127: 22,
  131: 23,
  137: 24,
  141: 25,
  147: 26,
};

/** Node 20, 24 and 26 all word it this way, with line breaks in the gaps. */
const NODE_MODULE_VERSION_PATTERN =
  /NODE_MODULE_VERSION (\d+)\.\s+This version of Node\.js requires\s+NODE_MODULE_VERSION (\d+)/;

const MAX_ABI = 9999;
const MAX_NODE_VERSION_LENGTH = 32;

/** The versions this module reads; `process.versions` by default. */
export interface NodeVersions {
  readonly node: string;
  readonly modules: string;
}

/** What `describeRuntime` needs to know about the driver load. */
export type RuntimeDriverLoad =
  | { readonly ok: true }
  | { readonly ok: false; readonly failure: { readonly code: string; readonly driverAbi?: number } };

/**
 * The ABI the addon was built for, read from Node's refusal to load it, or
 * null when the error is anything else. Only the first number is used: the
 * running Node's ABI comes from `process.versions`, not from the message.
 */
export function parseBindingAbi(err: unknown): number | null {
  if (typeof err !== 'object' || err === null) return null;
  const { code, message } = err as { code?: unknown; message?: unknown };
  if (code !== 'ERR_DLOPEN_FAILED') return null;
  const match = NODE_MODULE_VERSION_PATTERN.exec(String(message));
  if (match === null) return null;
  const abi = Number(match[1]);
  return Number.isInteger(abi) && abi >= 1 && abi <= MAX_ABI ? abi : null;
}

/** The running Node's ABI as a number; 0 when it cannot be read as one. */
export function nodeAbiOf(versions: NodeVersions): number {
  const abi = Number.parseInt(versions.modules, 10);
  return Number.isInteger(abi) && abi >= 0 ? abi : 0;
}

/** The one line logged, and shown on the navdata axis, for an ABI mismatch. */
export function abiMismatchReason(
  nodeVersion: string,
  nodeAbi: number,
  driverAbi: number,
  requiredNodeMajor: number | null,
): string {
  const remedy =
    requiredNodeMajor === null
      ? `install the Node release for ABI ${driverAbi}`
      : `install Node ${requiredNodeMajor}`;
  return (
    `navdata disabled: Node ${nodeVersion} (ABI ${nodeAbi}) cannot load the SQLite ` +
    `driver built for ABI ${driverAbi}; ${remedy} or set nodePath in config.json`
  );
}

/** The `runtime` block every status line carries. */
export function describeRuntime(
  load: RuntimeDriverLoad,
  versions: NodeVersions = process.versions,
): RuntimeStatus {
  const nodeVersion = String(versions.node).slice(0, MAX_NODE_VERSION_LENGTH);
  const nodeAbi = nodeAbiOf(versions);
  if (load.ok) {
    return { nodeVersion, nodeAbi, driver: 'ok', driverAbi: null, requiredNodeMajor: null };
  }
  const driverAbi = load.failure.driverAbi;
  if (driverAbi === undefined) {
    return { nodeVersion, nodeAbi, driver: 'failed', driverAbi: null, requiredNodeMajor: null };
  }
  return {
    nodeVersion,
    nodeAbi,
    driver: 'abi-mismatch',
    driverAbi,
    requiredNodeMajor: NODE_MAJOR_BY_ABI[driverAbi] ?? null,
  };
}
