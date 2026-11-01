//! "Translate for me": chat in languages the streamer doesn't speak is read
//! (or shown) in the streamer's language.
//!
//! Providers, measured on the target setup (2026-11-01):
//! - MyMemory (default): free web service, 0.4–1.3 s per message, no GPU;
//!   5,000 characters a day, 50,000 with a contact email.
//! - DeepL: the streamer's own free API key (500,000 characters a month).
//! - Voice Creator Pro: offline on the voice PC, 4–5 s per message, but VCP
//!   keeps one model at a time, so the next sentence waits ~15 s while the
//!   speaking model loads again.

use std::collections::HashMap;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Deserialize;

/// Translations kept for repeated messages ("gg", copy pasta).
const CACHE: usize = 500;

/// Who translates, with what account.
pub struct Service<'a> {
    /// "mymemory", "deepl" or "vcp".
    pub provider: &'a str,
    /// MyMemory contact email ("" = none).
    pub email: &'a str,
    pub deepl_key: Option<&'a str>,
    pub vcp: Option<&'a ttv_vcp::Client>,
}

pub struct Translator {
    http: reqwest::Client,
    cache: Mutex<HashMap<(String, String, String), String>>,
}

impl Translator {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, cache: Mutex::new(HashMap::new()) }
    }

    /// `text` from language `from` into `to` (codes like "es", "de").
    pub async fn translate(&self, svc: &Service<'_>, text: &str, from: &str, to: &str) -> Result<String, String> {
        let Service { provider, email, deepl_key, vcp } = *svc;
        let text = text.trim();
        if text.is_empty() || from == to {
            return Ok(text.to_string());
        }
        let key = (format!("{provider}:{from}"), to.to_string(), text.to_string());
        if let Some(t) = self.cache.lock().get(&key) {
            return Ok(t.clone());
        }
        let out = match provider {
            "deepl" => self.deepl(deepl_key.ok_or("Enter your DeepL API key in the settings first.")?, text, from, to).await?,
            "vcp" => {
                let vcp = vcp.ok_or("No voice host set up yet.")?;
                crate::speech::translate(vcp, &[text.to_string()], to, Some(from)).await?.into_iter().next().unwrap_or_default()
            }
            _ => self.mymemory(email, text, from, to).await?,
        };
        let out = out.trim().to_string();
        if out.is_empty() {
            return Err("the translation came back empty".into());
        }
        let mut c = self.cache.lock();
        if c.len() >= CACHE {
            c.clear();
        }
        c.insert(key, out.clone());
        Ok(out)
    }

    async fn mymemory(&self, email: &str, text: &str, from: &str, to: &str) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "translatedText")]
            translated_text: String,
        }
        #[derive(Deserialize)]
        struct Answer {
            #[serde(rename = "responseData")]
            data: Option<Data>,
            #[serde(rename = "responseStatus")]
            status: serde_json::Value,
            #[serde(rename = "responseDetails", default)]
            details: serde_json::Value,
            #[serde(rename = "quotaFinished", default)]
            quota_finished: Option<bool>,
        }
        let pair = format!("{}|{}", mymemory_code(from), mymemory_code(to));
        let mut q: Vec<(&str, &str)> = vec![("q", text), ("langpair", &pair)];
        if !email.trim().is_empty() {
            q.push(("de", email.trim()));
        }
        let r = self.http.get("https://api.mymemory.translated.net/get").query(&q).timeout(Duration::from_secs(10)).send().await.map_err(|e| format!("MyMemory: {e}"))?;
        let a: Answer = r.json().await.map_err(|e| format!("MyMemory: {e}"))?;
        if a.quota_finished == Some(true) {
            return Err("MyMemory's daily limit is used up (enter an email in the settings for 10× more, or use DeepL)".into());
        }
        let ok = a.status.as_i64() == Some(200) || a.status.as_str() == Some("200");
        match a.data {
            Some(d) if ok => Ok(html_unescape(&d.translated_text)),
            _ => Err(format!("MyMemory: {}", a.details.as_str().unwrap_or("no translation"))),
        }
    }

    async fn deepl(&self, key: &str, text: &str, from: &str, to: &str) -> Result<String, String> {
        #[derive(Deserialize)]
        struct T {
            text: String,
        }
        #[derive(Deserialize)]
        struct Answer {
            translations: Vec<T>,
        }
        // free keys end with ":fx" and use their own host
        let host = if key.ends_with(":fx") { "https://api-free.deepl.com" } else { "https://api.deepl.com" };
        let body = serde_json::json!({ "text": [text], "target_lang": deepl_target(to), "source_lang": from.to_ascii_uppercase() });
        let r = self
            .http
            .post(format!("{host}/v2/translate"))
            .header("Authorization", format!("DeepL-Auth-Key {key}"))
            .json(&body)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("DeepL: {e}"))?;
        match r.status().as_u16() {
            200 => {}
            403 => return Err("DeepL rejected the API key".into()),
            456 => return Err("the DeepL quota for this month is used up".into()),
            s => return Err(format!("DeepL answered HTTP {s}")),
        }
        let a: Answer = r.json().await.map_err(|e| format!("DeepL: {e}"))?;
        a.translations.into_iter().next().map(|t| t.text).ok_or_else(|| "DeepL sent no translation".into())
    }
}

/// MyMemory wants a few languages with their region.
fn mymemory_code(code: &str) -> &str {
    match code {
        "zh" => "zh-CN",
        "pt" => "pt-BR",
        c => c,
    }
}

/// DeepL's target codes (English and Portuguese need a variant).
fn deepl_target(code: &str) -> String {
    match code {
        "en" => "EN-US".into(),
        "pt" => "PT-BR".into(),
        "zh" => "ZH-HANS".into(),
        c => c.to_ascii_uppercase(),
    }
}

/// MyMemory sometimes returns HTML entities ("&#39;").
fn html_unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_entities() {
        assert_eq!(mymemory_code("zh"), "zh-CN");
        assert_eq!(mymemory_code("de"), "de");
        assert_eq!(deepl_target("en"), "EN-US");
        assert_eq!(deepl_target("de"), "DE");
        assert_eq!(html_unescape("it&#39;s &quot;fine&quot; &amp; ok"), "it's \"fine\" & ok");
    }

    /// Live: one request to MyMemory (needs internet).
    #[tokio::test]
    #[ignore]
    async fn mymemory_live() {
        let t = Translator::new(reqwest::Client::new());
        let svc = Service { provider: "mymemory", email: "", deepl_key: None, vcp: None };
        let out = t.translate(&svc, "Hola a todos, me encanta este juego.", "es", "de").await.unwrap();
        assert!(out.to_lowercase().contains("spiel"), "{out}");
        // the second time comes from the cache
        let t0 = std::time::Instant::now();
        assert_eq!(t.translate(&svc, "Hola a todos, me encanta este juego.", "es", "de").await.unwrap(), out);
        assert!(t0.elapsed() < Duration::from_millis(5));
    }
}
