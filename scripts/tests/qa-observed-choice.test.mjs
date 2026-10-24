import assert from "node:assert/strict";
import test from "node:test";
import { routeObservedAction } from "../qa-observed-choice.mjs";

const candidate = { id: "open_attachment", targetId: "native-node-12", operation: "tap" };
const node = {
  id: candidate.targetId,
  role: "button",
  label: "Attach",
  visible: true,
  enabled: true,
};
const observation = (sequence, overrides = {}) => ({
  id: `snapshot-${sequence}`,
  sequence,
  artifact: "synthetic-artifact",
  device: "synthetic-simulator",
  lease: "window+account+focus-1",
  screen: "conversation-qa",
  idle: true,
  summary: "Synthetic conversation with attachment button",
  nodes: [structuredClone(node)],
  ...overrides,
});
const answer = (choice = candidate.id, probability = 0.8) => ({
  answers: {
    next_action: {
      type: "choice",
      choice,
      confidence: probability,
      probabilities: {
        open_attachment: choice === candidate.id ? probability : 1 - probability,
        insufficient_evidence: choice === candidate.id ? 1 - probability : probability,
      },
    },
  },
});

function setup({ snapshots, decide, ready, execute, verify, candidates } = {}) {
  const counters = { reads: 0, decisions: 0, writes: 0, checks: 0, gates: 0 };
  const states = snapshots ?? [
    observation(1),
    observation(2),
    observation(3, { screen: "attachment-sheet" }),
  ];
  let request;
  const driver = {
    async ready(action) {
      counters.gates++;
      return ready ? ready(action, counters.gates) : true;
    },
    async observe() {
      const state = states[counters.reads++];
      if (state instanceof Error) throw state;
      return structuredClone(state);
    },
    async execute(action, scope) {
      counters.writes++;
      assert.deepEqual(action, candidate);
      assert.deepEqual(scope, {
        observationId: "snapshot-2",
        artifact: "synthetic-artifact",
        device: "synthetic-simulator",
        lease: "window+account+focus-1",
      });
      if (execute) return execute(action, scope);
    },
  };
  return {
    counters,
    request: () => request,
    run: (overrides = {}) =>
      routeObservedAction({
        objective: "Open attachments",
        expectedScreen: "attachment-sheet",
        candidates: candidates ?? [candidate],
        driver,
        decide: async (input) => {
          request = input;
          counters.decisions++;
          return decide ? decide(input) : { structuredContent: answer(), isError: false };
        },
        verify: async (input) => {
          counters.checks++;
          return verify
            ? verify(input)
            : [{ name: "gallery grid visible", passed: true, observationId: input.after.id }];
        },
        ...overrides,
      }),
  };
}

test("one observed choice executes once and current destination/effect assertions are retained", async () => {
  const harness = setup();
  const result = await harness.run();
  assert.equal(result.status, "assertions_passed");
  assert.equal(result.actionAttempted, true);
  assert.deepEqual(harness.counters, { reads: 3, decisions: 1, writes: 1, checks: 1, gates: 2 });
  const input = harness.request();
  assert.equal(input.questions.next_action.type, "choice");
  assert.deepEqual(Object.keys(input.questions.next_action.criteria), [
    "open_attachment",
    "insufficient_evidence",
  ]);
  assert.equal(input.state.observation_id, "snapshot-1");
  assert.match(input.state.observation_hash, /^[a-f0-9]{64}$/);
  assert.deepEqual(result.decision.probabilities, answer().answers.next_action.probabilities);
  assert.equal(result.checks[0].observationId, "snapshot-3");
});

test("insufficient evidence abstains; confidence never supplies a default tap", async () => {
  const harness = setup({ decide: () => answer("insufficient_evidence", 1) });
  const result = await harness.run();
  assert.equal(result.status, "abstained");
  assert.equal(result.reason, "insufficient_evidence");
  assert.equal(harness.counters.writes, 0);
  assert.equal(harness.counters.reads, 1);
});

test("provider errors and malformed choices never retry or execute", async () => {
  for (const decide of [
    () => {
      throw new Error("timeout; do not repeat a possibly billed dispatch");
    },
    () => ({ isError: true, structuredContent: answer() }),
    () => ({ ...answer(), partial_failure: true }),
    () => ({ structuredContent: { ...answer(), partial_failure: true } }),
    () => answer("invented_selector"),
    () => ({ answers: { next_action: { ...answer().answers.next_action, probabilities: {} } } }),
    () => ({ answers: { next_action: { ...answer().answers.next_action, confidence: NaN } } }),
  ]) {
    const harness = setup({ decide });
    const result = await harness.run();
    assert.equal(result.status, "unknown");
    assert.ok(["provider_error", "invalid_choice"].includes(result.reason));
    assert.equal(harness.counters.decisions, 1);
    assert.equal(harness.counters.writes, 0);
  }
});

test("only observed visible enabled IDs and supported operations enter the choice", async () => {
  for (const candidates of [
    [{ ...candidate, targetId: "invented" }],
    [candidate, candidate],
    [{ ...candidate, id: "insufficient_evidence" }],
    [{ ...candidate, id: "bad-choice" }],
    [{ ...candidate, operation: "arbitrary_command" }],
  ]) {
    const harness = setup({ candidates });
    assert.equal((await harness.run()).reason, "invalid_observation_or_candidates");
    assert.equal(harness.counters.decisions, 0);
    assert.equal(harness.counters.writes, 0);
  }
  for (const property of ["visible", "enabled"]) {
    const harness = setup({
      snapshots: [observation(1, { nodes: [{ ...node, [property]: false }] })],
    });
    assert.equal((await harness.run()).reason, "no_current_action");
    assert.equal(harness.counters.decisions, 0);
    assert.equal(harness.counters.writes, 0);
  }
});

test("all pre-dispatch scope, screen, node changes and replayed observations retire the choice", async () => {
  const replacements = [
    observation(2, { artifact: "different-build" }),
    observation(2, { device: "different-device" }),
    observation(2, { lease: "new-account-or-focus-window" }),
    observation(2, { screen: "different-screen" }),
    observation(2, { summary: "different observed content" }),
    observation(2, { idle: false }),
    observation(2, { nodes: [] }),
    observation(2, { nodes: [{ ...node, label: "Send" }] }),
    observation(2, { nodes: [{ ...node, enabled: false }] }),
    observation(2, { nodes: [{ ...node, visible: false }] }),
    observation(2, { nodes: [{ ...node, role: "destructive-button" }] }),
    observation(2, { id: "snapshot-1" }),
    observation(2, { sequence: 1 }),
  ];
  for (const fresh of replacements) {
    const harness = setup({ snapshots: [observation(1), fresh] });
    assert.equal((await harness.run()).reason, "stale_observation");
    assert.equal(harness.counters.writes, 0);
  }
});

test("operator/artifact authorization is checked before deciding and immediately before executing", async () => {
  for (const refuseAt of [1, 2]) {
    const harness = setup({
      ready: (action, call) => {
        if (call === 2) assert.deepEqual(action, candidate);
        return call !== refuseAt;
      },
    });
    assert.equal((await harness.run()).reason, "not_authorized");
    assert.equal(harness.counters.writes, 0);
    assert.equal(harness.counters.decisions, refuseAt === 1 ? 0 : 1);
  }
});

test("decision callback cannot mutate retained candidates into a different target", async () => {
  const candidates = [structuredClone(candidate)];
  const harness = setup({
    candidates,
    decide: () => {
      candidates[0].targetId = "send";
      return answer();
    },
  });
  assert.equal((await harness.run()).status, "assertions_passed");
});

test("provider bridge cannot rewrite the retained target or candidate allowlist", async () => {
  const harness = setup({
    decide: (request) => {
      request.state.candidates[0].targetId = "invented-send";
      request.questions.next_action.criteria.injected = "run arbitrary command";
      return answer();
    },
  });
  const result = await harness.run();
  assert.equal(result.status, "assertions_passed");
  assert.equal(result.action.targetId, candidate.targetId);
});

test("high confidence cannot override failed, absent or stale assertions", async () => {
  for (const [verify, reason] of [
    [
      ({ after }) => [{ name: "exact effect", passed: false, observationId: after.id }],
      "assertion_failed",
    ],
    [() => [], "invalid_assertions"],
    [() => [{ name: "effect", passed: true, observationId: "snapshot-1" }], "invalid_assertions"],
    [
      () => {
        throw new Error("native assertion failed");
      },
      "assertion_error",
    ],
  ]) {
    const harness = setup({ decide: () => answer(candidate.id, 1), verify });
    const result = await harness.run();
    assert.equal(result.status, "failed");
    assert.equal(result.reason, reason);
    assert.equal(harness.counters.writes, 1);
  }
});

test("the actual new destination must be idle and scoped; a stale snapshot cannot pass", async () => {
  for (const [after, reason] of [
    [observation(3, { screen: "wrong-sheet" }), "wrong_destination"],
    [observation(3, { screen: "attachment-sheet", idle: false }), "not_idle"],
    [observation(3, { lease: "new-owner" }), "stale_effect_observation"],
    [observation(3, { artifact: "other-build" }), "stale_effect_observation"],
    [observation(3, { device: "other-device" }), "stale_effect_observation"],
    [observation(3, { id: "snapshot-2" }), "stale_effect_observation"],
    [observation(3, { sequence: 2 }), "stale_effect_observation"],
    [new Error("snapshot unavailable"), "post_observation_failed"],
  ]) {
    const harness = setup({ snapshots: [observation(1), observation(2), after] });
    const result = await harness.run();
    assert.equal(result.status, "failed");
    assert.equal(result.reason, reason);
    assert.equal(harness.counters.checks, 0);
  }
});

test("an ambiguous driver exception is observed but never retried or erased by passing assertions", async () => {
  const harness = setup({
    execute: () => {
      throw new Error("delivered but reply lost");
    },
  });
  const result = await harness.run();
  assert.equal(result.status, "failed");
  assert.equal(result.reason, "driver_error");
  assert.equal(result.executionFailed, true);
  assert.equal(result.checks[0].passed, true);
  assert.equal(harness.counters.writes, 1);
  assert.equal(harness.counters.reads, 3);
});

test("a trusted candidate projection receives an isolated observation and cannot rewrite the retained tree", async () => {
  const harness = setup();
  const result = await harness.run({
    candidates: (observed) => {
      assert.equal(observed.nodes[0].id, candidate.targetId);
      observed.nodes[0].id = "invented";
      observed.lease = "rewritten-owner";
      return [candidate];
    },
  });
  assert.equal(result.status, "assertions_passed");
  assert.equal(harness.counters.writes, 1);
});
