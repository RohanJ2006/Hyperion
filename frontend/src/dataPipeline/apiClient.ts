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

// const BASE_URL = 'http://0.0.0.0:8000';
// const BASE_API = `${BASE_URL}/api/visualization`;
const BASE_API = '/api/visualization';
 
// ─── SNAPSHOT FETCH ───────────────────────────────────────────────────────────
export async function fetchSnapshot(): Promise<visualSnapshot> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 3000); // 3000ms limit

  try {
    const res = await fetch(`${BASE_API}/snapshot`, { signal: controller.signal });
    clearTimeout(timeoutId); // Clear the timeout if we get a fast response
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data: visualSnapshot = await res.json();
    accountFuelDelta(data.satellites);
    return data;
  } catch (error) {
    clearTimeout(timeoutId);
    console.warn('Backend offline or timed out (3s). Using fallback snapshot.');
    return fallbackSnapshot();
  }
}

// ─── ANALYTICS FETCH ──────────────────────────────────────────────────────────
export async function fetchAnalytics(): Promise<AnalyticsSnapshot> {
    return generateFallbackAnalytics();
} 

// ─── FALLBACK SNAPSHOT ────────────────────────────────────────────────────────
// Fuel and satellite positions are seeded once and stable across polls.
// A portion of debris is generated close to each satellite so the bullseye
// chart always has visible dots when the real backend is offline.
const _fakeFuelState = new Map<string, number>();
const _fakeSatPos    = new Map<string, { lat: number; lon: number }>();

// 1° of latitude ≈ 111 km. Debris within ±9.5 km maps inside the 10 km ring.
const KM_PER_DEG            = 111.0;
const NEAR_DEBRIS_SPREAD_KM = 9.5;
const NEAR_DEBRIS_SPREAD_DEG = NEAR_DEBRIS_SPREAD_KM / KM_PER_DEG;

function fallbackSnapshot(): visualSnapshot {
  const timestamp = new Date().toISOString();
  const satellites: satelliteInformation[] = [];

  for (let i = 0; i <= 50; i++) {
    const id = `SAT-Alpha-${String(i).padStart(2, '0')}`;

    // Stable position — seeded once, never changes between polls
    if (!_fakeSatPos.has(id)) {
      _fakeSatPos.set(id, {
        lat: (Math.random() - 0.5) * 140,
        lon: (Math.random() - 0.5) * 360,
      });
    }

    // Initialise fuel state on first call (50 kg max per problem statement)
    if (!_fakeFuelState.has(id)) {
      _fakeFuelState.set(id, 40 + Math.random() * 10);
    }

    let fuel = _fakeFuelState.get(id)!;

    // ~15% chance of a small burn (0.1–0.8 kg) each poll cycle
    if (Math.random() < 0.15 && fuel > 2) {
      fuel = Math.max(0, fuel - (0.1 + Math.random() * 0.7));
      _fakeFuelState.set(id, fuel);
    }

    const { lat, lon } = _fakeSatPos.get(id)!;
    satellites.push({
      id,
      lat,
      lon,
      fuel_kg: parseFloat(fuel.toFixed(2)),
      status: fuel < 5 ? 'CRITICAL' : fuel < 15 ? 'WARNING' : 'NOMINAL',
    });
  }

  // After building the snapshot, run fuel accounting so cumulativeFuelConsumed updates
  accountFuelDelta(satellites);

  const debris_cloud: debrisTuple[] = [];
  let debrisIdx = 1;

  // ── Near debris: 4–6 pieces per satellite within the 10 km bullseye zone ──
  satellites.forEach((sat) => {
    const nearCount = 4 + Math.floor(Math.random() * 3); // 4–6 per satellite
    for (let n = 0; n < nearCount; n++) {
      const dLat = (Math.random() * 2 - 1) * NEAR_DEBRIS_SPREAD_DEG;
      // Longitude degree size shrinks toward poles
      const cosLat = Math.cos(sat.lat * (Math.PI / 180)) || 0.01;
      const dLon   = (Math.random() * 2 - 1) * (NEAR_DEBRIS_SPREAD_DEG / cosLat);
      debris_cloud.push([
        `DEB-NEAR-${debrisIdx++}`,
        sat.lat + dLat,
        sat.lon + dLon,
        300 + Math.random() * 500,
      ]);
    }
  });

  // ── Background debris: global scatter (will not appear on the 10 km bullseye) ──
  for (let i = 0; i < 50; i++) {
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