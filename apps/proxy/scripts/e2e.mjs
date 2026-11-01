// End-to-end test of a running TTV-TTS Proxy (debug build) against a real VCP.
//   node apps/proxy/scripts/e2e.mjs <proxy host:port> <access key> [direct VCP base for comparison]
// The proxy must run with TTV_PROXY_VCP_KEY set (debug builds), so /ttv/v1/dev/pairing exists.
// Uses NODE_TLS_REJECT_UNAUTHORIZED=0 for fetch; the certificate pin is checked separately.
import tls from "node:tls";
import dgram from "node:dgram";

process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
process.removeAllListeners("warning");

const [hostPort = "127.0.0.1:8110", key = "", direct = ""] = process.argv.slice(2);
const [host, port] = hostPort.split(":");
const base = `https://${hostPort}`;
let failed = 0;
const ok = (cond, name, extra = "") => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${extra ? `  (${extra})` : ""}`);
  if (!cond) failed++;
};
const j = async (path, opts = {}) => {
  const res = await fetch(base + path, opts);
  let body = null;
  try {
    body = await res.json();
  } catch {}
  return { status: res.status, body };
};
const auth = (k) => ({ headers: { "X-API-Key": k } });

// 1. public info
const info = await j("/ttv/v1/info");
ok(info.status === 200 && info.body.api === 1 && info.body.fingerprint.length === 64, "GET /ttv/v1/info is public", `${info.body?.name} v${info.body?.version}, VCP ${info.body?.vcp_version}`);

// 2. certificate pin = fingerprint from /info
const fp = await new Promise((resolve, reject) => {
  const s = tls.connect({ host, port: Number(port), rejectUnauthorized: false }, () => {
    resolve(s.getPeerCertificate().fingerprint256.replaceAll(":", "").toLowerCase());
    s.end();
  });
  s.on("error", reject);
});
ok(fp === info.body.fingerprint, "TLS certificate matches the advertised fingerprint");

// 3. auth
ok((await j("/ttv/v1/status")).status === 401, "status without key -> 401");
ok((await j("/api/v1/voices", auth("wrong-key"))).status === 401, "forwarded call with wrong key -> 401");
ok((await j("/api/v1/health")).status === 200, "VCP health stays public through the proxy");
const st = await j("/ttv/v1/status", auth(key));
ok(st.status === 200 && st.body.vcp.reachable, "status with access key", `VCP reachable=${st.body?.vcp?.reachable} key_ok=${st.body?.vcp?.key_ok} model=${st.body?.vcp?.model_name}`);
ok((await j(`/ttv/v1/status?api_key=${encodeURIComponent(key)}`)).status === 200, "key as ?api_key= works");
ok((await j("/ttv/v1/status", { headers: { Authorization: `Bearer ${key}` } })).status === 200, "key as Bearer works");

// 4. pairing
const code = (await j("/ttv/v1/dev/pairing", { method: "POST", ...auth(key) })).body?.code;
const wrong = code === "000000" ? "111111" : "000000";
const bad = await j("/ttv/v1/pair", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ code: wrong, device_name: "e2e", device_id: "e2e-test" }) });
ok(bad.status === 403 && bad.body.code === "bad_code", "wrong pairing code rejected");
const pair = await j("/ttv/v1/pair", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ code, device_name: "E2E test", device_id: "e2e-test" }) });
ok(pair.status === 200 && pair.body.token?.length === 32, "pairing with the right code returns a token");
const token = pair.body?.token;
const again = await j("/ttv/v1/pair", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ code, device_name: "x", device_id: "x" }) });
ok(again.status === 403 && again.body.code === "pairing_closed", "a used code can't pair twice");
const viaToken = await j("/api/v1/voices", auth(token));
ok(viaToken.status === 200 && Array.isArray(viaToken.body.voices), "device token reaches VCP through the proxy", `${viaToken.body?.voices?.length} voices`);

// 5. events WebSocket
const ws = new WebSocket(`wss://${hostPort}/ttv/v1/events?system=1&api_key=${encodeURIComponent(token)}`);
const events = [];
ws.onmessage = (e) => events.push(JSON.parse(e.data).type);
await new Promise((r) => setTimeout(r, 4500));
ws.close();
ok(events[0] === "status" && events.includes("system"), "events WebSocket sends status, then system stats", events.slice(0, 6).join(","));

// 6. speech stream through the proxy (and directly for comparison)
async function speak(b, k, label) {
  const t0 = performance.now();
  const res = await fetch(`${b}/api/v1/speak`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-API-Key": k },
    body: JSON.stringify({ text: "Hello from the TTV-TTS proxy test.", language: "English", deliver: "url" }),
  });
  const sp = await res.json();
  if (!sp.session_id) return ok(false, `${label}: speak`, JSON.stringify(sp).slice(0, 160));
  const audio = await fetch(`${b}/api/v1/streaming/${sp.session_id}/audio?format=wav`, { headers: { "X-API-Key": k } });
  const reader = audio.body.getReader();
  let first = 0;
  let bytes = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!first && bytes + value.length > 44) first = performance.now() - t0;
    bytes += value.length;
  }
  const total = performance.now() - t0;
  ok(bytes > 44 * 100, `${label}: speech streamed`, `first audio ${first.toFixed(0)} ms, done ${total.toFixed(0)} ms, ${((bytes - 44) / 48000).toFixed(1)} s audio, voice ${sp.voice_name}`);
  return first;
}
await speak(base, token, "proxy (warm-up)");
const viaProxy = await speak(base, token, "proxy");
if (direct) {
  const viaDirect = await speak(direct, key, "direct");
  console.log(`      proxy overhead on first audio: ${(viaProxy - viaDirect).toFixed(0)} ms`);
}

// 7. LAN discovery (UDP)
const reply = await new Promise((resolve) => {
  const sock = dgram.createSocket("udp4");
  const timer = setTimeout(() => (sock.close(), resolve(null)), 1500);
  sock.on("message", (msg) => {
    clearTimeout(timer);
    sock.close();
    resolve(msg.toString());
  });
  sock.send(Buffer.from("TTVTTS-DISCOVER/1"), Number(port), host);
});
ok(!!reply && reply.startsWith("TTVTTS-PROXY/1 ") && JSON.parse(reply.slice(15)).fingerprint === info.body.fingerprint, "UDP discovery answers with the proxy info");

// 8. clean up the test device
const status2 = await j("/ttv/v1/status", auth(key));
ok(status2.body.devices.some((d) => d.id === "e2e-test" && d.last_address), "paired device is listed with its address");

console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
