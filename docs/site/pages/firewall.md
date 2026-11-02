---
title: Firewall
description: Let TTV-TTS reach the proxy – Windows Firewall, Windows Firewall Control, security suites, macOS and Linux.
icon: ShieldCheck
---

TTV-TTS on your streaming PC talks to the proxy on **port 8110** (TCP for the connection, UDP so it can find the proxy). The firewall on the **proxy's PC** has to let that in. Nothing has to change on the streaming PC, and nothing on your router.

## The quick way {#check}

Open the proxy → **Firewall & network**. It checks:

- whether the rule **“TTV-TTS Proxy”** exists, is on and points to the right program file,
- whether other rules **block** the proxy (Windows creates those if you clicked *Cancel* on its “allow access” question),
- whether your network is set to **Public** (a rule for private networks doesn't apply there),
- whether **Windows Firewall Control** or another firewall is installed.

![The proxy's firewall check](proxy-firewall.webp)

**Allow on private networks** (or **Allow on all networks**) fixes it – Windows asks once for administrator rights.

## Windows Defender Firewall by hand {#windows}

1. Press the Windows key, type “Windows Defender Firewall with Advanced Security” and open it.
2. **Inbound Rules → New Rule … → Program**, path: the proxy's program file (shown on the proxy's firewall page) → **Allow the connection**.
3. Tick **Domain** and **Private** (and **Public** only if your home network is classified as public) → name it “TTV-TTS Proxy”.
4. If Windows showed an “allow access” popup earlier and you clicked **Cancel**, it created blocking rules: delete inbound rules named “TTV-TTS Proxy” with a red icon.

## Windows Firewall Control (Binisoft / Malwarebytes) {#wfc}

WFC works on top of Windows Firewall and may remove rules it doesn't know.

1. When WFC shows a notification for “TTV-TTS Proxy”, choose **Allow** (inbound).
2. With **Secure Rules** on: WFC → **Security** → *Define here a new authorized group* → enter `TTV-TTS` → click **+**. Then click **Allow …** in the proxy again.
3. WFC → **Rules Panel** → search for “TTV-TTS”: the rule must be enabled and allow inbound connections.
4. The filtering profile *Medium* blocks unknown programs; the allow rule is enough, no profile change needed.

## Security suites {#suites}

Norton, Bitdefender, ESET, Kaspersky, Avast, AVG, McAfee, G DATA, Avira and others replace Windows Firewall with their own. Allow the proxy's program file for incoming connections on private networks:

| Suite | Where |
|---|---|
| Norton | Settings → Firewall → Program Control → Add |
| Bitdefender | Protection → Firewall → Settings → Rules → Add rule (Allow, Home/Office) |
| ESET | Setup → Network protection → Firewall → Edit rules → Add (In, Allow) |
| Kaspersky | Settings → Security settings → Firewall → Applications → set the proxy to *Trusted* |
| Avast / AVG | Protection → Firewall → Application settings → New application rule |
| McAfee | Firewall → Program permissions → Allow full access |
| G DATA | Firewall → Rule sets → your network → New rule (incoming, allow) |
| Others (ZoneAlarm, Comodo, GlassWire, simplewall, Sophos) | allow inbound TCP and UDP port 8110 for the proxy |

## macOS {#macos}

The first time, macOS asks *“Do you want the application TTV-TTS Proxy to accept incoming network connections?”* – click **Allow**. Later: **System Settings → Network → Firewall → Options → TTV-TTS Proxy → Allow incoming connections**. With Little Snitch or LuLu, allow incoming connections for TTV-TTS Proxy.

## Linux {#linux}

```sh
# ufw
sudo ufw allow from 192.168.0.0/16 to any port 8110 proto tcp
sudo ufw allow from 192.168.0.0/16 to any port 8110 proto udp
# firewalld
sudo firewall-cmd --permanent --zone=home --add-port=8110/tcp --add-port=8110/udp
sudo firewall-cmd --reload
```

## Test the connection {#test}

On the streaming PC, TTV-TTS → **Voice host** should list the proxy under *Found in your network*. Without TTV-TTS, in PowerShell:

```powershell
Test-NetConnection 192.168.1.25 -Port 8110
```

(with the proxy PC's address). `TcpTestSucceeded : True` means the firewall lets it through. Or open `https://192.168.1.25:8110/ttv/v1/info` in a browser there – a certificate warning is normal, the proxy uses its own certificate.

## Router {#router}

Nothing to do: both PCs are in the same network. **Never forward port 8110 (or Voice Creator Pro's 8100) to the internet.** To share voices with friends, the proxy has a separate, limited internet port – see [Friends over the internet](friends). To use the proxy yourself away from home, use a VPN such as Tailscale or WireGuard.
