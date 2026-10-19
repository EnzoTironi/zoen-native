import assert from "node:assert/strict";
import test from "node:test";
import {
  runtime,
  connect,
  named,
  call,
  register,
  eventually,
} from "./socket-lifecycle-harness.mjs";

test("actual terminal callback queue INSERT failure is recovered by the next real durable alarm", async () => {
  const mf = await runtime();
  try {
    const channel = await connect(mf, named("login-first"));
    const ready = await register(mf, channel);
    const previousAlarms = ready.audit.filter(
      (event) => event.phase === "actual-alarm-before",
    ).length;
    assert.ok(
      ready.nextAlarm > Date.now() + 60_000,
      "No pending immediate wake may accidentally hide a missing terminal finally rearm",
    );
    await call(mf, "/fault-offline");
    channel.socket.close(1000, "owned actual terminal fault scenario");
    await channel.closed();
    const recovered = await eventually(
      mf,
      (state) =>
        state.connections === 0 &&
        state.attached === 0 &&
        state.offline === 0 &&
        state.audit.some((event) => event.phase === "actual-close-sql-fault") &&
        state.audit.filter((event) => event.phase === "actual-alarm-before").length >
          previousAlarms,
      "An actual callback fault must recover through a genuine runtime alarm",
    );
    const fault = recovered.audit.find((event) => event.phase === "actual-close-sql-fault");
    assert.equal(fault.ended, 1, "Terminal correlation remains durable after INSERT rollback");
    assert.equal(fault.attached, 1, "Failed enqueue retains its recovery marker");
    assert.equal(fault.offline, 0);
    assert.equal(fault.sessions, 0, "Ended correlation cannot retain authentication authority");
    const position = recovered.audit.indexOf(fault);
    const alarm = recovered.audit
      .slice(position + 1)
      .find((event) => event.phase === "actual-alarm-before");
    assert.ok(alarm, "Recovery cannot be obtained by manually invoking another close callback");
    assert.equal(alarm.ended, 1);
    assert.equal(alarm.attached, 1);
    assert.equal(alarm.offline, 0);
    assert.equal(recovered.sessions, 0);
    assert.equal(recovered.challenges, 0);
    assert.equal(recovered.ready, 0);
    assert.equal(recovered.identities, 1);
    assert.equal(recovered.devices, 1);
  } finally {
    await mf.dispose();
  }
});
