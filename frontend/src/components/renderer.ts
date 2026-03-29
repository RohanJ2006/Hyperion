import { Application, Assets, Sprite, Texture, Rectangle, Container , Graphics } from 'pixi.js';

// [0: ID, 1: Lat, 2: Lon, 3: Alt, 4: X, 5: Y, 6: TYPE]
const ENTITY_STRIDE = 7; 
const MAX_ENTITIES = 25_000;


export async function pixiInit(sharedMemory: Float64Array) { 
  const container = document.getElementById('pixi-container') as HTMLElement || null;
  if(!container) return;

  const app = new Application();
  await app.init({
    resizeTo : container,
    antialias : false, 
    resolution: window.devicePixelRatio || 1,
    autoDensity: true,
    preference: 'webgl'
  });

  container.appendChild(app.canvas);

  // Load and Setup the Map 
  const MapTexture = await Assets.load("/mercator_map3.png");
  MapTexture.source.scaleMode = 'linear';
  const mapSprite = new Sprite(MapTexture);
  
  // Center the anchor so scaling scales from the middle
  mapSprite.anchor.set(0.5);
  app.stage.addChild(mapSprite);

  // We create a container specifically for the entities and put it INSIDE the mapSprite.
  const entityContainer = new Container();
  // Because mapSprite's anchor is 0.5, its internal (0,0) is in the dead center.
  // We offset the container to the top-left so Rust can just calculate X/Y from (0 to MapWidth).
  entityContainer.x = -MapTexture.width / 2;
  entityContainer.y = -MapTexture.height / 2;
  mapSprite.addChild(entityContainer);

  // 2. The Trajectory Layer (NEW) 
  const trajectoryLayer = new Graphics();
  entityContainer.addChild(trajectoryLayer);

  //NEW: The Terminator Line (Day/Night Shadow) 
  const terminatorShadow = new Graphics();
  // Add it BEFORE the sprites so it renders underneath them
  entityContainer.addChild(terminatorShadow);

  function updateTerminator() {
    const now = new Date();
    const mapWidth = MapTexture.width;
    const mapHeight = MapTexture.height;

    terminatorShadow.clear();

    // 1. Calculate Sun's Position
    const dayOfYear = Math.floor((now.getTime() - new Date(now.getFullYear(), 0, 0).getTime()) / 86400000);
    const gamma = (2 * Math.PI / 365) * (dayOfYear - 1 + (now.getUTCHours() - 12) / 24);
    
    // Solar declination (approximate formula)
    let declination = 0.006918 - 0.399912 * Math.cos(gamma) + 0.070257 * Math.sin(gamma) - 0.006758 * Math.cos(2 * gamma) + 0.000907 * Math.sin(2 * gamma);
    // Prevent division by zero at equinoxes
    if (declination === 0) declination = 0.00001; 

    // Subsolar longitude (where the sun is currently overhead)
    const timeOffset = (now.getUTCHours() + now.getUTCMinutes() / 60 + now.getUTCSeconds() / 3600) - 12;
    const subsolarLonRad = -timeOffset * 15 * (Math.PI / 180);

    // 2. Build the shadow polygon
    const points: number[] = [];
    
    for (let lonDeg = -180; lonDeg <= 180; lonDeg += 2) {
      const lonRad = lonDeg * (Math.PI / 180);

      // Latitude of the terminator for this longitude
      const latRad = Math.atan(-Math.cos(lonRad - subsolarLonRad) / Math.tan(declination));
      const latDeg = latRad * (180 / Math.PI);

      // Convert to Mercator X/Y (Mirroring your Rust backend logic exactly)
      const x = ((lonDeg + 180) / 360) * mapWidth;
      const clampedLat = Math.max(-85.051129, Math.min(85.051129, latDeg));
      const clampedLatRad = clampedLat * (Math.PI / 180);
      const mercN = Math.log(Math.tan((Math.PI / 4) + (clampedLatRad / 2)));
      const y = (mapHeight / 2) * (1 - (mercN / Math.PI));

      points.push(x, y);
    }

    // Close the polygon at the top or bottom depending on the season
    const isNorthernSummer = declination > 0;
    points.push(mapWidth, isNorthernSummer ? mapHeight : 0);
    points.push(0, isNorthernSummer ? mapHeight : 0);

    // Draw using PixiJS v8 syntax
    terminatorShadow.poly(points);
    terminatorShadow.fill({ color: 0x000000, alpha: 0.5 }); // 50% opacity black
  }

  // Draw the shadow immediately on load
  updateTerminator();

  // Update the shadow once a minute to match Earth's rotation, 
  // entirely decoupled from the 60FPS render loop.
  setInterval(updateTerminator, 60_000);

  // --- 3. The Texture Atlas (Performance Secret) ---
  // Draw both your purple satellite and a red debris dot on one hidden canvas
  const atlasCanvas = document.createElement('canvas');
  atlasCanvas.width = 16;
  atlasCanvas.height = 8;
  const ctx = atlasCanvas.getContext('2d')!;

  // Draw Satellite
  ctx.fillStyle = '#2016ed';
  ctx.beginPath(); ctx.arc(4, 4, 4, 0, Math.PI * 4); ctx.fill();

  // Draw Debris
  ctx.fillStyle = '#ff4444cc';
  ctx.beginPath(); ctx.arc(12, 4, 2, 0, Math.PI * 4); ctx.fill();

  const atlasTexture = Texture.from(atlasCanvas);

const satTexture = new Texture({
  source: atlasTexture.source,
  frame: new Rectangle(0, 0, 8, 8),
});

const debrisTexture = new Texture({
  source: atlasTexture.source,
  frame: new Rectangle(8, 0, 8, 8),
});

  // --- 4. Pre-allocate Sprites ---
  const sprites: Sprite[] = new Array(MAX_ENTITIES);
  for (let i = 0; i < MAX_ENTITIES; i++) {
    const sprite = new Sprite(debrisTexture); // Default texture
    sprite.anchor.set(0.5);
    sprite.visible = false; // Hide until data arrives
    entityContainer.addChild(sprite);
    sprites[i] = sprite;
  }

  // --- 5. Responsive Resize Loop ---
  let lastWidth = 0;
  let lastHeight = 0;

  app.ticker.add(() => {
    if (app.screen.width !== lastWidth || app.screen.height !== lastHeight) {
        // Keep map centered
        mapSprite.x = app.screen.width / 2;
        mapSprite.y = app.screen.height / 2;
        
        // Keep map contained while maintaining aspect ratio
        const newScale = Math.min(app.screen.width / MapTexture.width, app.screen.height / MapTexture.height);
        mapSprite.scale.set(newScale);

        lastWidth = app.screen.width;
        lastHeight = app.screen.height;
    }
  });

  // --- 6. The Render Engine ---
  function renderFrame(entityCount: number) {
    const count = Math.min(entityCount, MAX_ENTITIES);
    for (let i = 0; i < count; i++) {
      const offset = i * ENTITY_STRIDE;
      const sprite = sprites[i];

      // DYNAMIC TYPE CHECK: Look at the 7th slot in the memory block (Index 6)
      // 1.0 means Satellite, and 0.0 means Debris
      const isSatellite = sharedMemory[offset + 6] === 1.0;
      sprite.texture = isSatellite ? satTexture : debrisTexture;

      // Update position
      sprite.x = sharedMemory[offset + 4];
      sprite.y = sharedMemory[offset + 5];
      sprite.visible = true;
    }

    // Hide any unused sprites
    for (let i = count; i < MAX_ENTITIES; i++) {
      sprites[i].visible = false;
    }
  }

  // Return the render function AND the raw map dimensions for Rust to use
  return { 
    renderFrame, 
    mapWidth: MapTexture.width, 
    mapHeight: MapTexture.height 
  };
}