---
title: FAQ
description: Kurze Antworten auf häufige Fragen.
icon: CircleInfo
---

### Brauche ich eine Twitch-Anmeldung? {#login}

Nein. TTV-TTS liest den Chat anonym, wie ein nicht angemeldeter Zuschauer. Melde dich nur an, wenn Follows und Kanalpunkte-Belohnungen ohne Text [angesagt](profiles#announcements) werden sollen.

### Was kostet es? {#cost}

TTV-TTS und der Proxy sind kostenlos und Open Source (MIT). Du brauchst [Voice Creator Pro](https://voicecreator.pro) und eine Grafikkarte, auf der es läuft. Pro Nachricht kostet nichts; die optionale Übersetzung durch MyMemory ist kostenlos, DeepL mit eigenem Schlüssel hat ein kostenloses Kontingent.

### Wie schnell ist es? {#speed}

Gemessen auf einer RTX 3070 mit Qwen3 in Voice Creator Pro: Eine Nachricht beginnt etwa 1,3 bis 1,8 Sekunden, nachdem sie im Chat erscheint, und eine Sekunde Sprache braucht etwa eine halbe Sekunde zum Erzeugen. Die erste Nachricht nach dem Entwurf einer Stimme dauert länger (etwa 18 s), weil Voice Creator Pro sein Sprech-Modell neu laden muss – siehe [Fehlersuche](troubleshooting#slow-first).

### Bremst es mein Spiel? {#game}

Auf einem PC teilt sich Voice Creator Pro die Grafikkarte mit deinem Spiel, während es spricht oder rechnet. Darum lassen viele Streamer Voice Creator Pro auf einem zweiten PC mit dem [Proxy](proxy) laufen – TTV-TTS selbst braucht wenig.

### Können Zuschauer es missbrauchen? {#abuse}

Du bestimmst, was gelesen wird: Rollen, ein Befehl oder eine Kanalpunkte-Belohnung, Grenzen pro Nachricht und pro Zuschauer, gesperrte Wörter, Umgang mit Links, eine [Freigabe-Warteschlange](live#approval), Mod-Befehle zum Überspringen und Stummschalten – und von Mods gelöschte Nachrichten werden nie vorgelesen.

### Welche Sprachen? {#languages}

So viele, wie dein Voice-Creator-Pro-Modell spricht – Qwen3 spricht Englisch, Deutsch, Französisch, Spanisch, Italienisch, Portugiesisch, Russisch, Japanisch, Chinesisch und Koreanisch. Presets werden pro Sprache berechnet; für Sprachen ohne berechnete Stimmen nimmt TTV-TTS die Ersatzsprache.

### Kann ich eigene Stimmen nutzen? {#own-voices}

Ja: Schreib [eigene Presets](presets#own-presets) (mit einer KI oder von Hand), leg einem Zuschauer eine beliebige Voice-Creator-Pro-Stimme fest, oder nutz die Stimmenmodi *Stimme pro Zuschauer* / *eine Stimme* mit deinen eigenen Stimmen.

### Funktioniert es mit OBS? {#obs}

Ja – TTV-TTS spielt auf einem Audiogerät, das OBS als Desktop-Audio oder mit *Anwendungsaudioaufnahme* aufnimmt. Siehe [In OBS](first-stream#obs).

### Gibt es die alte OBS-Browserquelle noch? {#v1}

Ja, TTV-TTS 1.x funktioniert weiter unter seiner alten Adresse, wird aber nicht weiterentwickelt. Siehe [Von 1.x umsteigen](from-v1).

### Wo werden meine Stimmen gespeichert? {#storage}

In Voice Creator Pro. TTV-TTS führt eine Bibliothek, welche Stimme zu welcher Preset-Figur und Sprache gehört, und (optional) verlustfreie Sicherungen der Aufnahmen.
