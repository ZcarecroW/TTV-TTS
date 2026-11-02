---
title: Von 1.x umsteigen
description: Von der OBS-Browserquelle zur Desktop-App – was sich ändert und was bleibt.
icon: ClockRotate
---

TTV-TTS 1.x war eine einzelne HTML-Datei, genutzt als OBS-Browserquelle. 2.0 ist eine Desktop-App: Sie spielt auf einem Audiogerät statt in OBS, und sie kann viel mehr – Presets, einen Proxy für einen zweiten PC, Effekte, Übersetzung, Freigaben, Tastenkürzel.

## Deine Stimmen kommen mit {#classic}

Die 300 Stimmen, die du für 1.x gemacht hast (`TTS-de 01 …` bis `TTS-ko 30 …`, 30 Rollen in 10 Sprachen), sind noch in Voice Creator Pro. TTV-TTS findet sie und bietet auf der Seite **Presets** an: **Als Preset übernehmen**. Du bekommst das Preset **Classic 30** mit denselben Rollen – nichts muss berechnet werden. Zuschauer lassen sich dann mit diesen Stimmen vorlesen wie mit jedem anderen Preset.

Alternativ funktioniert der Stimmenmodus **Rolle pro Zuschauer** in einem [Profil](profiles#voices) genau wie 1.x: Jeder Zuschauer behält eine Rolle, gesprochen von der Stimme dieser Rolle in der Nachrichtensprache.

## Einstellungen {#settings}

1.x wurde mit URL-Parametern und einem Konfigurationsblock eingestellt. 2.0 hat dieselben Optionen – und mehr – in den [Profilen](profiles): Kanäle, Rollen, ignorierte Nutzer, Emotes und Emojis, Links, Namensansagen, gesperrte Wörter und Ersetzungen, Grenzen, Mod-Befehle. Richte sie einmal in der App ein.

## OBS {#obs}

Entfern die TTV-TTS-Browserquelle aus deinen OBS-Szenen (oder lass sie stumm, bis du umgestiegen bist). TTV-TTS 2.0 spielt auf einem Audiogerät, das OBS aufnimmt – siehe [In OBS](first-stream#obs).

## Voice Creator Pro auf einem zweiten PC {#lan}

Mit 1.x hast du vielleicht `vcp-lan-setup.ps1` auf dem Voice-Creator-Pro-PC ausgeführt, das Port 8101 mit einer Windows-Portweiterleitung geöffnet hat. 2.0 braucht das nicht: Der [TTV-TTS Proxy](proxy) ersetzt es, verschlüsselt und mit Koppeln. Entfern die alte Einrichtung auf diesem PC – in einer PowerShell als Administrator, im Ordner mit dem Skript:

```powershell
powershell -ExecutionPolicy Bypass -File .\vcp-lan-setup.ps1 -Uninstall
```

Das entfernt seine geplante Aufgabe, seine Firewall-Regel, die Portweiterleitung und seine Hilfsdateien. (Das Skript gibt es weiter unter seiner alten Adresse, falls du es nicht mehr hast.)

## Die alte Version {#old}

1.x funktioniert weiter unter seiner alten Adresse – deine OBS-Browserquelle geht nicht kaputt –, wird aber nicht weiterentwickelt. Seine Anleitung ist weiter online: [TTV-TTS 1.x](https://zcarecrow.github.io/TTV-TTS/readme.html).
