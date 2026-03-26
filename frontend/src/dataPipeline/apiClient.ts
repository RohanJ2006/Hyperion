export interface satelliteInformation{
  id : string,
  lat : number,
  lon : number,
  fuel_kg : number,
  status : 'NOMINAL' | 'CRITICAL' | 'WARNING';
}

export type debrisTuple = [string, number, number, number];

export type IsoDateString = string;  //ISO format

export interface visualSnapshot{
  timestamp : IsoDateString,
  satellites : satelliteInformation[],
  debris_cloud : debrisTuple[]
}

export interface EfficiencyDatapoint {
  timestamp: IsoDateString; 
  avgFuel_kg: number;
  collisionsAvoided: number;
}

export interface ManeuverEvent {
  satelliteId: string;
  type: 'BURN' | 'COAST';
  startTime: IsoDateString; 
  endTime: IsoDateString;   
}

export interface AnalyticsSnapshot {
  timestamp: IsoDateString; 
  efficiencyHistory: EfficiencyDatapoint[];
  maneuvers: ManeuverEvent[];
}

const BASE_API = 'api/visualization';

//function to get the snapshot of the data to be projected 
export async function fetchSnapshot():Promise<visualSnapshot>{
  try {
    const res = await fetch(`${BASE_API}/snapshot`);
    if(!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
  } catch (error) {
    console.log("backend has failed so we are falling back to fallBack code for visualization!", error)
    return fallbackSnapshot();
  }
}


//this is a fallback function in case something fails 
function fallbackSnapshot():visualSnapshot{

  const timestamp = new Date().toISOString();

  const satellites:satelliteInformation[] = [];
  for(let i = 0 ; i <=50 ; i++) {
    satellites.push({
      id: `SAT-Alpha-${String(i).padStart(2, '0')}`,
      lat: (Math.random() - 0.5) * 140,
      lon: (Math.random() - 0.5) * 360,
      fuel_kg: 10 + Math.random() * 40,
      status: Math.random() > 0.9 ? 'WARNING' : Math.random() > 0.98 ? 'CRITICAL' : 'NOMINAL',
    })
  }

  const debris_cloud:debrisTuple[] = [];
  for (let i = 1; i <= 200; i++) {
    debris_cloud.push([
      `DEB-${10000 + i}`,
      (Math.random() - 0.5) * 140, 
      (Math.random() - 0.5) * 360, 
      300 + Math.random() * 500 
    ]);
  }

  return {timestamp , satellites , debris_cloud };
}

export async function fetchAnalytics(): Promise<AnalyticsSnapshot> {
  try {
    const res = await fetch('/api/visualization/analytics');
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return await res.json();
  } catch (error) {
    console.warn("Analytics API offline, using fallback data.");
    return generateFallbackAnalytics();
  }
}

function generateFallbackAnalytics(): AnalyticsSnapshot {
  const now = Date.now();
  const history: EfficiencyDatapoint[] = [];
  
  // Generate 6 historical data points
  for (let i = 5; i >= 0; i--) {
    history.push({
      timestamp: new Date(now - i * 60000).toISOString(),
      avgFuel_kg: 48 - (5 - i) * 0.2, // Slowly draining
      collisionsAvoided: Math.random() > 0.8 ? 1 : 0
    });
  }

  return {
    timestamp: new Date(now).toISOString(),
    efficiencyHistory: history,
    maneuvers: [
      {
        satelliteId: 'SAT-Alpha-01',
        type: 'BURN',
        startTime: new Date(now + 60000).toISOString(), // Starts in 60s
        endTime: new Date(now + 120000).toISOString()   // Lasts 60s
      },
      {
        satelliteId: 'SAT-Alpha-04',
        type: 'BURN',
        startTime: new Date(now + 180000).toISOString(),
        endTime: new Date(now + 260000).toISOString()
      }
    ]
  };
}