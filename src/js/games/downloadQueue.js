/** Bounded per-game queue. New versions replace waiting work, never running work. */
export function createDownloadQueue({ run, onQueued = () => {}, onChanged = () => {}, onError = () => {}, concurrency = 4 }) {
	if (!Number.isInteger(concurrency) || concurrency < 1) throw new RangeError("concurrency must be a positive integer");
	const waiting = new Map();
	const running = new Map();
	let failed = 0;
	const snapshot = () => ({ active: running.size, waiting: waiting.size, failed });
	const changed = () => onChanged(snapshot());

	function pump() {
		while (running.size < concurrency && waiting.size) {
			const [key, item] = waiting.entries().next().value;
			// A newer version of this game must wait for its current download.
			const candidate = running.has(key)
				? [...waiting.entries()].find(([id]) => !running.has(id)) : [key, item];
			if (!candidate) break;
			const [id, work] = candidate;
			waiting.delete(id);
			running.set(id, work);
			void Promise.resolve().then(() => run(work.item, () => work.cancelled)).catch(error => {
				if (!work.cancelled) { failed++; onError(error, work.item); }
			}).finally(() => {
				running.delete(id);
				pump();
				changed();
			});
		}
	}

	return {
		enqueue(item) {
			const key = item.cleanId;
			const active = running.get(key);
			const pending = waiting.get(key);
			if (pending?.item.version === item.version || (active && !active.cancelled && active.item.version === item.version)) return false;
			if (!running.size && !waiting.size) failed = 0;
			waiting.set(key, { item, cancelled: false });
			// Do not replace a running download's progress with "queued".
			if (!active) onQueued(item);
			pump(); changed();
			return true;
		},
		cancel(key) {
			waiting.delete(key);
			const active = running.get(key);
			if (active) active.cancelled = true;
			changed();
		},
		snapshot,
	};
}
