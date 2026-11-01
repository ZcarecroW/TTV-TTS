//! Windows Firewall: checks whether other PCs may reach the proxy, and adds
//! the rule (one UAC prompt). The rule allows this program (TCP for apps, UDP
//! for discovery) and sits in the group "TTV-TTS" - Windows Firewall Control
//! users authorize that group once. Also removes block rules for this program
//! that Windows creates when its "allow access" popup is cancelled.
//!
//! Other systems: macOS asks by itself on first use; on Linux the guide in
//! the window explains ufw/firewalld.

use serde::{Deserialize, Serialize};

pub const RULE_NAME: &str = "TTV-TTS-Proxy";
pub const RULE_GROUP: &str = "TTV-TTS";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleInfo {
    pub enabled: bool,
    pub action: String,
    pub profile: String,
    pub program: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NetProfile {
    pub alias: String,
    /// Public, Private, DomainAuthenticated
    pub category: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FirewallStatus {
    /// The proxy can check and fix the firewall on this system (Windows).
    pub managed: bool,
    /// Our own rule, if present.
    pub rule: Option<RuleInfo>,
    /// Our rule exists, is enabled, allows, and points at this program.
    pub rule_ok: bool,
    /// Enabled inbound allow rules for this program (ours or Windows' own).
    pub allows: u32,
    /// Enabled inbound block rules for this program (they win over allows).
    pub blocks: u32,
    pub profiles: Vec<NetProfile>,
    /// Some network is classified "Public" while our rule covers private only.
    pub public_network_blocked: bool,
    /// Windows Firewall Control (Binisoft/Malwarebytes) runs.
    pub wfc: bool,
    /// Third-party firewalls registered with Windows Security.
    pub third_party: Vec<String>,
    /// Windows Firewall profiles that are switched on.
    pub firewall_on: Vec<String>,
    pub program: String,
    pub error: Option<String>,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use base64::Engine;

    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Raw {
        rule: Option<RawRule>,
        allows: u32,
        blocks: u32,
        profiles: Vec<NetProfile>,
        #[serde(rename = "thirdParty")]
        third_party: Vec<String>,
        #[serde(rename = "firewallOn")]
        firewall_on: Vec<String>,
        wfc: bool,
    }

    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct RawRule {
        enabled: String,
        action: String,
        profile: String,
        program: String,
    }

    fn encode(script: &str) -> String {
        let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        base64::engine::general_purpose::STANDARD.encode(utf16)
    }

    fn ps_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }

    fn program() -> String {
        std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub async fn check() -> FirewallStatus {
        let exe = program();
        let script = format!(
            r#"$ErrorActionPreference='SilentlyContinue'
$exe={exe}
$r=Get-NetFirewallRule -Name '{RULE_NAME}'
$rule=$null
if($r){{$rule=@{{enabled=[string]$r.Enabled;action=[string]$r.Action;profile=[string]$r.Profile;program=[string](($r|Get-NetFirewallApplicationFilter).Program)}}}}
$mine=@(Get-NetFirewallApplicationFilter -Program $exe|Get-NetFirewallRule|Where-Object{{$_.Direction -eq 'Inbound' -and $_.Enabled -eq 'True'}})
$p=@(Get-NetConnectionProfile|ForEach-Object{{@{{alias=[string]$_.InterfaceAlias;category=[string]$_.NetworkCategory}}}})
$t=@(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName FirewallProduct|ForEach-Object{{[string]$_.displayName}})
$on=@(Get-NetFirewallProfile|Where-Object{{$_.Enabled -eq 'True'}}|ForEach-Object{{[string]$_.Name}})
$wfc=[bool](Get-Process -Name wfc,wfcs) -or [bool](Get-Service -Name _wfcs,wfcs)
@{{rule=$rule;allows=@($mine|Where-Object{{$_.Action -eq 'Allow'}}).Count;blocks=@($mine|Where-Object{{$_.Action -eq 'Block'}}).Count;profiles=$p;thirdParty=$t;firewallOn=$on;wfc=$wfc}}|ConvertTo-Json -Depth 4 -Compress"#,
            exe = ps_quote(&exe)
        );
        let mut st = FirewallStatus { managed: true, program: exe.clone(), ..Default::default() };
        let out = {
            let mut cmd = tokio::process::Command::new("powershell.exe");
            cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &encode(&script)]);
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            cmd.kill_on_drop(true);
            tokio::time::timeout(std::time::Duration::from_secs(25), cmd.output()).await
        };
        let raw: Raw = match out {
            Ok(Ok(o)) => match serde_json::from_slice(&o.stdout) {
                Ok(r) => r,
                Err(e) => {
                    st.error = Some(format!("unexpected answer from PowerShell: {e}"));
                    return st;
                }
            },
            Ok(Err(e)) => {
                st.error = Some(format!("PowerShell could not be started: {e}"));
                return st;
            }
            Err(_) => {
                st.error = Some("the firewall check took too long".into());
                return st;
            }
        };
        st.allows = raw.allows;
        st.blocks = raw.blocks;
        st.profiles = raw.profiles;
        st.third_party = raw.third_party.into_iter().filter(|t| !t.is_empty()).collect();
        st.firewall_on = raw.firewall_on;
        st.wfc = raw.wfc;
        if let Some(r) = raw.rule {
            let rule = RuleInfo { enabled: r.enabled == "True", action: r.action, profile: r.profile, program: r.program };
            st.rule_ok = rule.enabled && rule.action == "Allow" && rule.program.eq_ignore_ascii_case(&exe);
            let any = rule.profile.contains("Any") || rule.profile.contains("Public");
            st.public_network_blocked = st.rule_ok && !any && st.profiles.iter().any(|p| p.category == "Public");
            st.rule = Some(rule);
        }
        st
    }

    /// Adds our rule (and removes block rules for this program) with admin
    /// rights. `all_networks` = also on networks Windows calls "Public".
    pub async fn fix(all_networks: bool) -> Result<(), String> {
        let exe = program();
        let profile = if all_networks { "Any" } else { "Private,Domain" };
        let script = format!(
            r#"$ErrorActionPreference='Stop'
$exe={exe}
Get-NetFirewallRule -Name '{RULE_NAME}' -ErrorAction SilentlyContinue|Remove-NetFirewallRule
Get-NetFirewallApplicationFilter -Program $exe -ErrorAction SilentlyContinue|Get-NetFirewallRule|Where-Object{{$_.Direction -eq 'Inbound' -and $_.Action -eq 'Block'}}|Remove-NetFirewallRule
New-NetFirewallRule -Name '{RULE_NAME}' -DisplayName 'TTV-TTS Proxy' -Group '{RULE_GROUP}' -Description 'Lets TTV-TTS on other PCs in your network use Voice Creator Pro through TTV-TTS Proxy.' -Direction Inbound -Program $exe -Action Allow -Profile {profile} -Enabled True|Out-Null"#,
            exe = ps_quote(&exe)
        );
        let params = format!("-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -EncodedCommand {}", encode(&script));
        tokio::task::spawn_blocking(move || run_elevated("powershell.exe", &params)).await.map_err(|e| e.to_string())?
    }

    /// Runs a program with admin rights (UAC prompt) and waits for it.
    fn run_elevated(file: &str, params: &str) -> Result<(), String> {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
        use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let verb = wide("runas");
        let file = wide(file);
        let params = wide(params);
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_HIDE;
        // SAFETY: all pointers point into buffers that outlive the call.
        let ok = unsafe { ShellExecuteExW(&mut info) };
        if ok == 0 {
            let err = unsafe { GetLastError() };
            return Err(if err == 1223 { "cancelled".into() } else { format!("could not get admin rights (error {err})") });
        }
        if info.hProcess.is_null() {
            return Ok(());
        }
        // SAFETY: hProcess is a valid process handle owned by us until closed.
        unsafe {
            let wait = WaitForSingleObject(info.hProcess, INFINITE);
            let mut code: u32 = 1;
            GetExitCodeProcess(info.hProcess, &mut code);
            CloseHandle(info.hProcess);
            if wait != WAIT_OBJECT_0 || code != 0 {
                return Err(format!("the firewall change failed (exit code {code})"));
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use imp::{check, fix};

#[cfg(not(windows))]
pub async fn check() -> FirewallStatus {
    FirewallStatus {
        managed: false,
        program: std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        ..Default::default()
    }
}

#[cfg(not(windows))]
pub async fn fix(_all_networks: bool) -> Result<(), String> {
    Err("On this system the proxy can't change the firewall itself - see the guide below.".into())
}
