import { test } from "node:test";
import assert from "node:assert/strict";
import { createDownloadQueue } from "../src/js/games/downloadQueue.js";

const tick = () => new Promise(resolve => setImmediate(resolve));
const item = (cleanId, version = "1") => ({ cleanId, version });
function harness(concurrency = 2) {
  const runs = [], states = [], errors = [];
  const queue = createDownloadQueue({ concurrency,
    run: (item, cancelled) => new Promise((resolve, reject) => runs.push({ item, cancelled, resolve, reject })),
    onChanged: state => states.push(state), onError: error => errors.push(error),
  });
  return { queue, runs, states, errors };
}

test("bounded queue coalesces waiting versions and ignores duplicates", async () => {
  const { queue, runs } = harness(1);
  queue.enqueue(item("a")); queue.enqueue(item("b")); queue.enqueue(item("b", "2"));
  assert.equal(queue.enqueue(item("a")), false);
  await tick(); assert.equal(runs.length, 1);
  runs[0].resolve(); await tick();
  assert.deepEqual(runs[1].item, item("b", "2"));
  runs[1].resolve(); await tick();
  assert.deepEqual(queue.snapshot(), { active: 0, waiting: 0, failed: 0 });
});

test("new version arriving during a download is not lost or run concurrently for the same game", async () => {
  const { queue, runs } = harness();
	queue.enqueue(item("a")); queue.enqueue(item("a", "2")); queue.enqueue(item("b"));
	assert.equal(queue.enqueue(item("a")), false); // Old duplicate must not replace the newer pending version.
  await tick(); assert.deepEqual(runs.map(run => run.item), [item("a"), item("b")]);
  runs[0].resolve(); await tick(); assert.deepEqual(runs[2].item, item("a", "2"));
  runs[1].resolve(); runs[2].resolve(); await tick();
});

test("deletion cancels waiting and invalidates running results without freeing their slot early", async () => {
  const { queue, runs } = harness(1);
  queue.enqueue(item("a")); queue.enqueue(item("b")); await tick();
  queue.cancel("a"); queue.cancel("b");
  assert.equal(runs[0].cancelled(), true);
  queue.enqueue(item("a", "2")); await tick(); assert.equal(runs.length, 1);
  runs[0].reject(new Error("deleted")); await tick();
  assert.deepEqual(runs[1].item, item("a", "2"));
  assert.equal(queue.snapshot().failed, 0);
  runs[1].resolve(); await tick();
});

test("failed work does not stall other games or become a success message", async () => {
  const { queue, runs, errors, states } = harness(1);
  queue.enqueue(item("a")); queue.enqueue(item("b")); await tick();
  runs[0].reject(new Error("network")); await tick();
  runs[1].resolve(); await tick();
  assert.equal(errors.length, 1);
  assert.deepEqual(states.at(-1), { active: 0, waiting: 0, failed: 1 });
  queue.enqueue(item("retry")); await tick(); assert.equal(queue.snapshot().failed, 0);
  runs[2].resolve(); await tick();
});
