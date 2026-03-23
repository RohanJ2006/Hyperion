// import Chart from 'chart.js/auto';
import updateClock from './utility/clock';
import fullScreen from './utility/fullScreen';
import pixiInit from './components/Mercator';

setInterval(updateClock, 1000);
updateClock();
fullScreen();



async function bootstrap(): Promise<void> {
    // Pause the execution of this function until the PixiJS canvas is fully initialized and attached
    await pixiInit();
    // Synchronously initialize the polar chart
    
}

// Trigger the master bootstrap function to kick off the entire application cycle when the file loads
bootstrap();