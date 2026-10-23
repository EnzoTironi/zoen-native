import assert from "node:assert/strict";
import test from "node:test";
import { routeArgentAction } from "../qa-argent-choice.mjs";
import { argentBody, argentControls } from "../qa-argent-observation.mjs";

// Synthetic text from the installed 0.29 formatter contract, NOT device evidence.
const line = (options = {}) => {
  const {
    label = "Attach",
    id = "attach",
    flags = "",
    frame = "0.100, 0.800, 0.100, 0.100",
  } = options;
  return `  AXButton "${label}"${id ? ` id="${id}"` : ""}${flags ? ` [${flags}]` : ""}  (${frame})`;
};
const describe = (lines = [line()], source = "xcuitest-runner") => ({
  source,
  description: `Source: ${source}\nMode: nested\nCoordinates are normalized [0,1]\n\nROOT  AXGroup (0.000, 0.000, 1.000, 1.000)\n\n${lines.join("\n")}\n`,
});
const choice = (id = "attach") => ({
  structuredContent: {
    answers: {
      next_action: {
        type: "choice",
        choice: id,
        confidence: 0.8,
        probabilities: { attach: 0.8, insufficient_evidence: 0.2 },
      },
    },
  },
});
const candidate = {
  id: "attach",
  operation: "tap",
  selector: { role: "AXButton", identifier: "attach" },
};

function setup(options = {}) {
  let reads = 0;
  let decisions = 0;
  let dispatches = 0;
  let gates = 0;
  let request;
  const calls = [];
  const scope = {
    artifact: "synthetic-sealed-build",
    device: "synthetic-phone",
    lease: "exclusive-owner-1",
  };
  const recording = { name: "synthetic-recording", project_root: "/synthetic/project" };
  const snapshots = options.snapshots ?? [describe(), describe(), describe(), describe([])];
  const argent = {
    recording,
    readScope: async () => (options.readScope ? options.readScope(scope, reads) : { ...scope }),
    ready: async (action) => {
      gates++;
      return options.ready ? options.ready(action, gates) : true;
    },
    identifyScreen: () => ({
      screen: reads >= 4 ? "attachment-sheet" : "conversation",
      summary: "Only the relevant synthetic controls",
    }),
    redactLabel: (label) => (label === "Attach" ? label : "[redacted]"),
    async callTool(name, args) {
      calls.push({ name, args: structuredClone(args) });
      if (name === "await-screen-idle") return { settled: options.idle !== false };
      if (name === "describe") {
        const value = snapshots[reads++];
        if (value instanceof Error) throw value;
        return value;
      }
      assert.equal(name, "flow-add-step");
      dispatches++;
      if (options.execute) return options.execute(args);
      return {
        message: "Step added",
        toolResult: { tapped: true },
        stepCount: 2,
        recorded: '2. tap: {"id":"attach"}',
        savedTo: "/synthetic/recording.yaml",
      };
    },
  };
  return {
    calls,
    scope,
    recording,
    counts: () => ({ reads, decisions, dispatches, gates }),
    request: () => request,
    run: (patch = {}) =>
      routeArgentAction({
        objective: "Open the attachment sheet",
        expectedScreen: "attachment-sheet",
        candidates: options.candidates ?? [candidate],
        argent,
        decide: async (input) => {
          decisions++;
          request = input;
          return options.decide ? options.decide(input) : choice();
        },
        verify:
          options.verify ??
          (({ after }) => [
            {
              name: "destination identity",
              passed: after.screen === "attachment-sheet",
              observationId: after.id,
            },
          ]),
        ...patch,
      }),
  };
}

test("one actual-contract projection chooses via MCP and dispatches the existing recorder once", async () => {
  const harness = setup();
  const result = await harness.run();
  assert.equal(result.status, "assertions_passed");
  assert.deepEqual(harness.counts(), { reads: 4, decisions: 1, dispatches: 1, gates: 4 });
  const write = harness.calls.find((call) => call.name === "flow-add-step");
  assert.deepEqual(write.args, {
    name: "synthetic-recording",
    project_root: "/synthetic/project",
    command: "gesture-tap",
    args: JSON.stringify({
      udid: "synthetic-phone",
      x: 0.15000000000000002,
      y: 0.8500000000000001,
    }),
  });
  const input = harness.request();
  assert.equal(input.questions.next_action.type, "choice");
  assert.equal(input.state.candidates[0].label, "Attach");
  assert.equal("frame" in input.state.candidates[0], false);
  assert.equal("selector" in input.state.candidates[0], false);
  assert.equal(result.adapter.recorded, '2. tap: {"id":"attach"}');
});

test("unrelated clock/timeline/password values never enter the bounded evidence or freshness hash", async () => {
  const first = describe([line(), line({ id: "clock", label: "11:21 private token" })]);
  const second = describe([line(), line({ id: "clock", label: "11:22 changed private token" })]);
  const harness = setup({ snapshots: [first, second, describe(), describe([])] });
  assert.equal((await harness.run()).status, "assertions_passed");
  const serialized = JSON.stringify(harness.request());
  assert.equal(serialized.includes("private token"), false);
  assert.equal(serialized.includes("11:2"), false);
});

test("missing, disabled, zero-frame and sensitive targets abstain before inference", async () => {
  for (const snapshot of [
    describe([]),
    describe([line({ flags: "disabled" })]),
    describe([line({ frame: "0.100, 0.800, 0.000, 0.100" })]),
    describe([line({ flags: "password" })]),
  ]) {
    const harness = setup({ snapshots: [snapshot] });
    assert.equal((await harness.run()).reason, "no_current_action");
    assert.equal(harness.counts().decisions, 0);
    assert.equal(harness.counts().dispatches, 0);
  }
});

test("duplicate identifiers and exact-label matches are ambiguous, never guessed by order", async () => {
  for (const candidates of [
    [candidate],
    [{ ...candidate, selector: { role: "AXButton", label: "Attach" } }],
  ]) {
    const harness = setup({ candidates, snapshots: [describe([line(), line()])] });
    assert.equal((await harness.run()).reason, "ambiguous_argent_target");
    assert.equal(harness.counts().decisions, 0);
    assert.equal(harness.counts().dispatches, 0);
  }
});

test("a changed node between decision and either fresh read cannot dispatch", async () => {
  for (const changeAt of [1, 2]) {
    for (const changed of [
      describe([]),
      describe([line({ label: "Send" })]),
      describe([line({ flags: "disabled" })]),
      describe([line({ frame: "0.200, 0.800, 0.100, 0.100" })]),
    ]) {
      const snapshots = [describe(), describe(), describe(), describe([])];
      snapshots[changeAt] = changed;
      const harness = setup({ snapshots });
      const result = await harness.run();
      assert.notEqual(result.status, "assertions_passed");
      assert.equal(harness.counts().dispatches, 0);
    }
  }
});

test("artifact/device/lease replacement during a read or before dispatch cannot borrow the new owner", async () => {
  for (const key of ["artifact", "device", "lease"]) {
    const harness = setup({
      readScope: (scope, reads) => ({ ...scope, ...(reads >= 2 ? { [key]: "replacement" } : {}) }),
    });
    assert.notEqual((await harness.run()).status, "assertions_passed");
    assert.equal(harness.counts().dispatches, 0);
  }
  const harness = setup({ ready: (_action, call) => call < 4 });
  assert.equal((await harness.run()).reason, "driver_error");
  assert.equal(harness.counts().dispatches, 0);
});

test("unsupported simulator actionability evidence is explicit; no enabled boolean is invented", async () => {
  for (const source of ["ax-service", "native-devtools", "unknown-future-source"]) {
    const harness = setup({ snapshots: [describe([line()], source)] });
    assert.equal((await harness.run()).reason, "unsupported_actionability_source");
    assert.equal(harness.counts().decisions, 0);
    assert.equal(harness.counts().dispatches, 0);
  }
});

test("invalid operation, role-only selectors and authorization refusal spend no inference or action", async () => {
  for (const candidates of [
    [{ ...candidate, operation: "type" }],
    [{ ...candidate, selector: { role: "AXButton" } }],
  ]) {
    const harness = setup({ candidates });
    assert.equal((await harness.run()).reason, "invalid_argent_policy");
    assert.equal(harness.counts().decisions, 0);
  }
  const harness = setup({ ready: () => false });
  assert.equal((await harness.run()).reason, "not_authorized");
  assert.equal(harness.calls.length, 0);
});

test("an unsettled initial observation cannot be sent to the provider or tapped", async () => {
  const harness = setup({ idle: false });
  assert.equal((await harness.run()).reason, "not_idle");
  assert.equal(harness.counts().decisions, 0);
  assert.equal(harness.counts().dispatches, 0);
});

test("provider timeout resolves unknown once; a late billed response cannot act or repeat", async () => {
  let finish;
  const harness = setup({
    decide: () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  });
  const result = await harness.run({ decisionTimeoutMs: 5 });
  assert.equal(result.reason, "provider_error");
  assert.equal(result.adapter.error, "provider_timeout");
  finish(choice());
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(harness.counts().decisions, 1);
  assert.equal(harness.counts().dispatches, 0);
});

test("MCP errors, absent/invented choices and insufficient evidence never become actions", async () => {
  for (const response of [
    { isError: true, ...choice() },
    choice("unobserved"),
    choice("insufficient_evidence"),
  ]) {
    const harness = setup({ decide: () => response });
    assert.notEqual((await harness.run()).status, "assertions_passed");
    assert.equal(harness.counts().dispatches, 0);
    assert.equal(harness.counts().decisions, 1);
  }
});

test("recording object mutation cannot redirect the already-authorized recording", async () => {
  let harness;
  harness = setup({
    decide: () => {
      harness.recording.name = "other-recording";
      return choice();
    },
  });
  assert.equal((await harness.run()).status, "assertions_passed");
  assert.equal(
    harness.calls.find((call) => call.name === "flow-add-step").args.name,
    "synthetic-recording",
  );
});

test("lost dispatch replies, raw-point/role-only/wrong-selector receipts and failed writes remain failed without retry", async () => {
  for (const execute of [
    () => {
      throw new Error("delivered but reply lost");
    },
    () => ({ recorded: "2. tap: (0.15, 0.85)", savedTo: "/fixture.yaml" }),
    () => ({ recorded: '2. tap: {"role":"AXButton"}', savedTo: "/fixture.yaml" }),
    () => ({ recorded: '2. tap: {"id":"send"}', savedTo: "/fixture.yaml" }),
    () => ({ recorded: '2. tap: {"id":"attach"}', savedTo: null }),
  ]) {
    const harness = setup({ execute });
    const result = await harness.run();
    assert.equal(result.status, "failed");
    assert.equal(result.reason, "driver_error");
    assert.equal(harness.counts().dispatches, 1);
    assert.equal(
      result.checks[0].passed,
      true,
      "even a passing after-check cannot erase dispatch ambiguity",
    );
  }
});

test("actual-contract wrapper parsing rejects malformed/ambiguous evidence instead of treating plain text as nodes", () => {
  assert.deepEqual(argentBody({ content: [{ type: "text", text: '{"settled":true}' }] }), {
    settled: true,
  });
  assert.throws(() =>
    argentBody({
      content: [
        { type: "text", text: "{}" },
        { type: "text", text: "{}" },
      ],
    }),
  );
  assert.throws(() => argentBody({ isError: true, structuredContent: { settled: true } }));
  assert.throws(() =>
    argentControls(describe(['  AXButton "ambiguous "quote""  (0.100, 0.100, 0.100, 0.100)'])),
  );
  assert.throws(() => argentControls({ ...describe(), hint: "tree incomplete" }));
  assert.throws(() => argentControls(describe([line({ frame: "NaN, 0.100, 0.100, 0.100" })])));
});

test("actual MCP content envelopes and exact label-only targets use the existing semantic recorder receipt", async () => {
  const original = describe([line({ id: "" })]);
  const wrapped = { content: [{ type: "text", text: JSON.stringify(original) }] };
  const harness = setup({
    candidates: [{ ...candidate, selector: { role: "AXButton", label: "Attach" } }],
    snapshots: [wrapped, wrapped, wrapped, describe([])],
    execute: () => ({ recorded: '2. tap: {"text":"Attach"}', savedTo: "/synthetic/flow.yaml" }),
  });
  assert.equal((await harness.run()).status, "assertions_passed");
});

test("a recorder selector with extra constraints is not silently accepted as an exact replay target", async () => {
  const harness = setup({
    execute: () => ({
      recorded: '2. tap: {"id":"attach","text":"Send"}',
      savedTo: "/synthetic/flow.yaml",
    }),
  });
  assert.equal((await harness.run()).reason, "driver_error");
  assert.equal(harness.counts().dispatches, 1);
});

test("wrong native destination and failed deterministic postconditions cannot become a passing adapter step", async () => {
  const harness = setup({
    verify: ({ after }) => [{ name: "real effect absent", passed: false, observationId: after.id }],
  });
  assert.equal((await harness.run()).reason, "assertion_failed");
  const wrong = setup();
  assert.equal(
    (await wrong.run({ expectedScreen: "different-destination" })).reason,
    "wrong_destination",
  );
});
