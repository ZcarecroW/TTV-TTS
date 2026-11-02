---
title: Live
description: Was gerade gelesen wird, was als Nächstes kommt – und wie du eingreifst.
icon: Waveform
---

Auf der Seite **Live** siehst du TTV-TTS während des Streams bei der Arbeit zu.

![Die Live-Seite](live.webp)

## Liest gerade {#reading}

Oben stehen deine Profile mit je einem Schalter. Jedes eingeschaltete Profil liest seine Kanäle; mehrere gleichzeitig sind kein Problem (zum Beispiel dein Kanal und eine Kooperation). **Chat verbunden** zeigt, dass TTV-TTS im Chat ist; jeder Kanal zeigt *trete bei …*, bis Twitch ihn hereingelassen hat.

## Spricht gerade und als Nächstes {#queue}

- **Spricht gerade** – die vorgelesene Nachricht mit Name des Zuschauers, Stimme, Sprache und, falls erkannt, dem Gefühl (zum Beispiel *Freude (normal)*). Übersetzte Nachrichten zeigen die Übersetzung darunter.
- **Als Nächstes** – Nachrichten, die warten. Eine Nachricht, die zu lange wartet oder zu lang ist, fällt nach den Grenzen im [Profil](profiles#limits) weg.
- **Zuletzt** – was gelesen, übersprungen oder nicht gelesen wurde, mit dem Grund (zum Beispiel *ignorierter Nutzer (Bot)* oder *gesperrtes Wort*) und wie lange es bis zum ersten Ton gedauert hat.

### Knöpfe {#buttons}

| Knopf | Was er macht |
|---|---|
| **Überspringen** | stoppt die aktuelle Nachricht; die nächste beginnt |
| **Pause** / **Weiter** | neue Nachrichten warten in der Warteschlange; nichts geht verloren |
| **Warteschlange leeren** | verwirft alles, was wartet |
| **Nochmal abspielen** | wiederholt eine der letzten 20 Nachrichten (innerhalb von 3 Minuten) – sofort, ohne sie neu zu erzeugen |
| **Stumm** (neben einem Namen) | dieser Zuschauer wird in diesem Profil nicht mehr vorgelesen |

Dieselben Aktionen gibt es als [globale Tastenkürzel](settings#hotkeys), als [Chat-Befehle für Mods](profiles#mod-commands) und über die [Fernsteuerungs-API](remote-api).

## Nachrichten freigeben {#approval}

Für heikle Streams schaltest du im Profil **Nachrichten vor dem Vorlesen freigeben** ein. Jede Nachricht wartet dann unter **Wartet auf dein OK**, bis du **Vorlesen** oder **Verwerfen** klickst (oder das Tastenkürzel drückst). Was niemand freigibt, fällt nach der im Profil eingestellten Zeit weg.

![Nachrichten, die auf Freigabe warten](live-approval.webp)

Ansagen von Abos, Raids und anderen Ereignissen brauchen kein OK.

## Testnachricht {#test}

Unter **Testnachricht** kannst du beliebigen Text unter beliebigem Namen schicken. Er nimmt genau den Weg einer Chatnachricht – praktisch, um vor dem Stream eine Stimme, einen Filter oder ein Emote auszuprobieren.

## Wenn nichts gelesen wird {#nothing}

- Kein Profil ist eingeschaltet → *Kein Profil ist an*.
- Kein Stimmen-Host → TTV-TTS kann den Chat lesen, aber nicht sprechen. Siehe [Voice Creator Pro verbinden](voice-host).
- Eine Nachricht steht unter *Zuletzt* als *nicht gelesen* → der Grund steht daneben. Siehe [Fehlersuche](troubleshooting).
