export interface WasmExport {
  memory : WebAssembly.Memory;
  get_memory_pointer: ()=> number;
  get_buffer_len: ()=> number;
  compute_mercator: (count:number , width:number , height:number)=> void;
}

export interface WasmCore {
 sharedMemory : Float64Array;
 bufferLength : number;
 stride : number;
 computeMercator : (count : number , width : number , height : number) => void ;
}

export async function initWasmCore(): Promise<WasmCore> {
  const response = await fetch('/wasm_core.wasm');
  const bytes = await response.arrayBuffer();

  const { instance } = await WebAssembly.instantiate(bytes);

  const exports = instance.exports as unknown as WasmExport;

  const pointer = exports.get_memory_pointer();
  const bufferLength = exports.get_buffer_len();

  const sharedMemory = new Float64Array(
    exports.memory.buffer,
    pointer,
    bufferLength
  )

  const stride = 7;

  return {
    sharedMemory,
    bufferLength,
    stride,
    computeMercator:(count:number , width : number , height : number) => {
      exports.compute_mercator(count , width , height) ;
    },
    
  }
}