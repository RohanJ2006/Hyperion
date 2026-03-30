import { fetchSnapshot , type visualSnapshot } from "./apiClient";
import { fetchAnalytics, type AnalyticsSnapshot } from './apiClient';

export type stateListener = (data:visualSnapshot) => void; //snapshot 
export type AnalyticsListener = (data: AnalyticsSnapshot) => void; //analytics

export interface stateManagement {
  subscribe : (listener: stateListener) => () => void;
  start: () => void;
  stop: () => void;
  getLatest: () => visualSnapshot | null;
}

export interface AnalyticsStore {
  subscribe: (listener: AnalyticsListener) => () => void;
  start: () => void;
  stop: () => void;
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


export function createAnalyticsState(): AnalyticsStore {
  const listeners: Set<AnalyticsListener> = new Set();
  let timerId: ReturnType<typeof setTimeout> | null = null;
  let isRunning = false;
  let latest: AnalyticsSnapshot | null = null;

  function notify(data: AnalyticsSnapshot): void {
    latest = Object.freeze(data);
    listeners.forEach((fn) => fn(latest!));
  }

  async function poll(): Promise<void> {
    if (!isRunning) return;

    try {
      const data = await fetchAnalytics();
      notify(data);
    } catch (error) {
      console.error('[AnalyticsStore] Failed to fetch', error);
    } finally {
      if (isRunning) {
        // EXACTLY 3 SECONDS POLLING RATE
        timerId = setTimeout(poll, 3000); 
      }
    }
  }

  function subscribe(listener: AnalyticsListener): () => void {
    listeners.add(listener);
    if (latest) listener(latest);
    return () => listeners.delete(listener);
  }

  function start(): void {
    if (isRunning) return;
    isRunning = true;
    poll();
  }

  function stop(): void {
    isRunning = false;
    if (timerId) {
      clearTimeout(timerId);
      timerId = null;
    }
  }

  return { subscribe, start, stop };
}