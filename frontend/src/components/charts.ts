import Chart, { type ChartConfiguration } from 'chart.js/auto';
import type { visualSnapshot, satelliteInformation } from '../dataPipeline/apiClient';
import type { createState , stateManagement} from '../dataPipeline/stateManagement'; // Adjust path if needed
import type { Point } from 'chart.js';
import type { AnalyticsSnapshot } from '../dataPipeline/apiClient';
import type { AnalyticsStore } from '../dataPipeline/stateManagement';

// --- SLEEK AEROSPACE THEME ---
const THEME = {
  bg: 'transparent',
  grid: '#1e293b',     
  text: '#64748b',     
  accent: '#38bdf8',
  safe: '#10b981',     // Safe (> 5km)
  warning: '#eab308',  // Warning (< 5km)
  critical: '#ef4444', // critical (< 1km)
  cooldown: '#334155'  // cooldown thrusters
};

// Global Chart.js resets for minimalist look
Chart.defaults.color = THEME.text;
Chart.defaults.font.family = "'IBM Plex Mono', monospace";
Chart.defaults.elements.point.borderWidth = 0;
Chart.defaults.elements.bar.borderWidth = 0;
Chart.defaults.plugins.tooltip.backgroundColor = 'rgba(10, 15, 30, 0.9)';
Chart.defaults.plugins.tooltip.borderColor = THEME.grid;
Chart.defaults.plugins.tooltip.borderWidth = 1;

let bullseyeChart: Chart;
let fuelGaugeChart: Chart;
let efficiencyChart: Chart;
let ganttChart: Chart;

// Keep track of the last processed timestamp to prevent useless re-renders
let lastTimestamp: string | null = null;

// The Conjunction "Bullseye" Plot 
export function initBullseyeChart(canvasId: string) {
  const ctx = document.getElementById(canvasId) as HTMLCanvasElement;
  if (!ctx) return;

  const config: ChartConfiguration = {
    type: 'scatter',
    data: {
      datasets: [{
        label: 'Debris Proxies',
        data: [], 
        backgroundColor: THEME.safe,
        pointRadius: 4, 
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: { legend: { display: false } },
      scales: {
        x: {
          min: -15, max: 15,
          grid: { color: THEME.grid, lineWidth: (c) => c.tick.value === 0 ? 2 : 1 },
          border: { display: false },
          ticks: { stepSize: 5 }
        },
        y: {
          min: -15, max: 15,
          grid: { color: THEME.grid, lineWidth: (c) => c.tick.value === 0 ? 2 : 1 },
          border: { display: false },
          ticks: { stepSize: 5 }
        }
      }
    }
  };
  bullseyeChart = new Chart(ctx, config);
}

// 2. Fuel Gauge (Target: #fuel-gauge-canvas)
export function initFuelGauge(canvasId: string) {
  const ctx = document.getElementById(canvasId) as HTMLCanvasElement;
  if (!ctx) return;

  const config: ChartConfiguration<'doughnut'> = {
    type: 'doughnut',
    data: {
      labels: ['Fuel Remaining', 'Depleted'],
      datasets: [{
        data: [50, 0], // Starts at 50kg max
        backgroundColor: [THEME.accent, THEME.grid],
        borderWidth: 0,
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      circumference: 180, 
      cutout: '80%', 
      rotation: -90,      
      plugins: { legend: { display: false } }
    }
  };
  fuelGaugeChart = new Chart(ctx, config);
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Efficiency Chart (Target: #efficiency-canvas)
// ─────────────────────────────────────────────────────────────────────────────
export function initEfficiencyChart(canvasId: string) {
  const ctx = document.getElementById(canvasId) as HTMLCanvasElement;
  if (!ctx) return;

  const config: ChartConfiguration = {
    type: 'line',
    data: {
      labels: ['T-5', 'T-4', 'T-3', 'T-2', 'T-1', 'Now'],
      datasets: [
        {
          label: 'Δv Consumed',
          data: [0, 0, 0, 0, 0, 0],
          borderColor: THEME.accent,
          borderWidth: 2,
          tension: 0, 
          yAxisID: 'y'
        },
        {
          type: 'bar',
          label: 'Collisions Avoided',
          data: [0, 0, 0, 0, 0, 0],
          backgroundColor: THEME.safe,
          barThickness: 6, 
          yAxisID: 'y1'
        }
      ]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      interaction: { mode: 'index', intersect: false },
      plugins: { legend: { display: false } }, // Hide legend to save space
      scales: {
        x: { grid: { display: false }, border: { display: false } },
        y: { type: 'linear', position: 'left', grid: { color: THEME.grid }, border: { display: false } },
        y1: { type: 'linear', position: 'right', grid: { display: false }, border: { display: false }, suggestedMax: 5 }
      }
    }
  };
  efficiencyChart = new Chart(ctx, config);
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Maneuver Timeline Gantt (Target: #timeline-canvas)
// ─────────────────────────────────────────────────────────────────────────────
export function initGanttChart(canvasId: string) {
  const ctx = document.getElementById(canvasId) as HTMLCanvasElement;
  if (!ctx) return;

  const config: ChartConfiguration = {
    type: 'bar',
    data: {
      labels: ['SAT-A01', 'SAT-A04', 'SAT-B12'],
      datasets: [
        {
          label: 'Burn Window',
          data: [[0, 60], [120, 180], [300, 360]], // [Start, End]
          backgroundColor: THEME.accent,
          barPercentage: 0.4, 
        },
        {
          label: 'Thruster Cooldown (600s)',
          data: [[60, 660], [180, 780], [360, 960]], // End of burn + 600s
          backgroundColor: THEME.cooldown,
          barPercentage: 0.2, 
        }
      ]
    },
    options: {
      indexAxis: 'y', 
      responsive: true,
      maintainAspectRatio: false,
      plugins: { legend: { position: 'top', labels: { boxWidth: 10, usePointStyle: true } } },
      scales: {
        x: { min: -100, max: 1200, grid: { color: THEME.grid }, border: { display: false } },
        y: { stacked: true, grid: { display: false }, border: { display: false } }
      }
    }
  };
  ganttChart = new Chart(ctx, config);
}

// ─────────────────────────────────────────────────────────────────────────────
// SUBSCRIPTION & UPDATE LOGIC
// ─────────────────────────────────────────────────────────────────────────────

export function setupChartSubscriptions(store: stateManagement) {
  store.subscribe((snapshot: visualSnapshot) => {
    // PREVENT USELESS RE-RENDERS: Only update if the data is genuinely new
    if (!snapshot || snapshot.timestamp === lastTimestamp) return;
    lastTimestamp = snapshot.timestamp;

    updateDashboardCharts(snapshot);
  });
}

function updateDashboardCharts(snapshot: visualSnapshot) {
  // 1. UPDATE FUEL GAUGE
  if (snapshot.satellites && snapshot.satellites.length > 0) {
    let totalFuel = 0;
    snapshot.satellites.forEach(sat => totalFuel += sat.fuel_kg);
    const avgFuel = totalFuel / snapshot.satellites.length;
    
    fuelGaugeChart.data.datasets[0].data[0] = avgFuel;
    fuelGaugeChart.data.datasets[0].data[1] = 50.0 - avgFuel; // 50kg limit
    
    // Dynamic color coding based on severity
    const gaugeColor = avgFuel < 10 ? THEME.critical : (avgFuel < 25 ? THEME.warning : THEME.accent);
    fuelGaugeChart.data.datasets[0].backgroundColor = [gaugeColor, THEME.grid];
    fuelGaugeChart.update();
  }

  // 2. UPDATE BULLSEYE PLOT (Proximity calculation)
  if (snapshot.debris_cloud && snapshot.debris_cloud.length > 0) {
    const bullseyeData: Point[] = [];
    const bullseyeColors: string[] = [];
    
    // We grab the first 30 debris objects to display on the radar relative to origin
    const radarDebris = snapshot.debris_cloud.slice(0, 30);

    radarDebris.forEach((debris) => {
      // MOCK RELATIVE DISTANCE: In reality, backend sends local RTN coordinates.
      // Here we map latitude/longitude diffs to an arbitrary X/Y for the radar.
      const x = (debris[2] % 30) - 15; // constrain to -15 to +15 grid
      const y = (debris[1] % 30) - 15; 
      const distance = Math.sqrt(x*x + y*y);
      
      bullseyeData.push({ x, y });
      
      // RISK THRESHOLDS (Green, Yellow < 5km, Red < 1km)
      if (distance < 1.0) bullseyeColors.push(THEME.critical);
      else if (distance < 5.0) bullseyeColors.push(THEME.warning);
      else bullseyeColors.push(THEME.safe);
    });

    bullseyeChart.data.datasets[0].data = bullseyeData;
    bullseyeChart.data.datasets[0].backgroundColor = bullseyeColors as any;
    bullseyeChart.update();
  }
}

let lastAnalyticsTimestamp: string | null = null;

export function setupAnalyticsSubscriptions(analyticsStore: AnalyticsStore) {
  analyticsStore.subscribe((snapshot: AnalyticsSnapshot) => {
    if (!snapshot || snapshot.timestamp === lastAnalyticsTimestamp) return;
    lastAnalyticsTimestamp = snapshot.timestamp;

    const now = Date.now();

    // 1. PROCESS EFFICIENCY HISTORY
    if (snapshot.efficiencyHistory && efficiencyChart.data.datasets) {
      const labels: string[] = [];
      const fuelData: number[] = [];
      const collisionData: number[] = [];

      snapshot.efficiencyHistory.forEach((point) => {
        // Convert ISO string to a clean time label (e.g., "08:05:00")
        const timeLabel = new Date(point.timestamp).toLocaleTimeString([], { hour12: false });
        labels.push(timeLabel);
        fuelData.push(point.avgFuel_kg);
        collisionData.push(point.collisionsAvoided);
      });

      efficiencyChart.data.labels = labels;
      efficiencyChart.data.datasets[0].data = fuelData;
      efficiencyChart.data.datasets[1].data = collisionData;
      efficiencyChart.update();
    }

    // 2. PROCESS GANTT MANEUVERS (ISO to Relative Seconds)
    if (snapshot.maneuvers && ganttChart.data.datasets) {
      const labels: string[] = [];
      const burnWindows: [number, number][] = [];
      const cooldowns: [number, number][] = [];

      snapshot.maneuvers.forEach((maneuver) => {
        labels.push(maneuver.satelliteId);

        // Convert ISO strings to absolute milliseconds
        const startMs = new Date(maneuver.startTime).getTime();
        const endMs = new Date(maneuver.endTime).getTime();

        // Calculate relative seconds compared to exactly NOW
        const relativeStart = (startMs - now) / 1000;
        const relativeEnd = (endMs - now) / 1000;

        burnWindows.push([relativeStart, relativeEnd]);
        // The mandatory 600s cooldown starts exactly when the burn ends
        cooldowns.push([relativeEnd, relativeEnd + 600]); 
      });

      ganttChart.data.labels = labels;
      ganttChart.data.datasets[0].data = burnWindows;
      ganttChart.data.datasets[1].data = cooldowns;
      ganttChart.update();
    }
  });
}