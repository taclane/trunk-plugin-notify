# Notifications for Trunk Recorder Pro

Tells you when something happens on the radio: a fire page goes out, a siren is set off, a radio
sends an emergency alarm, a talkgroup is heard for the first time. Rules say what to watch for;
destinations say where word goes: Discord, Slack, ntfy, any webhook, or a command on the
recorder's computer. A call's recording can follow.

## Features

- Rules over a system's **signals** (named pages, sirens, DTMF strings, Morse IDs), any **tone
  pattern**, **emergencies** (a call flagged emergency, or a radio's alarm), **calls** beginning,
  and **new talkgroups** and **radios**
- Narrowed by system, talkgroup, radio and encryption; a cooldown so a rule fires once per event
- Your own title and message ("LOCALTOWN FIRE PAGE"), with placeholders
- Discord embeds, Slack blocks, ntfy (title, priority, click-through), the notification's JSON,
  or a body template of your own; headers for tokens
- Commands, given the notification on stdin and as `TRP_*` variables
- **Sent when it's heard, when the call ends, or both**, and **with the recording** when the call
  ends: a file for Discord and ntfy, inline for JSON webhooks, a file's path for commands, a link
  where a file can't go
- **Send a test** and **Send a test recording** on each destination
- Its own page in the recorder's interface, and a **Notify** button on each signal

Needs Trunk Recorder Pro 0.2.10 or later (plugin SDK 0.7.0). On an earlier recorder it runs, but
hears no tones, and has no page of its own. Until SDK 0.7.0 is on crates.io, it's built from
Trunk Recorder Pro's v0.2.10.

## Install

In Trunk Recorder Pro, open **Plugins**, choose **Install from GitHub…**, and enter
`TrunkRecorder/trunk-plugin-notify`. Turn it on: **Notifications** appears in the navigation.

To build it yourself:

```sh
cargo build --release
```

and add `target/release/notify` with **Add from a file…**.

## Signals

A signal is a tone pattern a system sends, with a name: `FD_PAGE`, `SIREN_SIGNAL_1`. They're
made in the recorder: in Setup, each system's **Signals** (type one in from a paging list), or
**Add** on a pattern heard on its **Tones** card (Radio system tab). Each signal's **Notify** button
there adds a rule for it here and opens this page.

## Configure

| Setting | Key | Description |
| ------- | --- | ----------- |
| Rules | `rules` | What to watch for, and where to send word (below) |
| Destinations | `destinations` | Where rules send word (below) |
| Link recordings from | `recorderAddress` | The recorder's address as people reading notifications reach it (`http://100.64.0.5:8091`): recordings are linked from it ([below](#recordings)) |

### Rules

Every condition a rule sets has to hold; one left empty is any. For "this or that", make two.

| Setting | Key | Default | Description |
| ------- | --- | ------- | ----------- |
| Name | `name` | | What it's called; the title when it has none |
| On | `enabled` | `true` | |
| When | `event` | `signal` | `signal`, `tone` (a tone is heard), `emergency`, `call`, `new_talkgroup` or `new_radio` |
| Systems | `systems` | | Short names |
| Signals | `signals` | | `signal`: which, as `system:ID` (an `ID` alone: any system's) |
| Kinds of tone | `patterns` | | `tone`: `two_tone`, `long`, `hi_low`, `pulsed`, `digits` (DTMF), `knox`, `morse`, `sequence`, `beep` |
| Tones | `tones` | | `tone`: particular tones, typed in ([below](#tones-typed-in)); empty: any tone of those kinds |
| Talkgroups | `talkgroups` | | Numbers, ranges (`1000-1099`) and patterns (`507*`) |
| Radios | `units` | | The same, for radio IDs |
| Encryption | `encrypted` | `either` | Calls: `either`, `encrypted` or `clear` |
| Not again within, s | `cooldownS` | `60` | Once for the same system, talkgroup and signal (or pattern) in this long |
| Title | `title` | | The headline. Placeholders: `{system}` `{site}` `{talkgroup}` `{alpha_tag}` `{unit}` `{unit_tag}` `{signals}` `{text}` `{rule}` `{event}` |
| Message | `message` | | The words; empty: what happened. The same placeholders |
| Send | `send` | `heard` | `heard` (at once), `call_end` (when the call it was on ends) or `both`. A radio's alarm, on no call, is sent at once ([below](#when-the-call-ends)) |
| With the recording | `recording` | `false` | The message when the call ends carries its audio ([below](#recordings)) |
| Send to | `to` | | Destinations, by name |

### Tones typed in

A rule for "a tone is heard" can name particular tones: a page from a paging list that hasn't been
heard on the air yet, say a Quick Call II pair at 313.0 and 1642.0 Hz. The page asks for what each kind
needs: a page's tone A and tone B, a long tone's one tone, DTMF or KNOX digits, a Morse ID's text,
and lengths only where they're played (▶ plays it; pressed again, stops). In the settings each is
`pattern`, `tonesHz` (A then B for a page), `digits` (Morse: the text), `knox` (KNOX digits) and
`lengthsS` (not matched, since pagers vary them).

A tone heard counts as one typed in as the recorder counts a signal: within ±1.5 % of it when it's
measured in the audio (P25 Phase 1, analog), and within 15.7 Hz + 0.5 % when a P25 Phase 2 tone
frame names it (those name tones only to the nearest 31.25 Hz). **Extra tolerance, %**
(`extraTolerancePct`) widens both by that much. The page says what it comes to, in Hz, for the
tones typed in. Digits and Morse match exactly.

Tones a system sends that are named in the recorder are its **signals**: "a signal is heard".

A new talkgroup or radio is one first heard after this plugin has known the system an hour, so
starting with a new system doesn't announce everything on it. What's been heard is kept in the
plugin's data folder.

### Destinations

| Setting | Key | Description |
| ------- | --- | ----------- |
| Name | `name` | What rules send to it by |
| Kind | `type` | `webhook` or `command` |
| Format | `format` | Webhooks: `json` (the notification), `discord`, `slack` or `ntfy` |
| URL | `url` | Webhooks: where to |
| Method | `method` | Webhooks: empty for POST |
| Headers | `headers` | Webhooks: one a line, `Authorization: Bearer abc` |
| Own body | `body` | Webhooks: a template in place of the format: `{title}` `{text}` `{rule}` `{event}` `{system}` `{talkgroup}` `{alpha_tag}` `{unit}` `{signals}` `{url}` `{json}`. One that begins `{` is sent as JSON, its values escaped |
| Command | `command` | Run by the shell, with the notification's JSON on stdin and `TRP_TITLE`, `TRP_TEXT`, `TRP_RULE`, `TRP_EVENT`, `TRP_SYSTEM`, `TRP_TALKGROUP`, `TRP_UNIT`, `TRP_SIGNALS`, `TRP_URL` and, for a recording, `TRP_AUDIO`. It has 30 seconds |

### When the call ends

A rule sent **when the call ends** (or **both**) waits for the call the event was on to be over: a
page's notification goes once the dispatcher stops talking, with how long the call ran. A call that
isn't recorded is over when it ends; a recorded one when it's saved (or a minute after it ends, if it
never is, as one too short to keep). However many times a rule fires during one call (a page and its
warble), it sends one message when the call ends.

### Recordings

A rule **with the recording** sends its call-end message with the call's audio (M4A when another plugin has the recorder make one, else WAV; up to 8 MB):

| Destination | The audio |
| ----------- | --------- |
| Discord | Attached to the message |
| ntfy | The notification's attachment |
| JSON webhook | Inline: `audio` with its `name`, `type` and `base64` |
| Slack | A link only: Slack's incoming webhooks can't take a file |
| Own body | A link only (`{url}`) |
| Command | A file of its own, `TRP_AUDIO`, there while the command runs |

**Link recordings from** gives every recording notification a link to the call as well, and is the
only way Slack gets one: set it to the recorder's address as the people reading the notifications
reach it, such as its Tailscale address. The link plays the call for as long as the recorder keeps
its audio (Setup → Recording). Empty: no links.

A webhook that can't be reached, or answers with a server error, is tried twice more (after 2 and
10 seconds). Email and many other services are reachable through an
[Apprise API](https://github.com/caronc/apprise-api) URL.

Recordings of calls decrypted with your keys are never sent or linked.

## What was sent

Each notification, and how each destination took it, is in the plugin's log on its page. How
each destination is doing (up, down, its last error) is on the recorder's **Health** page.

See [example messages](example_messages.md) for what each format sends.

## License

GPL-3.0-or-later, like Trunk Recorder.
