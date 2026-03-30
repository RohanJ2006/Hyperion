export interface satelliteInformation {
  id : string;
  lat : number;
  lon : number;
  fuel_kg : number;
  status : 'NOMINAL' | 'CRITICAL' | 'WARNING';
}

export type debrisTuple = [string, number, number, number];

export type IsoDateString = string;  //ISO format

export interface visualSnapshot {
  timestamp: IsoDateString;
  satellites: satelliteInformation[];
  debris_cloud: debrisTuple[];
}

export interface ManeuverEvent {
  satelliteId: string;
  type: 'BURN' | 'COAST';
  startTime: IsoDateString;
  endTime: IsoDateString;
}

export interface AnalyticsSnapshot {
  timestamp: IsoDateString;
  debrisAvoided: number;        // NEW: cumulative debris avoidances up to this point
  maneuvers: ManeuverEvent[];
}
 
// ─── GLOBAL FUEL ACCOUNTING ──────────────────────────────────────────────────
// Tracks the last known fuel_kg per satellite ID so we can detect burns.
// Lives at module scope so it persists across every fetchSnapshot() call.
const lastKnownFuel = new Map<string, number>();
 
// Running total of fuel consumed across the entire fleet, accumulated over time.
// Exported so charts.ts can read it directly.
export let cumulativeFuelConsumed = 0;
 
// Called by fetchSnapshot after each poll. Diffs current vs previous fuel levels.
// Only accumulates when fuel has strictly decreased (a burn happened).
// Returns the kg burned in this particular snapshot cycle (0 if no burns).
export function accountFuelDelta(satellites: satelliteInformation[]): number {
  let deltaThisCycle = 0;
 
  satellites.forEach((sat) => {
    const prev = lastKnownFuel.get(sat.id);
 
    if (prev !== undefined && sat.fuel_kg < prev) {
      // Fuel decreased → a burn happened between the last snapshot and this one
      const burned = prev - sat.fuel_kg;
      deltaThisCycle += burned;
      cumulativeFuelConsumed += burned;
    }
 
    // Always update regardless of direction (refuel not possible, but handles
    // first-time registration and floating-point noise cleanly)
    lastKnownFuel.set(sat.id, sat.fuel_kg);
  });
 
  return deltaThisCycle;
}
 
// ─── EFFICIENCY CURVE DATA ────────────────────────────────────────────────────
// Each point is one (cumulativeFuel, cumulativeCollisions) coordinate.
// charts.ts reads this array directly to render the efficiency line.
export interface EfficiencyPoint {
  fuel: number;        // X axis: cumulative kg burned across fleet
  collisions: number;  // Y axis: cumulative debris avoidances
}
 
export const efficiencyHistory: EfficiencyPoint[] = [];
 
// const BASE_API = 'api/visualization';

const isDev = import.meta.env.DEV;
const BASE_URL = isDev ? 'http://localhost:8000' : '';
const BASE_API = `${BASE_URL}/api/visualization`;
 
// ─── SNAPSHOT FETCH ───────────────────────────────────────────────────────────
export async function fetchSnapshot(): Promise<visualSnapshot> {
  try {
    const res = await fetch(`${BASE_API}/snapshot`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data: visualSnapshot = await res.json();
    accountFuelDelta(data.satellites);
    return data;
  } catch (error) {
    console.log('Backend offline, using fallback snapshot.', error);
    return fallbackSnapshot();
  }
}
 
// ─── ANALYTICS FETCH ──────────────────────────────────────────────────────────
// export async function fetchAnalytics(): Promise<AnalyticsSnapshot> {
//   try {
//     const res = await fetch('/api/visualization/analytics');
//     if (!res.ok) throw new Error(`HTTP ${res.status}`);
//     return await res.json();
//   } catch (error) {
//     console.warn('Analytics API offline, using fallback.');
//     return generateFallbackAnalytics();
//   }
// }

export async function fetchAnalytics(): Promise<AnalyticsSnapshot> {
  // We CANNOT fetch from the backend because the PDF does not allow an analytics API.
  // Instead, we use the local generator to feed the UI charts.
  return generateFallbackAnalytics();
}

// ─── FALLBACK SNAPSHOT ────────────────────────────────────────────────────────
// Simulates realistic fuel burns: each satellite has a stable fuel level that
// occasionally drops by a small amount (simulating a burn event).
// We persist fuel state in _fakeFuelState so values carry over between calls.
const _fakeFuelState = new Map<string, number>();

function fallbackSnapshot(): visualSnapshot {
  const timestamp = new Date().toISOString();
  const satellites: satelliteInformation[] = [];
 
  for (let i = 0; i <= 50; i++) {
    const id = `SAT-Alpha-${String(i).padStart(2, '0')}`;
 
    // Initialise fuel state on first call (50kg max per problem statement)
    if (!_fakeFuelState.has(id)) {
      _fakeFuelState.set(id, 40 + Math.random() * 20); // start 30–50 kg
    }
 
    let fuel = _fakeFuelState.get(id)!;
 
    // ~15% chance of a small burn (0.1–0.8 kg) each poll cycle
    if (Math.random() < 0.15 && fuel > 2) {
      fuel = Math.max(0, fuel - (0.1 + Math.random() * 0.7));
      _fakeFuelState.set(id, fuel);
    }
    satellites.push({
      id,
      lat: (Math.random() - 0.5) * 140,
      lon: (Math.random() - 0.5) * 360,
      fuel_kg: parseFloat(fuel.toFixed(2)),
      status: fuel < 5 ? 'CRITICAL' : fuel < 15 ? 'WARNING' : 'NOMINAL',
    });
  }
 
  // After building the snapshot, run fuel accounting so cumulativeFuelConsumed updates
  accountFuelDelta(satellites);
 
  const debris_cloud: debrisTuple[] = [];
  for (let i = 1; i <= 200; i++) {
    debris_cloud.push([
      `DEB-${10000 + i}`,
      (Math.random() - 0.5) * 140,
      (Math.random() - 0.5) * 360,
      300 + Math.random() * 500,
    ]);
  }
 
  return { timestamp, satellites, debris_cloud };
}
 
// ─── FALLBACK ANALYTICS ───────────────────────────────────────────────────────
// debrisAvoided increments by 0 or 1 each call (~30% chance),
let _fakeDebrisAvoided = 0;
 
// ─── FALLBACK MANEUVER STATE ─────────────────────────────────────────────────
// We keep a stable schedule that shifts relative to "now" each call, so the
// Gantt always shows upcoming and in-progress burns rather than static offsets.
// The schedule is seeded once and then referenced by relative offset in seconds.
interface FakeManeuver {
  satelliteId: string;
  type: 'BURN' | 'COAST';
  startOffsetS: number;   // seconds relative to call time
  durationS: number;
}
 
// Fixed pool of satellites with scheduled burns at staggered times.
const FAKE_SCHEDULE: FakeManeuver[] = [
  { satelliteId: 'SAT-Alpha-01', type: 'BURN',  startOffsetS:   60, durationS: 75  },
  { satelliteId: 'SAT-Alpha-01', type: 'COAST', startOffsetS:  135, durationS: 600 },
  { satelliteId: 'SAT-Alpha-04', type: 'BURN',  startOffsetS:  200, durationS: 90  },
  { satelliteId: 'SAT-Alpha-04', type: 'COAST', startOffsetS:  290, durationS: 600 },
  { satelliteId: 'SAT-Alpha-07', type: 'BURN',  startOffsetS:  350, durationS: 60  },
  { satelliteId: 'SAT-Alpha-12', type: 'BURN',  startOffsetS:  480, durationS: 120 },
  { satelliteId: 'SAT-Alpha-12', type: 'BURN',  startOffsetS:  720, durationS: 45  }, // recovery burn
  { satelliteId: 'SAT-Alpha-19', type: 'BURN',  startOffsetS:  900, durationS: 80  },
  { satelliteId: 'SAT-Alpha-23', type: 'BURN',  startOffsetS: 1100, durationS: 55  },
  { satelliteId: 'SAT-Alpha-31', type: 'BURN',  startOffsetS:  -90, durationS: 60  }, // already burning
];
 
function generateFallbackAnalytics(): AnalyticsSnapshot {
  const now = Date.now();
 
  // ~30% chance of a new avoidance event each 3s cycle
  if (Math.random() < 0.3) _fakeDebrisAvoided++;
 
  const maneuvers: ManeuverEvent[] = FAKE_SCHEDULE.map(m => ({
    satelliteId: m.satelliteId,
    type: m.type,
    startTime: new Date(now + m.startOffsetS * 1000).toISOString(),
    endTime:   new Date(now + (m.startOffsetS + m.durationS) * 1000).toISOString(),
  }));
 
  return {
    timestamp:    new Date(now).toISOString(),
    debrisAvoided: _fakeDebrisAvoided,
    maneuvers,
  };
}
 