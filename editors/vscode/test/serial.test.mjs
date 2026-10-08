import assert from "node:assert/strict";
import { test } from "node:test";
import { serialized } from "../out/serial.js";

const tick = () => new Promise((resolve) => setImmediate(resolve));

test("overlapping operations never interleave", async () => {
  const exclusive = serialized();
  const log = [];
  const op = (name) => async () => {
    log.push(`${name}:start`);
    await tick();
    await tick();
    log.push(`${name}:end`);
  };
  await Promise.all([exclusive(op("a")), exclusive(op("b")), exclusive(op("c"))]);
  assert.deepEqual(log, [
    "a:start", "a:end", "b:start", "b:end", "c:start", "c:end",
  ]);
});

test("a failed operation rejects its caller and lets later ones run", async () => {
  const exclusive = serialized();
  const failing = exclusive(async () => {
    throw new Error("boom");
  });
  const next = exclusive(async () => "ok");
  await assert.rejects(failing, /boom/);
  assert.equal(await next, "ok");
});
