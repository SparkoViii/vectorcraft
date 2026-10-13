const f = new Float64Array([1.5, 2.5, 3.5]);
const c = new Uint8ClampedArray([300, -5, 127.5]);
const buf = new ArrayBuffer(4);
const view = new DataView(buf);
view.setUint16(0, 0x1234);
const bytes = Array.from(new Uint8Array(buf));
view.setUint16(2, 0x1234, true);
({
  sum: f.reduce((a, b) => a + b, 0),
  clamped: Array.from(c),
  bytes: Array.from(new Uint8Array(buf)),
  firstTwo: bytes.slice(0, 2),
  mapped: Array.from(f.map((x) => x * 2)),
  sub: Array.from(new Int16Array([1, 2, 3, 4]).subarray(1, 3)),
});
