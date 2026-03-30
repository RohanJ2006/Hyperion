export type telemetryCallback = (entityCount:number) => void;

export interface telemetryStream {
  connect: () => void; //connect the websocket
  disconnect: () => void; //disconnect the websocket
  isConnected:()=> boolean; //tells if connected or not 
}

export function initializeTelemetryStream (
  sharedMemory : Float64Array,
  stride : number,
  onData : telemetryCallback
):telemetryStream{
  let ws : WebSocket | null = null;
  let connected = false;

  const floatsPerEntity = stride;

  function connect():void{
    if(ws) return ;
    // ws = new WebSocket('websocket.url'); // this needs to be changed with the url from the backend
    const isDev = import.meta.env.DEV;
    const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsHost = isDev ? 'localhost:8000' : window.location.host;
    // You will eventually need to add the /ws/telemetry WebSocket route in your Axum main.rs to actually push the Float64Array data, but this gets the frontend pointing to the right place
    ws = new WebSocket(`${wsProtocol}//${wsHost}/ws/telemetry`); 
    ws.binaryType = 'arraybuffer';

    ws.onopen = ()=> {
      connected = true;
    };

    ws.onmessage = (event:MessageEvent<ArrayBuffer>) => {
      const incoming = new Float64Array(event.data);
      const safeIncoming = incoming.length > sharedMemory.length 
        ? incoming.subarray(0, sharedMemory.length) 
        : incoming;
      const entityCount = (safeIncoming.length / floatsPerEntity) | 0;

      sharedMemory.set(incoming);

      onData(entityCount);
    };

    ws.onclose = () => {
      connected = false;
      setTimeout(connect, 2000);
    };

    ws.onerror = () => {
      ws?.close();
    }
  }

  function disconnect(): void {
    if(ws) {
      ws.onclose = null;
      ws.close();
      ws = null;
      connected = false;
    }
  }

  function isConnected():boolean{
    return connected;
  }

  return {connect , disconnect , isConnected} ;
}