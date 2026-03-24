
export interface satelliteInformation{
  id : string,
  lat : number,
  lon : number,
  fuel_kg : number,
  status : 'NOMINAL' | 'CRITICAL' | 'WARNING';
}

export type debrisTuple = [string, number, number, number];

export type IsoDateString = string;

export interface visualSnapshot{
  timestamp : IsoDateString,
  satellites : satelliteInformation[],
  debris_cloud : debrisTuple[]
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