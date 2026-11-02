// Internet access of a proxy ("friends over the internet"): what the friends
// card shows and does. The proxy window calls its own commands, the app goes
// through the link to its proxy; outside Tauri both use the demo below.

/** A friend who uses a proxy over the internet. */
export interface GuestInfo {
  id: string;
  name: string;
  chars_today: number;
  chars_per_day: number;
  joined: number;
  last_seen: number | null;
  last_address: string | null;
  total_chars: number;
}

export interface InviteView {
  id: string;
  name: string;
  created: number;
  expires: number;
  chars_per_day: number;
}

/** An HTTPS certificate from Let's Encrypt for a DuckDNS name. */
export interface CertStatus {
  domain: string | null;
  valid_from: number | null;
  valid_until: number | null;
  renew_at: number | null;
  /** working on a new one: account, order, dns, validating, issuing */
  step: string | null;
  error: string | null;
  error_at: number | null;
  next_try: number | null;
  staging: boolean;
}

/** Internet access of a proxy. */
export interface InternetStatus {
  enabled: boolean;
  port: number;
  listening: boolean;
  error: string | null;
  domain: string;
  duckdns_token: boolean;
  duckdns_at: number | null;
  duckdns_result: string | null;
  public_ip: string | null;
  test_at: number | null;
  reachable: boolean | null;
  address: string;
  invites: InviteView[];
  guests: GuestInfo[];
  connections: number;
  /** OBS links from the internet use HTTPS */
  obs_https?: boolean;
  https?: CertStatus;
  /** some OBS link is shared on the internet */
  obs_online?: boolean;
}

export interface InternetPatch {
  enabled?: boolean;
  port?: number;
  domain?: string;
  /** "" removes the stored token */
  token?: string;
  obs_https?: boolean;
}

export interface NewInvite {
  invite: InviteView;
  /** the code to send to the friend (shown once) */
  code: string;
}

export interface FriendsApi {
  status(): Promise<InternetStatus>;
  set(patch: InternetPatch): Promise<InternetStatus>;
  test(): Promise<{ reachable: boolean }>;
  invite(name: string, days: number, charsPerDay: number, address: string | null): Promise<NewInvite>;
  withdraw(id: string): Promise<unknown>;
  revoke(id: string): Promise<unknown>;
}

const now = () => Math.floor(Date.now() / 1000);

/** A demo proxy's internet access (UI development, checks, screenshots). */
export function demoFriends(): FriendsApi {
  const internet: InternetStatus = {
    enabled: false,
    port: 8111,
    listening: false,
    error: null,
    domain: "",
    duckdns_token: false,
    duckdns_at: null,
    duckdns_result: null,
    public_ip: "203.0.113.42",
    test_at: null,
    reachable: null,
    address: "203.0.113.42:8111",
    invites: [],
    guests: [{ id: "g1", name: "Mia", chars_today: 12400, chars_per_day: 30000, joined: now() - 3 * 86400, last_seen: now() - 40, last_address: "198.51.100.7", total_chars: 88000 }],
    connections: 1,
    obs_https: true,
    https: { domain: null, valid_from: null, valid_until: null, renew_at: null, step: null, error: null, error_at: null, next_try: null, staging: false },
    obs_online: true,
  };
  const wait = (ms: number) => new Promise((res) => setTimeout(res, ms));
  return {
    status: async () => structuredClone(internet),
    set: async (p) => {
      if (p.enabled !== undefined) Object.assign(internet, { enabled: p.enabled, listening: p.enabled });
      if (p.obs_https !== undefined) internet.obs_https = p.obs_https;
      if (p.domain !== undefined) {
        internet.domain = p.domain.replace(/\.duckdns\.org$/, "");
        if (p.token) internet.duckdns_token = true;
        if (internet.duckdns_token) {
          Object.assign(internet, { duckdns_at: now(), duckdns_result: "OK", address: `${internet.domain}.duckdns.org:8111` });
          // the demo "fetches" a certificate in two steps
          const domain = `${internet.domain}.duckdns.org`;
          internet.https = { ...internet.https!, step: "validating" };
          setTimeout(() => (internet.https = { ...internet.https!, step: null, domain, valid_from: now(), valid_until: now() + 90 * 86400, renew_at: now() + 60 * 86400 }), 1500);
        }
      }
      return structuredClone(internet);
    },
    test: async () => {
      await wait(500);
      Object.assign(internet, { test_at: now(), reachable: true });
      return { reachable: true };
    },
    invite: async (name, days, charsPerDay) => {
      const invite = { id: `i${internet.invites.length + 1}`, name, created: now(), expires: now() + days * 86400, chars_per_day: charsPerDay };
      internet.invites.push(invite);
      return { invite, code: "ttv1:eyJhZGRyZXNzIjoibXktc3RyZWFtLmR1Y2tkbnMub3JnOjgxMTEiLCJmaW5nZXJwcmludCI6ImRlbW8ifQ" };
    },
    withdraw: async (id) => {
      internet.invites = internet.invites.filter((i) => i.id !== id);
      return null;
    },
    revoke: async (id) => {
      internet.guests = internet.guests.filter((g) => g.id !== id);
      return null;
    },
  };
}
