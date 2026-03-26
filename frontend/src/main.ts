import updateClock from './utility/clock';
import fullScreen from './utility/fullScreen';
import { pixiInit } from './components/renderer';
import { initWasmCore } from './dataPipeline/wasm_loader'; 
import { initializeTelemetryStream } from './dataPipeline/telemetryStream';
import { 
  initBullseyeChart, initFuelGauge, initEfficiencyChart, initGanttChart, 
  setupChartSubscriptions, setupAnalyticsSubscriptions 
} from './components/charts';
import { createState , createAnalyticsState } from './dataPipeline/stateManagement';

setInterval(updateClock, 1000);
updateClock();
fullScreen();


async function bootstrap(): Promise<void> {
    console.log("Booting Command Center...");

    // 1. Load the Rust Engine
    const wasmCore = await initWasmCore();
    
    // 2. Initialize the UI with the shared memory
    const pixiApp = await pixiInit(wasmCore.sharedMemory);
    if (!pixiApp) return;

    // 3. Setup Telemetry Pipeline
    const telemetry = initializeTelemetryStream(
        wasmCore.sharedMemory, 
        7, 
        (count) => {
            // Tell Rust to do the math using the original Map Image dimensions!
            wasmCore.computeMercator(count, pixiApp.mapWidth, pixiApp.mapHeight);
            // Tell Pixi to draw the new coordinates
            pixiApp.renderFrame(count);
        }
    );

    telemetry.connect();
    // 4. TEST MODE: If WebSocket fails to connect after 2 seconds, simulate data
    setTimeout(() => {
        if (!telemetry.isConnected()) {
            console.warn("No WebSocket connection. Starting Local Demo Mode...");
            localFallBack(wasmCore.sharedMemory, wasmCore.computeMercator, pixiApp);
        }
    }, 5000);

    // 1. Target the canvas elements in your HTML
    initBullseyeChart('bullseye-canvas');
    initFuelGauge('fuel-gauge-canvas'); // Make sure to add this ID to your HTML
    initEfficiencyChart('efficiency-canvas'); // Make sure to add this ID to your HTML
    initGanttChart('timeline-canvas'); // Make sure to add this ID to your HTML

    const stateStore = createState();
  setupChartSubscriptions(stateStore);
  stateStore.start();

  // Slow Pipeline (3s) - Powers Historical Line Chart & Gantt
  const analyticsStore = createAnalyticsState();
  setupAnalyticsSubscriptions(analyticsStore);
  analyticsStore.start();
}



bootstrap();