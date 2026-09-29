// tests/runtime.test.ts — tests src/runtime.ts.
//
// The driver's ABI is read out of Node's own dlopen message, so the messages
// here follow the exact wording Node 20, 24 and 26 print, line breaks and all.
// The install path in front of that wording is synthetic: what matters is that
// it is there, because the reason text must never repeat it.

import { describe, expect, it } from 'vitest';
import {
  abiMismatchReason,
  describeRuntime,
  NODE_MAJOR_BY_ABI,
  parseBindingAbi,
} from '../src/runtime';

function dlopenError(built: number, running: number): Error {
  return Object.assign(
    new Error(
      "The module '\\\\?\\C:\\Program Files\\Sabia\\sidecar\\node_modules\\better-sqlite3\\build\\Release\\better_sqlite3.node'\n" +
        'was compiled against a different Node.js version using\n' +
        `NODE_MODULE_VERSION ${built}. This version of Node.js requires\n` +
        `NODE_MODULE_VERSION ${running}. Please try re-compiling or re-installing\n` +
        'the module (for instance, using `npm rebuild` or `npm install`).',
    ),
    { code: 'ERR_DLOPEN_FAILED' },
  );
}

const NODE_20 = { node: '20.20.2', modules: '115' };

describe('the driver ABI', () => {
  it("parses the binding ABI from Node's NODE_MODULE_VERSION message", () => {
    // A Node-20 binding under Node 24, and a Node-24 binding under Node 20.
    expect(parseBindingAbi(dlopenError(115, 137))).toBe(115);
    expect(parseBindingAbi(dlopenError(137, 115))).toBe(137);
  });

  it('does not read an ABI out of other load failures', () => {
    const noRequires = Object.assign(new Error('compiled against NODE_MODULE_VERSION 108'), {
      code: 'ERR_DLOPEN_FAILED',
    });
    expect(parseBindingAbi(noRequires)).toBeNull();

    const missing = Object.assign(dlopenError(115, 137), { code: 'MODULE_NOT_FOUND' });
    expect(parseBindingAbi(missing)).toBeNull();

    expect(parseBindingAbi(undefined)).toBeNull();
    expect(parseBindingAbi(null)).toBeNull();
    expect(parseBindingAbi('NODE_MODULE_VERSION 115. This version of Node.js requires NODE_MODULE_VERSION 137')).toBeNull();
    expect(parseBindingAbi(42)).toBeNull();

    // Numbers outside what an ABI can be are not an ABI.
    expect(parseBindingAbi(dlopenError(0, 137))).toBeNull();
    expect(parseBindingAbi(dlopenError(10000, 137))).toBeNull();
  });

  it('maps ABIs to Node majors from the registry', () => {
    expect(NODE_MAJOR_BY_ABI).toEqual({
      108: 18, 111: 19, 115: 20, 120: 21, 127: 22, 131: 23, 137: 24, 141: 25, 147: 26,
    });
  });
});

describe('the runtime block', () => {
  it('describes the runtime for ok, abi-mismatch, unknown ABI and failed loads', () => {
    expect(describeRuntime({ ok: true }, NODE_20)).toEqual({
      nodeVersion: '20.20.2', nodeAbi: 115, driver: 'ok', driverAbi: null, requiredNodeMajor: null,
    });

    expect(
      describeRuntime({ ok: false, failure: { code: 'ERR_DLOPEN_FAILED', driverAbi: 137 } }, NODE_20),
    ).toEqual({
      nodeVersion: '20.20.2', nodeAbi: 115, driver: 'abi-mismatch', driverAbi: 137, requiredNodeMajor: 24,
    });

    expect(
      describeRuntime({ ok: false, failure: { code: 'ERR_DLOPEN_FAILED', driverAbi: 999 } }, NODE_20),
    ).toEqual({
      nodeVersion: '20.20.2', nodeAbi: 115, driver: 'abi-mismatch', driverAbi: 999, requiredNodeMajor: null,
    });

    expect(
      describeRuntime({ ok: false, failure: { code: 'ERR_DLOPEN_FAILED' } }, NODE_20),
    ).toEqual({
      nodeVersion: '20.20.2', nodeAbi: 115, driver: 'failed', driverAbi: null, requiredNodeMajor: null,
    });
    expect(
      describeRuntime({ ok: false, failure: { code: 'MODULE_NOT_FOUND' } }, NODE_20),
    ).toEqual({
      nodeVersion: '20.20.2', nodeAbi: 115, driver: 'failed', driverAbi: null, requiredNodeMajor: null,
    });

    // A modules string that is not a number is reported as 0, and a version
    // string is never longer than the wire allows.
    const odd = describeRuntime({ ok: true }, { node: '9'.repeat(40), modules: 'abc' });
    expect(odd.nodeAbi).toBe(0);
    expect(odd.nodeVersion).toHaveLength(32);

    // With no versions given, it describes the node running this test.
    expect(describeRuntime({ ok: true })).toEqual({
      nodeVersion: process.versions.node,
      nodeAbi: Number(process.versions.modules),
      driver: 'ok',
      driverAbi: null,
      requiredNodeMajor: null,
    });
  });

  it('phrases the mismatch from the numbers alone', () => {
    expect(abiMismatchReason('20.20.2', 115, 137, 24)).toBe(
      'navdata disabled: Node 20.20.2 (ABI 115) cannot load the SQLite driver built for ABI 137; install Node 24 or set nodePath in config.json',
    );
    expect(abiMismatchReason('20.20.2', 115, 999, null)).toBe(
      'navdata disabled: Node 20.20.2 (ABI 115) cannot load the SQLite driver built for ABI 999; install the Node release for ABI 999 or set nodePath in config.json',
    );
  });
});
