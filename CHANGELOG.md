# Changelog

## [0.2.1]

- Moved to [TrunkRecorder](https://github.com/TrunkRecorder/trunk-plugin-notify), for the plugin
  store. Built from Trunk Recorder Pro v0.2.10 (SDK 0.7.0), the first release it fully works with.

## [0.2.0]

- For Trunk Recorder Pro with its in-band signalling restructure: tones arrive as `tone_pattern`
  events (`tone` before), KNOX digits as a `knox` flag (no `KNOX ` prefix), and talkgroup and radio
  lists use the recorder's own matcher. A tone typed into a rule is `{ "pattern": "digits",
  "digits": "072864", "knox": true }` for KNOX.
- The Notify button is on each signal in Setup, where signals are now edited.
- A rule is sent when it's heard, when the call it was on ends, or both (`send`), and with the
  call's recording when the call ends (`recording`). "Then the recording" (`followUp`) is now `send:
  both` with `recording`: set it again on rules that had it.
- Built from Trunk Recorder Pro's main (SDK 0.7.0) until that SDK is on crates.io.

## [0.1.0]

The first release: notifications as a plugin for Trunk Recorder Pro.

- Rules over a system's signals (named pages, sirens, DTMF and KNOX strings, Morse IDs), any tone
  heard (by kind, or particular tones typed in, with ▶), emergencies (a call or a radio's alarm),
  calls beginning, and new talkgroups and radios; narrowed by system, talkgroup, radio and
  encryption, with a cooldown, your own title and message
- Discord embeds, Slack blocks, ntfy, the notification's JSON or a body of your own; commands with
  the notification on stdin and `TRP_*` variables
- Then the recording: once the call is saved, its audio as a file to Discord and ntfy, inline in
  JSON, as `TRP_AUDIO` to commands, and a link where one can't go
- Send a test and Send a test recording on each destination
- Its own page in the recorder (under Plugins), a Notify button on each signal, and settings that
  take effect as they're edited
- Radios and talkgroups by name, as the recorder knows them

Needs a Trunk Recorder Pro with tone signals and plugin pages (SDK 0.6.0); built against the branch
that brings them until that SDK is published.
