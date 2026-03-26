import { Application, Assets, Sprite, Texture, Rectangle, Container } from 'pixi.js';

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
    sprite.scale.set(5);
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