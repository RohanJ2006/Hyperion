import { fetchSnapshot , type visualSnapshot } from "./apiClient";

export type stateListener = (data:visualSnapshot) => void;

export interface stateManagement {
  subscribe : (listener: stateListener) => () => void;
  start: () => void;
  stop: () => void;
  getLatest: () => visualSnapshot | null;
}

export function createState(): stateManagement {
  const listeners: Set<stateListener> = new Set();
  let intervalId: ReturnType<typeof setInterval> | null = null;
  let latest: visualSnapshot | null = null;

  function notify(data : visualSnapshot): void {
    latest = data;
    listeners.forEach((fn) => fn(data))
  }

  async function poll() : Promise<void>{
    try {
      const snapshot = await fetchSnapshot();
      notify(snapshot);
    } catch {}
  }

  function subscribe(listener : stateListener): () => void {
    listeners.add(listener);
    if(latest) listener(latest);
    return () => listeners.delete(listener);
  }

  function start(): void {
    if (intervalId) return;
    poll();
    intervalId = setInterval(poll, 2000);
  }

  function stop(): void {
    if (intervalId) {
      clearInterval(intervalId);
      intervalId = null;
    }
  }

  function getLatest(): visualSnapshot | null {
    return latest;
  }

  return { subscribe, start, stop, getLatest };

}
