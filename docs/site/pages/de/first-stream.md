---
title: Dein erster Stream
description: Vom ersten Start bis zum vorgelesenen Chat – in etwa zehn Minuten.
icon: Rocket
---

Nach dem [Installieren](installation) und dem [Verbinden mit Voice Creator Pro](voice-host) fehlen noch vier Dinge.

## 1. Dein Kanal {#channel}

Öffne **Profile**. Ein Profil gibt es schon. Trag unter **Kanäle** deinen Twitch-Kanalnamen ein und drück Enter. Ein Profil kann mehrere Kanäle lesen – praktisch für Kooperationen.

Dann schalte das Profil ein: auf der Seite **Live**, unter **Liest gerade**, den Schalter daneben. TTV-TTS betritt den Chat; **Chat verbunden** erscheint.

## 2. Stimmen {#voices}

Such dir unter **Presets** eine Besetzung aus – zum Beispiel *Fantasy Tavern* – und klick auf **Berechnen**. Wähle:

- **Wie viele Stimmen**: 30 reichen für die meisten Kanäle; 60, 120 oder 240 für große Chats.
- **Sprachen**: die Sprachen, in denen dein Chat schreibt. Jede Sprache wird einzeln berechnet.

TTV-TTS zeigt, wie lange es dauert, gemessen auf deinem Stimmen-Host – grob eine halbe Minute pro Stimme. Du kannst dabei streamen: Der Chat hat immer Vorrang.

![Ein Preset berechnen: Stimmen, Sprachen und die geschätzte Zeit](preset-compute.webp "Berechnen")

> [!TIP]
> Keine Lust zu warten? Viele Presets gibt es **fertig zum Herunterladen** (Knopf **Holen**): ein paar Megabyte, etwa eine Minute zum Importieren, nichts zu berechnen.

Dann wählst du im Profil unter **Stimmen** dieses Preset. Zuschauer bekommen beim ersten Schreiben automatisch eine seiner Stimmen.

## 3. Reinhören {#test}

Tipp auf der Seite **Live** unter **Testnachricht** etwas ein und klick auf **Senden**. Sie nimmt denselben Weg wie eine echte Chatnachricht: aufgeräumt, Stimme gewählt, vorgelesen.

## 4. In OBS {#obs}

TTV-TTS spielt auf dem Ausgabegerät unter **Einstellungen → Ton** (normalerweise der Systemstandard). OBS hört es auf eine von zwei Arten:

- **Desktop-Audio** – nimmt OBS dein Standard-Ausgabegerät auf, ist TTV-TTS schon im Stream.
- **Anwendungsaudioaufnahme** (OBS 28+, Windows 10 2004+): Füg eine Quelle *Anwendungsaudioaufnahme* hinzu und wähl TTV-TTS. Dann kannst du die Lautstärke getrennt mischen.

Soll TTV-TTS nicht auf deinem Kopfhörer, aber im Stream sein, wähl in TTV-TTS ein virtuelles Kabel (VB-Cable, VoiceMeeter) als Ausgabegerät und füg es in OBS hinzu.

## Während du streamst {#live}

- **Überspringen** stoppt die aktuelle Nachricht, **Pause** hält neue zurück, **Warteschlange leeren** verwirft, was wartet – oder nimm [Tastenkürzel](settings#hotkeys), während du spielst.
- Deine Mods können Chat-Befehle wie `!skip` und `!ttspause` nutzen (siehe [Profile](profiles#mod-commands)).
- Ein Zuschauer klingt unpassend? Gib ihm auf der Seite [Zuschauer](viewers) eine andere Stimme.

Das war's – geh live. Die nächsten Seiten erklären jede Einstellung im Detail.
