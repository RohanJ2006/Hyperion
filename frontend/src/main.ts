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

// LOCAL fallBack if websocket fails 
function localFallBack(sharedMem: Float64Array, computeMercator: Function, pixiApp: any) {
    const ENTITIES = 1000;
    const STRIDE = 7;   

    // Orbital params per entity: [inclination, angularSpeed, phase, raan (lon offset)]
    const orbitParams = new Float64Array(ENTITIES * 4);

    for (let i = 0; i < ENTITIES; i++) {
        const base = i * STRIDE;
        const oBase = i * 4;

        sharedMem[base + 0] = i; // ID
        // sharedMem[base + 3] = 400 + Math.random() * 600; // Altitude

        const isSatellite = i < 200; // give more active satellite 
        sharedMem[base + 6] = isSatellite ? 1.0 : 0.0;

        let inc, speed, phase, raan;
        const rand = Math.random();

        // --- ORBITAL CLUSTERING FOR REALISM ---
        if (rand < 0.05) {
            // 1. GEOSTATIONARY RING (~5%): Zero inclination, matches Earth rotation. 
            // Creates a beautiful static ring of debris on the equator.
            inc = (Math.random() * 2) * (Math.PI / 180);
            speed = 0.1; // Matches earth rotation speed
            phase = Math.random() * Math.PI * 2;
            raan = Math.random() * 360;
        } else if (rand < 0.35) {
            // 2. SUN-SYNCHRONOUS ORBIT (~30%): Almost polar.
            // Creates dense vertical traffic moving steadily across the map.
            inc = (98.2 + (Math.random() - 0.5) * 2) * (Math.PI / 180);
            speed = 0.3 + Math.random() * 0.1; 
            phase = Math.random() * Math.PI * 2;
            raan = Math.random() * 360;
        } else if (rand < 0.50) {
            // 3. COLLISION CLOUD (~15%): E.g., The Cosmos/Iridium debris field.
            // Same orbital plane, just spread out along the path. Looks like a trailing swarm.
            inc = 86.4 * (Math.PI / 180); 
            speed = 0.45 + (Math.random() - 0.5) * 0.03;
            phase = Math.random() * Math.PI * 2;
            raan = 120.0 + (Math.random() - 0.5) * 5; // Tightly packed longitude node
        } else {
            // 4. STANDARD LEO (~50%): Varied inclinations and speeds.
            inc = (Math.random() * 80 + 10) * (Math.PI / 180);
            speed = 0.5 + Math.random() * 0.3;
            phase = Math.random() * Math.PI * 2;
            raan = Math.random() * 360;
        }

        // Make Active Satellites prominent (e.g., ISS orbit at 51.6 degrees)
        if (isSatellite) {
            inc = 51.6 * (Math.PI / 180);
            speed = 0.7; // Slightly faster to stand out
        }

        orbitParams[oBase + 0] = inc;
        orbitParams[oBase + 1] = speed;
        orbitParams[oBase + 2] = phase;
        orbitParams[oBase + 3] = raan;
    }

    let time = 0;

    // Lower these to slow down the entire simulation without breaking the math
    const SIM_SPEED_MULTIPLIER = 0.002; 
    const EARTH_ROTATION_SPEED = 0.05; 

    let lastFpsTime = performance.now();
    let framesThisSecond = 0;
    const fpsElement = document.getElementById('fps-counter');

    function animate() {
        time += 1;

        for (let i = 0; i < ENTITIES; i++) {
            const base = i * STRIDE;
            const oBase = i * 4;

            const inc = orbitParams[oBase + 0];
            const speed = orbitParams[oBase + 1] * SIM_SPEED_MULTIPLIER;
            const phase = orbitParams[oBase + 2];
            const raan = orbitParams[oBase + 3];

            // Current angle of the object in its orbital plane
            const theta = phase + time * speed;

            // 1. Latitude: Standard sinusoidal projection
            const lat = Math.asin(Math.sin(inc) * Math.sin(theta)) * (180 / Math.PI);

            // 2. Longitude: The atan2 equation creates the realistic 'S' curve ground track
            let lon = Math.atan2(Math.cos(inc) * Math.sin(theta), Math.cos(theta)) * (180 / Math.PI);

            // Apply the Right Ascension offset and subtract Earth's rotation to create westward drift
            lon = lon + raan - (time * EARTH_ROTATION_SPEED * SIM_SPEED_MULTIPLIER);

            // Safely wrap longitude to strictly stay within -180 to 180 (avoids Mercator snapping)
            lon = ((lon + 540) % 360) - 180;

            sharedMem[base + 1] = lat;
            sharedMem[base + 2] = lon;
        }

        // Execute the heavy WASM math and draw the frame
        computeMercator(ENTITIES, pixiApp.mapWidth, pixiApp.mapHeight);
        pixiApp.renderFrame(ENTITIES);

        framesThisSecond++;
        const now = performance.now();
        
        // Only update the DOM if 1000 milliseconds (1 second) have passed
        if (now - lastFpsTime >= 1000) {
            if (fpsElement) {
                fpsElement.textContent = `FPS = ${framesThisSecond} `;
                
                // Visual indicator of engine health
                if (framesThisSecond >= 100) {
                    fpsElement.style.color = '#10b981'; // Green (Optimal)
                } else if (framesThisSecond >= 60) {
                    fpsElement.style.color = '#f59e0b'; // Amber (Warning)
                } else {
                    fpsElement.style.color = '#ef4444'; // Red (Critical)
                }
            }
            
            // Reset counters for the next second
            framesThisSecond = 0;
            lastFpsTime = now;
        }

        requestAnimationFrame(animate);
    }

    animate();
}

bootstrap();