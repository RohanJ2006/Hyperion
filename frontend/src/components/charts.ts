import Chart, { type ChartConfiguration, type Plugin } from 'chart.js/auto';
import type { visualSnapshot, satelliteInformation, AnalyticsSnapshot } from '../dataPipeline/apiClient';
import { cumulativeFuelConsumed } from '../dataPipeline/apiClient';
import type { stateManagement, AnalyticsStore } from '../dataPipeline/stateManagement';
import type { Point } from 'chart.js';

// --- SLEEK AEROSPACE THEME ---
const THEME = {
  bg: 'transparent',
  grid: '#1e293b',
  text: '#64748b',
  accent: '#38bdf8',
  safe: '#10b981',
  warning: '#eab308',
  critical: '#ef4444',
  cooldown: '#334155'
};

Chart.defaults.color = THEME.text;
Chart.defaults.font.family = "'IBM Plex Mono', monospace";
Chart.defaults.elements.point.borderWidth = 0;
Chart.defaults.elements.bar.borderWidth = 0;
Chart.defaults.plugins.tooltip.backgroundColor = 'rgba(10, 15, 30, 0.9)';
Chart.defaults.plugins.tooltip.borderColor = THEME.grid;
Chart.defaults.plugins.tooltip.borderWidth = 1;

let bullseyeChart: Chart;
let efficiencyChart: Chart;
let ganttChart: Chart;

let lastTimestamp: string | null = null;
let selectedSatelliteId: string | null = null;
 
// ─────────────────────────────────────────────────────────────────────────────
// BULLSEYE GEOMETRY HELPERS
// ─────────────────────────────────────────────────────────────────────────────

// Earth radius in km (WGS-84 mean)
const RE_KM = 6371.0;

// Maximum proximity radius shown on the bullseye (km). Debris beyond this is ignored.
const BULLSEYE_RANGE_KM = 10;

/**
 * Haversine great-circle distance between two geodetic points (surface-projected, km).
 * We use surface distance only — the snapshot lat/lon gives the sub-satellite point
 * and the chart is a 2-D proximity view, not a 3-D conjunction plot.
 */
function haversineKm(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const toRad = (d: number) => d * (Math.PI / 180);
  const dLat = toRad(lat2 - lat1);
  const dLon = toRad(lon2 - lon1);
  const a =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(toRad(lat1)) * Math.cos(toRad(lat2)) * Math.sin(dLon / 2) ** 2;
  return RE_KM * 2 * Math.asin(Math.sqrt(a));
}

/**
 * Initial bearing (°, 0 = North, clockwise) from point 1 → point 2.
 * Used to place debris at the correct angular position on the polar bullseye.
 */
function bearingDeg(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const toRad = (d: number) => d * (Math.PI / 180);
  const dLon  = toRad(lon2 - lon1);
  const phi1  = toRad(lat1);
  const phi2  = toRad(lat2);
  const y = Math.sin(dLon) * Math.cos(phi2);
  const x = Math.cos(phi1) * Math.sin(phi2) - Math.sin(phi1) * Math.cos(phi2) * Math.cos(dLon);
  return ((Math.atan2(y, x) * (180 / Math.PI)) + 360) % 360;
}

/**
 * Convert polar (distKm, bearingDeg) → Cartesian (x, y) for the scatter chart.
 * Bearing 0° = North = up on chart (+Y), 90° = East = right (+X).
 *   x =  distKm * sin(bearing)
 *   y =  distKm * cos(bearing)
 */
function polarToCartesian(distKm: number, bearDeg: number): { x: number; y: number } {
  const rad = bearDeg * (Math.PI / 180);
  return { x: distKm * Math.sin(rad), y: distKm * Math.cos(rad) };
}

// ─────────────────────────────────────────────────────────────────────────────
// BULLSEYE POLAR PLUGIN
// Renders: 3 dashed rings (1 km / 5 km / 10 km), 8 sector dividers at 45° steps,
// degree labels at each sector, ring distance labels, and the centre satellite dot.
// Coordinate space: x = East component (km), y = North component (km), origin = sat.
// ─────────────────────────────────────────────────────────────────────────────
const bullseyeRingsPlugin: Plugin = {
  id: 'bullseyeRings',
  afterDraw(chart) {
    const { ctx, chartArea, scales } = chart;
    if (!chartArea) return;

    const xScale = scales['x'];
    const yScale = scales['y'];
    const originX = xScale.getPixelForValue(0);
    const originY = yScale.getPixelForValue(0);
    // px per 1 km — derived from the x-axis mapping
    const pxPerKm = Math.abs(xScale.getPixelForValue(1) - xScale.getPixelForValue(0));

    const rings = [
      { radiusKm: 1,  color: THEME.critical, label: '1km'  },
      { radiusKm: 5,  color: THEME.warning,  label: '5km'  },
      { radiusKm: 10, color: '#2d4a6b',      label: '10km' },
    ];

    ctx.save();
    // Clip to chart area so nothing bleeds outside the panel
    ctx.beginPath();
    ctx.rect(chartArea.left, chartArea.top, chartArea.right - chartArea.left, chartArea.bottom - chartArea.top);
    ctx.clip();

    // ── Sector divider lines (every 45°, 8 lines) ───────────────────────────
    const outerRadiusPx = pxPerKm * BULLSEYE_RANGE_KM;
    ctx.save();
    ctx.setLineDash([]);
    ctx.strokeStyle = '#1e293b';
    ctx.lineWidth = 1;
    ctx.globalAlpha = 0.8;
    for (let angleDeg = 0; angleDeg < 360; angleDeg += 45) {
      const rad = (angleDeg - 90) * (Math.PI / 180); // -90 so 0° = North = up
      ctx.beginPath();
      ctx.moveTo(originX, originY);
      ctx.lineTo(originX + outerRadiusPx * Math.cos(rad), originY + outerRadiusPx * Math.sin(rad));
      ctx.stroke();
    }
    ctx.restore();

    // ── Degree labels at each sector boundary (on the outermost ring) ────────
    ctx.save();
    ctx.font = "9px 'IBM Plex Mono', monospace";
    ctx.globalAlpha = 0.55;
    ctx.fillStyle = '#64748b';
    const labelRadiusPx = pxPerKm * (BULLSEYE_RANGE_KM * 0.82); // slightly inside outer ring
    [0, 45, 90, 135, 180, 225, 270, 315].forEach((angleDeg) => {
      const rad = (angleDeg - 90) * (Math.PI / 180);
      const lx = originX + labelRadiusPx * Math.cos(rad);
      const ly = originY + labelRadiusPx * Math.sin(rad);
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(`${angleDeg}`, lx, ly);
    });
    ctx.restore();

    // ── Dashed range rings ───────────────────────────────────────────────────
    rings.forEach(({ radiusKm, color, label }) => {
      const canvasRadius = pxPerKm * radiusKm;
      if (canvasRadius < 2) return;

      ctx.beginPath();
      ctx.setLineDash([4, 6]);
      ctx.arc(originX, originY, canvasRadius, 0, Math.PI * 2);
      ctx.strokeStyle = color;
      ctx.lineWidth = 1;
      ctx.globalAlpha = 0.75;
      ctx.stroke();

      // Ring distance label — placed at the 0° (East / right) intercept
      ctx.setLineDash([]);
      ctx.globalAlpha = 0.9;
      ctx.font = "9px 'IBM Plex Mono', monospace";
      ctx.fillStyle = color;
      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';
      ctx.fillText(label, originX + canvasRadius + 3, originY);
    });

    // ── Cardinal direction labels ─────────────────────────────────────────────
    ctx.setLineDash([]);
    ctx.globalAlpha = 0.35;
    ctx.font = "8px 'IBM Plex Mono', monospace";
    ctx.fillStyle = '#475569';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    const cardinalR = pxPerKm * BULLSEYE_RANGE_KM * 1.04;
    ctx.fillText('N', originX, originY - cardinalR);
    ctx.fillText('S', originX, originY + cardinalR);
    ctx.textAlign = 'left';
    ctx.fillText('E', originX + cardinalR, originY);
    ctx.textAlign = 'right';
    ctx.fillText('W', originX - cardinalR, originY);

    // ── Centre satellite dot ─────────────────────────────────────────────────
    ctx.setLineDash([]);
    ctx.globalAlpha = 1;
    ctx.beginPath();
    ctx.arc(originX, originY, 6, 0, Math.PI * 2);
    ctx.fillStyle = '#2563eb';
    ctx.fill();
    ctx.beginPath();
    ctx.arc(originX, originY, 6, 0, Math.PI * 2);
    ctx.strokeStyle = '#60a5fa';
    ctx.lineWidth = 1.5;
    ctx.stroke();

    ctx.restore();
  }
};

Chart.register(bullseyeRingsPlugin);
 
// ─────────────────────────────────────────────────────────────────────────────
// 1. Bullseye Chart
// ────────────────────────────────────────────────────────── ───────────────────
export function initBullseyeChart(canvasId: string) {
  const ctx = document.getElementById(canvasId) as HTMLCanvasElement;
  if (!ctx) return;
 
  const config: ChartConfiguration = {
    type: 'scatter',
    data: {
      datasets: [
        { label: 'Safe',     data: [], backgroundColor: THEME.safe,     pointRadius: 4 },
        { label: 'Warning',  data: [], backgroundColor: THEME.warning,  pointRadius: 5 },
        { label: 'Critical', data: [], backgroundColor: THEME.critical, pointRadius: 7,
          borderColor: 'rgba(239,68,68,0.5)', borderWidth: 3 }
      ]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: false,
      plugins: {
        legend: { display: false },
        tooltip: {
          callbacks: {
            label: (item) => {
              const x = item.parsed.x ?? 0; // East component (km)
              const y = item.parsed.y ?? 0; // North component (km)
              const dist = Math.sqrt(x * x + y * y).toFixed(2);
              const bear = ((Math.atan2(x, y) * (180 / Math.PI)) + 360) % 360;
              return [
                `Distance: ${dist} km`,
                `Bearing: ${bear.toFixed(0)}°`,
                `E: ${x.toFixed(2)} km  N: ${y.toFixed(2)} km`,
              ];
            }
          }
        }
      },
      scales: {
        x: { min: -BULLSEYE_RANGE_KM, max: BULLSEYE_RANGE_KM, grid: { display: false }, border: { display: false }, ticks: { display: false } },
        y: { min: -BULLSEYE_RANGE_KM, max: BULLSEYE_RANGE_KM, grid: { display: false }, border: { display: false }, ticks: { display: false } },
      }
    }
  };
 
  bullseyeChart = new Chart(ctx, config);
 
  const select = document.getElementById('satellite-select') as HTMLSelectElement;
  if (select) {
    select.addEventListener('change', () => {
      selectedSatelliteId = select.value || null;
    });
  }
}
 
// ─────────────────────────────────────────────────────────────────────────────
// 2. Per-Satellite Fuel List (DOM-rendered)
// ─────────────────────────────────────────────────────────────────────────────
function updateFuelList(satellites: satelliteInformation[]) {
  const list = document.getElementById('fuel-list');
  if (!list) return;
 
  list.innerHTML = '';
  const sorted = [...satellites].sort((a, b) => a.fuel_kg - b.fuel_kg);
 
  sorted.forEach((sat) => {
    const pct = Math.min(100, (sat.fuel_kg / 50) * 100);
    const barColor = sat.fuel_kg < 10 ? THEME.critical : sat.fuel_kg < 25 ? THEME.warning : THEME.accent;
    const statusDot = sat.status === 'CRITICAL' ? THEME.critical : sat.status === 'WARNING' ? THEME.warning : THEME.safe;
 
    const item = document.createElement('div');
    item.className = 'fuel-item';
    item.innerHTML = `
      <span style="width:6px;height:6px;border-radius:50%;background:${statusDot};flex-shrink:0;display:inline-block;"></span>
      <span class="fuel-item-id" title="${sat.id}">${sat.id}</span>
      <div class="fuel-item-bar-bg"><div class="fuel-item-bar" style="width:${pct}%;background:${barColor};"></div></div>
      <span class="fuel-item-value">${sat.fuel_kg.toFixed(1)}kg</span>
    `;
    list.appendChild(item);
  });
}
 
// ─────────────────────────────────────────────────────────────────────────────
// 3. Efficiency Chart
// ─────────────────────────────────────────────────────────────────────────────
export function initEfficiencyChart(canvasId: string) {
  const ctx = document.getElementById(canvasId) as HTMLCanvasElement;
  if (!ctx) return;
 
  // X = cumulative fuel consumed (kg), Y = cumulative collisions avoided.
  // Scatter rendered as a line — each new point appended as burns/avoidances happen.
  const config: ChartConfiguration<'scatter'> = {
    type: 'scatter',
    data: {
      datasets: [{
        label: 'Fleet Efficiency',
        data: [{ x: 0, y: 0 }],
        borderColor: THEME.accent,
        backgroundColor: 'rgba(56,189,248,0.15)',
        showLine: true,
        borderWidth: 2,
        tension: 0.25,
        pointRadius: 3,
        pointHoverRadius: 5,
        fill: false,
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: false,
      plugins: {
        legend: { display: false },
        tooltip: {
          callbacks: {
            label: (item) =>
              `Fuel: ${(item.parsed.x as number).toFixed(2)} kg  |  Avoided: ${item.parsed.y}`
          }
        }
      },
      scales: {
        x: {
          type: 'linear',
          grid: { color: THEME.grid },
          border: { display: false },
          ticks: { font: { size: 9 }, callback: (v) => `${v}kg` },
          title: { display: true, text: 'fuel consumed (kg)', font: { size: 9 }, color: THEME.text }
        },
        y: {
          type: 'linear',
          grid: { color: THEME.grid },
          border: { display: false },
          ticks: { font: { size: 9 }, stepSize: 1, precision: 0 },
          title: { display: true, text: 'collisions avoided', font: { size: 9 }, color: THEME.text },
          min: 0,
        }
      }
    }
  };
 
  efficiencyChart = new Chart(ctx, config);
}
 
// ─────────────────────────────────────────────────────────────────────────────
// 4. Gantt Chart (clean — 2 datasets only)
// ─────────────────────────────────────────────────────────────────────────────
export function initGanttChart(canvasId: string) {
  const ctx = document.getElementById(canvasId) as HTMLCanvasElement;
  if (!ctx) return;
 
  // Start with EMPTY data — the analytics subscription will populate it within 3s.
  // This prevents the hardcoded placeholder flash and avoids row-count mismatches
  // when Chart.js tries to reconcile labels vs dataset lengths on the first update.
  const config: ChartConfiguration = {
    type: 'bar',
    data: {
      labels: [],
      datasets: [
        {
          label: 'Burn Window',
          data: [],
          backgroundColor: THEME.accent,
          barPercentage: 0.6,
          borderRadius: 2,
        },
        {
          label: 'Thruster Cooldown (600s)',
          data: [],
          backgroundColor: THEME.cooldown,
          barPercentage: 0.35,
          borderRadius: 2,
        }
      ]
    },
    options: {
      indexAxis: 'y',
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: {
          display: true,
          position: 'top',
          labels: { boxWidth: 10, usePointStyle: true, font: { size: 10 }, padding: 12 }
        },
        tooltip: {
          callbacks: {
            label: (item) => {
              const raw = item.raw as [number, number];
              const sign = (v: number) => v >= 0 ? `T+${v}s` : `T${v}s`;
              return `${item.dataset.label}: ${sign(Math.round(raw[0]))} → ${sign(Math.round(raw[1]))}`;
            }
          }
        }
      },
      scales: {
        x: {
          // Let Chart.js auto-scale to the actual data range so no maneuvers get clipped.
          // We only set a reasonable min so past burns (negative relative time) still show.
          grid: { color: THEME.grid },
          border: { display: false },
          ticks: {
            font: { size: 9 },
            maxTicksLimit: 8,
            callback: (val) => {
              const v = val as number;
              if (v === 0) return 'NOW';
              return v >= 0 ? `T+${v}s` : `T${v}s`;
            }
          }
        },
        y: {
          // stacked: true is required for the Gantt pattern: both burn and cooldown bars
          // share the same Y row per satellite. Without this they render as grouped bars
          // on separate rows, breaking the visual.
          stacked: false,
          grid: { display: false },
          border: { display: false },
          ticks: { font: { size: 10 } }
        }
      }
    }
  };
 
  ganttChart = new Chart(ctx, config);
}
 
// Stub for compatibility — replaced by DOM fuel list
export function initFuelGauge(_canvasId: string) { /* no-op */ }
 
// ─────────────────────────────────────────────────────────────────────────────
// SUBSCRIPTIONS
// ─────────────────────────────────────────────────────────────────────────────
export function setupChartSubscriptions(store: stateManagement) {
  store.subscribe((snapshot: visualSnapshot) => {
    if (!snapshot || snapshot.timestamp === lastTimestamp) return;
    lastTimestamp = snapshot.timestamp;
    updateDashboardCharts(snapshot);
  });
}
 
function updateSatelliteDropdown(satellites: satelliteInformation[]) {
  const select = document.getElementById('satellite-select') as HTMLSelectElement;
  if (!select) return;
 
  const currentValue = select.value;
  select.innerHTML = '<option value="">— SELECT SAT —</option>';
  satellites.forEach(sat => {
    const opt = document.createElement('option');
    opt.value = sat.id;
    opt.textContent = sat.id;
    select.appendChild(opt);
  });
 
  if (currentValue && satellites.find(s => s.id === currentValue)) {
    select.value = currentValue;
    selectedSatelliteId = currentValue;
  } else if (!selectedSatelliteId && satellites.length > 0) {
    select.value = satellites[0].id;
    selectedSatelliteId = satellites[0].id;
  }
}
 
function updateDashboardCharts(snapshot: visualSnapshot) {
  if (snapshot.satellites && snapshot.satellites.length > 0) {
    updateSatelliteDropdown(snapshot.satellites);
    updateFuelList(snapshot.satellites);
  }

  if (snapshot.debris_cloud && snapshot.debris_cloud.length > 0) {
    const selectedSat = selectedSatelliteId
      ? snapshot.satellites?.find(s => s.id === selectedSatelliteId)
      : snapshot.satellites?.[0];

    const safeData: Point[] = [];
    const warningData: Point[] = [];
    const criticalData: Point[] = [];

    if (selectedSat) {
      snapshot.debris_cloud.forEach((debris) => {
        // debris = [id, lat, lon, alt_km]
        const distKm = haversineKm(selectedSat.lat, selectedSat.lon, debris[1], debris[2]);

        // Only plot debris within the 10 km proximity zone
        if (distKm > BULLSEYE_RANGE_KM) return;

        // Convert to polar → cartesian for scatter chart
        // x = East component, y = North component (North = up on chart)
        const bear = bearingDeg(selectedSat.lat, selectedSat.lon, debris[1], debris[2]);
        const { x, y } = polarToCartesian(distKm, bear);

        if (distKm < 1.0)       criticalData.push({ x, y });
        else if (distKm < 5.0)  warningData.push({ x, y });
        else                    safeData.push({ x, y });
      });
    }

    bullseyeChart.data.datasets[0].data = safeData;
    bullseyeChart.data.datasets[1].data = warningData;
    bullseyeChart.data.datasets[2].data = criticalData;
    bullseyeChart.update('none');
  }
}
 
let lastAnalyticsTimestamp: string | null = null;
 
// ─── EFFICIENCY CURVE STATE ───────────────────────────────────────────────────
// Tracks the last debrisAvoided value we received so we only push a new point
// when either fuel has been burned OR a new avoidance event was registered.
// We read cumulativeFuelConsumed directly from apiClient (updated by accountFuelDelta).
let lastEfficiencyFuel = 0;
let lastDebrisAvoided  = 0;
 
export function setupAnalyticsSubscriptions(analyticsStore: AnalyticsStore) {
  analyticsStore.subscribe((snapshot: AnalyticsSnapshot) => {
    if (!snapshot || snapshot.timestamp === lastAnalyticsTimestamp) return;
    lastAnalyticsTimestamp = snapshot.timestamp;
 
    const now = Date.now();
 
    // ── 1. EFFICIENCY CURVE UPDATE ────────────────────────────────────────────
    // cumulativeFuelConsumed is a live module-level variable in apiClient.ts,
    // updated every time fetchSnapshot() runs accountFuelDelta().
    // snapshot.debrisAvoided is the cumulative count from the backend (or fallback).
    //
    // We push a new point ONLY when something actually changed — either more fuel
    // was burned since the last analytics tick, or more debris were avoided.
    // This makes the chart grow only when real events happen, matching Image 2.
    const fuelNow      = cumulativeFuelConsumed;
    const avoidsNow    = snapshot.debrisAvoided ?? 0;
    const fuelChanged  = fuelNow > lastEfficiencyFuel + 0.001; // >1g tolerance
    const avoidsChanged = avoidsNow > lastDebrisAvoided;
 
    if ((fuelChanged || avoidsChanged) && efficiencyChart?.data.datasets?.[0]) {
      const pts = efficiencyChart.data.datasets[0].data as { x: number; y: number }[];
 
      // Append the new (cumulative fuel, cumulative collisions) coordinate
      pts.push({ x: parseFloat(fuelNow.toFixed(3)), y: avoidsNow });
 
      // Cap to last 200 points so the chart never gets sluggish over a long session
      if (pts.length > 200) pts.splice(0, pts.length - 200);
 
      lastEfficiencyFuel  = fuelNow;
      lastDebrisAvoided   = avoidsNow;
 
      efficiencyChart.update('none'); // skip animation for real-time feel
    }
 
    // ── 2. GANTT UPDATE ───────────────────────────────────────────────────────
    if (snapshot.maneuvers && ganttChart?.data.datasets) {
      // One row per UNIQUE satellite. A satellite may have multiple maneuvers
      // (e.g. evasion burn + recovery burn). We merge them into one row by
      // taking the first burn window and attaching a single cooldown after the
      // LAST burn end time, which is the correct physical interpretation.
      const satMap = new Map<string, { burns: [number,number][], lastEnd: number }>();
 
      snapshot.maneuvers
        .filter(m => m.type === 'BURN')   // only render BURN events; COAST has no block
        .forEach((maneuver) => {
          const startMs       = new Date(maneuver.startTime).getTime();
          const endMs         = new Date(maneuver.endTime).getTime();
          const relativeStart = (startMs - now) / 1000;
          const relativeEnd   = (endMs   - now) / 1000;
 
          if (!satMap.has(maneuver.satelliteId)) {
            satMap.set(maneuver.satelliteId, { burns: [], lastEnd: relativeEnd });
          }
          const entry = satMap.get(maneuver.satelliteId)!;
          entry.burns.push([relativeStart, relativeEnd]);
          entry.lastEnd = Math.max(entry.lastEnd, relativeEnd);
        });
 
      // Sort rows: soonest burn first so the schedule reads top-to-bottom chronologically
      const sorted = [...satMap.entries()].sort(
        ([, a], [, b]) => a.burns[0][0] - b.burns[0][0]
      );
 
      const labels:      string[]               = [];
      const burnData:    ([number,number] | null)[] = [];
      const cooldownData:([number,number] | null)[] = [];
 
      sorted.forEach(([satId, { burns, lastEnd }]) => {
        // Use first burn window as the displayed bar (widest span if multiple)
        const earliest = Math.min(...burns.map(b => b[0]));
        const latest   = Math.max(...burns.map(b => b[1]));
 
        labels.push(satId);
        burnData.push([earliest, latest]);
        // 600s mandatory cooldown starts at the end of the last burn
        cooldownData.push([lastEnd, lastEnd + 600]);
      });
 
      // Push a null placeholder if no maneuvers so Chart.js doesn't error on empty
      if (labels.length === 0) {
        labels.push('—');
        burnData.push([0, 0]);
        cooldownData.push([0, 0]);
      }
 
      ganttChart.data.labels           = labels;
      ganttChart.data.datasets[0].data = burnData;
      ganttChart.data.datasets[1].data = cooldownData;
      ganttChart.update();
    }
  });
}