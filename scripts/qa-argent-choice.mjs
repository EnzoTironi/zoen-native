// One adapter over the existing QA guard and Argent recorder, not a flow runner.
import { createHash, randomUUID } from "node:crypto";
import { isAbsolute } from "node:path";
import { routeObservedAction } from "./qa-observed-choice.mjs";
import { argentBody, argentControls } from "./qa-argent-observation.mjs";

const hash = (value) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const same = (left, right) =>
  ["artifact", "device", "lease"].every((key) => left[key] === right[key]);
const validScope = (value) =>
  value &&
  ["artifact", "device", "lease"].every(
    (key) => typeof value[key] === "string" && value[key].length > 0,
  );
const matches = (node, selector) =>
  node.role === selector.role &&
  (selector.identifier ? node.identifier === selector.identifier : node.label === selector.label);

/** callTool is the QA host's existing tool/recorder bridge. No shell or connection.
 * ready/readScope/identifyScreen are existing QA gate and native identity assertions.
 * redactLabel is mandatory: only the selected relevant controls leave the host.
 */
export async function routeArgentAction({
  objective,
  expectedScreen,
  candidates,
  argent,
  decide,
  verify,
  decisionTimeoutMs = 90_000,
}) {
  const details = {};
  let sequence = 0;
  const observed = new Map();
  let configured;
  let recording;
  try {
    configured = structuredClone(candidates);
    recording = structuredClone(argent?.recording);
  } catch {
    return { status: "unknown", reason: "invalid_argent_policy", actionAttempted: false };
  }
  if (
    !Array.isArray(configured) ||
    configured.length > 254 ||
    configured.some(
      (action) =>
        action.operation !== "tap" ||
        !action.selector ||
        typeof action.selector.role !== "string" ||
        (!action.selector.identifier && !action.selector.label),
    ) ||
    !recording ||
    !isAbsolute(recording.project_root) ||
    typeof recording.name !== "string" ||
    !recording.name ||
    typeof argent.redactLabel !== "function" ||
    !Number.isSafeInteger(decisionTimeoutMs) ||
    decisionTimeoutMs < 1 ||
    decisionTimeoutMs > 90_000
  )
    return { status: "unknown", reason: "invalid_argent_policy", actionAttempted: false };

  async function scope() {
    const value = structuredClone(await argent.readScope());
    if (!validScope(value)) throw new Error("invalid_operator_scope");
    return value;
  }
  async function observe() {
    const before = await scope();
    const idle = argentBody(await argent.callTool("await-screen-idle", { udid: before.device }));
    const raw = await argent.callTool("describe", { udid: before.device });
    const controls = argentControls(raw);
    const after = await scope();
    if (!same(before, after)) throw new Error("retired_operator_scope");
    const identity = await argent.identifyScreen(argentBody(raw));
    const nodes = [];
    const actions = [];
    const targets = new Map();
    for (const action of configured) {
      const found = controls.filter((node) => matches(node, action.selector));
      if (found.length > 1) throw new Error("ambiguous_argent_target");
      const node = found[0];
      if (!node || node.sensitive) continue;
      const id = `observed-${hash(node)}`;
      // Two action choices for one control may share the same observed node.
      if (!targets.has(id))
        nodes.push({
          id,
          role: node.role,
          label: argent.redactLabel(node.label),
          visible: node.visible,
          enabled: node.enabled,
        });
      targets.set(id, node);
      actions.push({ id: action.id, operation: action.operation, targetId: id });
    }
    const value = {
      ...after,
      id: randomUUID(),
      sequence: ++sequence,
      screen: identity.screen,
      summary: identity.summary,
      idle: idle.settled === true,
      nodes,
    };
    observed.clear(); // Never reuse a prior raw tree or retain unrelated UI content.
    observed.set(value.id, { value, actions, targets });
    return value;
  }
  const driver = {
    ready: (action) => argent.ready(action),
    observe: async () => {
      try {
        return await observe();
      } catch (error) {
        const known = [
          "unsupported_actionability_source",
          "degraded_argent_observation",
          "ambiguous_argent_target",
          "retired_operator_scope",
        ];
        if (known.includes(error.message)) details.error = error.message;
        throw error;
      }
    },
    async execute(action, executionScope) {
      const retained = observed.get(executionScope.observationId);
      const node = retained?.targets.get(action.targetId);
      if (
        !node ||
        !retained.actions.some(
          (item) =>
            item.id === action.id &&
            item.operation === action.operation &&
            item.targetId === action.targetId,
        )
      )
        throw new Error("unobserved_dispatch_target");
      if ((await argent.ready(structuredClone(action))) !== true)
        throw new Error("retired_operator_scope");
      const fresh = await observe();
      const freshTarget = observed.get(fresh.id)?.targets.get(action.targetId);
      if (
        !same(retained.value, executionScope) ||
        !same(fresh, executionScope) ||
        fresh.screen !== retained.value.screen ||
        !fresh.idle ||
        !freshTarget ||
        hash(freshTarget) !== hash(node) ||
        !freshTarget.visible ||
        !freshTarget.enabled
      )
        throw new Error("stale_dispatch_target");
      if (
        (await argent.ready(structuredClone(action))) !== true ||
        !same(await scope(), executionScope)
      )
        throw new Error("retired_operator_scope");
      // Only the observed current frame reaches the existing recorder. The model
      // sees neither coordinates nor selectors and cannot provide tool arguments.
      const result = argentBody(
        await argent.callTool("flow-add-step", {
          ...recording,
          command: "gesture-tap",
          args: JSON.stringify({
            udid: fresh.device,
            x: node.frame.x + node.frame.width / 2,
            y: node.frame.y + node.frame.height / 2,
          }),
        }),
      );
      details.recorded = result.recorded;
      const line = /^\d+\. tap: (\{.*\})$/.exec(result.recorded ?? "");
      let selector;
      try {
        selector = line && JSON.parse(line[1]);
      } catch {
        /* Unknown receipt stays failed. */
      }
      const exact =
        selector &&
        Object.keys(selector).length === 1 &&
        (node.identifier ? selector.id === node.identifier : selector.text === node.label) &&
        !Object.hasOwn(selector, "x") &&
        !Object.hasOwn(selector, "y");
      if (
        !exact ||
        typeof result.savedTo !== "string" ||
        result.toolResult?.isError === true ||
        result.message?.includes(" — ")
      )
        throw new Error("unverified_recorder_result");
    },
  };
  const result = await routeObservedAction({
    objective,
    expectedScreen,
    driver,
    verify,
    candidates: (value) => observed.get(value.id)?.actions ?? [],
    decide: async (request) => {
      let timer;
      try {
        return await Promise.race([
          Promise.resolve().then(() => decide(request)),
          new Promise((_, reject) => {
            timer = setTimeout(() => {
              details.error = "provider_timeout";
              reject(new Error("provider_timeout"));
            }, decisionTimeoutMs);
          }),
        ]);
      } finally {
        clearTimeout(timer);
      }
    },
  });
  if (details.error && result.reason === "invalid_observation_or_candidates")
    result.reason = details.error;
  return { ...result, adapter: details };
}
