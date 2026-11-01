// End-to-end test of internet access (guest API) on a running TTV-TTS Proxy
// (debug build bound to 127.0.0.1, see e2e.mjs) against a real VCP.
//   node apps/proxy/scripts/internet-e2e.mjs <lan host:port> <access key> <a VCP voice id>
// Turns internet access on, creates an invite, joins over the internet port
// with the pinned certificate and checks what a guest can and cannot do.
// Leaves internet access off again and removes the test guest.
import tls from "node:tls";

process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
process.removeAllListeners("warning");

const [lan = "127.0.0.1:8110", key = "", voiceArg = ""] = process.argv.slice(2);
const lanBase = `https://${lan}`;
let failed = 0;
const ok = (cond, name, extra = "") => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${extra ? `  (${extra})` : ""}`);
  if (!cond) failed++;
};
const call = async (base, path, { token, method = "GET", body } = {}) => {
  const res = await fetch(base + path, { method, headers: { ...(token ? { Authorization: `Bearer ${token}` } : {}), ...(body ? { "Content-Type": "application/json" } : {}) }, body: body ? JSON.stringify(body) : undefined });
  let json = null;
  try {
    json = await res.json();
  } catch {}
  return { status: res.status, body: json };
};
const fingerprintOf = (host, port) =>
  new Promise((resolve, reject) => {
    const s = tls.connect({ host, port: Number(port), rejectUnauthorized: false }, () => {
      resolve(s.getPeerCertificate().fingerprint256.replaceAll(":", "").toLowerCase());
      s.end();
    });
    s.on("error", reject);
  });

// 1. the host switches internet access on (LAN API, admin)
let st = await call(lanBase, "/ttv/v1/internet", { token: key, method: "POST", body: { enabled: true, port: 8111 } });
ok(st.status === 200 && st.body.enabled, "internet access switched on over the LAN API", JSON.stringify(st.body?.error ?? ""));
await new Promise((r) => setTimeout(r, 800));
st = await call(lanBase, "/ttv/v1/internet", { token: key });
ok(st.body?.listening === true, "the internet port listens", `port ${st.body?.port}`);
const lanHost = lan.split(":")[0];
const net = `https://${lanHost}:${st.body.port}`;
ok((await call(lanBase, "/ttv/v1/internet")).status === 401, "managing internet access needs the key");

// the code needs a public address: the test service finds it
const t = await call(lanBase, "/ttv/v1/internet/test", { token: key, method: "POST" });
ok(t.status === 200 && typeof t.body.reachable === "boolean", "reachability test answers", `reachable from the internet: ${t.body?.reachable}`);

// 2. an invite
const inv = await call(lanBase, "/ttv/v1/internet/invites", { token: key, method: "POST", body: { name: "E2E friend", days: 1, chars_per_day: 2000 } });
ok(inv.status === 200 && inv.body.code.startsWith("ttv1:"), "an invite code is made", inv.body?.code?.slice(0, 24));
const decoded = JSON.parse(Buffer.from(inv.body.code.slice(5), "base64url").toString());
const fp = await fingerprintOf(lanHost, st.body.port);
ok(decoded.fingerprint === fp, "the code pins the internet port's certificate");
ok(!JSON.stringify(st.body).includes(decoded.secret), "the secret isn't in the status (stored hashed)");

// 3. joining
const bad = await call(net, "/ttv/v1/join", { method: "POST", body: { secret: "x".repeat(32), device: "e2e" } });
ok(bad.status === 403, "a wrong secret is refused");
const join = await call(net, "/ttv/v1/join", { method: "POST", body: { secret: decoded.secret, device: "e2e" } });
ok(join.status === 200 && join.body.token, "the invite works", `guest ${join.body?.guest?.name}`);
const again = await call(net, "/ttv/v1/join", { method: "POST", body: { secret: decoded.secret, device: "e2e" } });
ok(again.status === 403, "the invite works only once");
const g = join.body.token;

// 4. what a guest sees
const gs = await call(net, "/ttv/v1/status", { token: g });
ok(gs.status === 200 && gs.body.guest?.name === "E2E friend", "guests get their status", `quota ${gs.body?.guest?.chars_today}/${gs.body?.guest?.chars_per_day}`);
ok(gs.body.devices.length === 0 && gs.body.addresses.length === 0 && !gs.body.vcp.exe && !gs.body.vcp.url && !gs.body.update, "…without devices, addresses, paths or updates");
ok((await call(net, "/ttv/v1/library", { token: g })).status === 200, "guests read the library");

// 5. what a guest may not do
for (const [path, method] of [["/ttv/v1/pair", "POST"], ["/ttv/v1/logs", "GET"], ["/ttv/v1/internet", "GET"], ["/ttv/v1/jobs", "POST"], ["/ttv/v1/vcp/stop", "POST"], ["/api/v1/voices", "GET"], ["/api/v1/settings/api-key", "PUT"], ["/api/v1/generation/design", "POST"]]) {
  const r = await call(net, path, { token: g, method, body: method === "GET" ? undefined : {} });
  ok(r.status === 404, `guest: ${method} ${path} doesn't exist`, String(r.status));
}
ok((await call(net, "/ttv/v1/library", { token: key })).status === 401, "the host's key doesn't work on the internet port");
ok((await call(lanBase, "/ttv/v1/status", { token: g })).status === 401, "a guest token doesn't work on the LAN port");
ok((await call(net, "/ttv/v1/library")).status === 401, "no token, no library");

// 6. speaking (a library voice: given, or the first one)
const voice = voiceArg || (await call(lanBase, "/ttv/v1/library", { token: key })).body?.entries?.[0]?.voice_id || "";
const notShared = await call(net, "/api/v1/generation/clone", { token: g, method: "POST", body: { voice_id: "voice_00000000", target_text: "Hallo" } });
ok(notShared.status === 403, "only library voices can be used");
const tooLong = await call(net, "/api/v1/generation/clone", { token: g, method: "POST", body: { voice_id: voice, target_text: "a".repeat(601) } });
ok(tooLong.status === 429 || tooLong.status === 403, "long texts are refused", String(tooLong.status));
const upload = await call(net, "/api/v1/generation/clone", { token: g, method: "POST", body: { ref_audio_base64: "UklGR", ref_text: "x", target_text: "Hallo" } });
ok(upload.status >= 400 && upload.status < 500, "cloning uploaded audio is refused", String(upload.status));
if (voice) {
  const t0 = Date.now();
  const say = await call(net, "/api/v1/generation/clone", { token: g, method: "POST", body: { voice_id: voice, target_text: "Hello from the internet test.", language: "English" } });
  ok(say.status === 200 && say.body?.success && say.body.audio_base64?.length > 1000, "a guest speaks with a library voice", `${Date.now() - t0} ms`);
  const me = await call(net, "/ttv/v1/status", { token: g });
  ok(me.body.guest.chars_today === "Hello from the internet test.".length, "the characters count against the quota", String(me.body.guest.chars_today));
}

// 7. revoke and clean up
const list = await call(lanBase, "/ttv/v1/internet", { token: key });
const me = list.body.guests.find((x) => x.name === "E2E friend");
ok(!!me && !!me.last_address, "the host sees the guest and where they connect from", me?.last_address);
await call(lanBase, `/ttv/v1/internet/guests/${me.id}/revoke`, { token: key, method: "POST" });
ok((await call(net, "/ttv/v1/library", { token: g })).status === 401, "a revoked guest is out at once");
await call(lanBase, "/ttv/v1/internet", { token: key, method: "POST", body: { enabled: false } });
await new Promise((r) => setTimeout(r, 800));
let closed = false;
try {
  await fetch(`${net}/ttv/v1/info`, { signal: AbortSignal.timeout(2000) });
} catch {
  closed = true;
}
ok(closed, "switched off, the internet port is closed");
console.log(failed ? `${failed} FAILED` : "all passed");
process.exit(failed ? 1 : 0);
