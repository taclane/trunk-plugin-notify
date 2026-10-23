# Example messages

## The notification (format `json`)

What a webhook in the JSON format is sent, and what a command gets on stdin. `event` is
`signal`, `tone`, `emergency`, `call`, `new_talkgroup`, `new_radio`, `recording` (a call's
recording, following a notification about it) or `test`.

```json
{
  "id": 3,
  "rule": "Fire pages",
  "event": "signal",
  "title": "LOCALTOWN FIRE PAGE",
  "short_name": "wood",
  "talkgroup": 1,
  "alpha_tag": "Fire Dispatch",
  "unit": 1675303,
  "unit_tag": "",
  "signals": ["wood:FD_PAGE"],
  "text": "wood:FD_PAGE on TG 1 Fire Dispatch",
  "call_num": 18233,
  "time": 1792687369.42,
  "detail": {
    "short_name": "wood",
    "call_num": 18233,
    "talkgroup": 1,
    "alpha_tag": "Fire Dispatch",
    "src": 1675303,
    "pattern": "two_tone",
    "tones": [467.13, 1032.3],
    "lengths": [1.0, 3.02],
    "format": "Quick Call II",
    "origin": "audio",
    "text": "two-tone 467.13 → 1032.3 Hz, 1 + 3.02 s (Quick Call II)",
    "signals": ["FD_PAGE"],
    "time": 1792687369.42
  }
}
```

Its recording, when the rule has **Then the recording**:

```json
{
  "id": 4,
  "rule": "Fire pages",
  "event": "recording",
  "title": "LOCALTOWN FIRE PAGE",
  "short_name": "wood",
  "talkgroup": 1,
  "alpha_tag": "Fire Dispatch",
  "unit": 1675303,
  "unit_tag": "",
  "signals": ["wood:FD_PAGE"],
  "text": "wood:FD_PAGE on TG 1 Fire Dispatch (recording, 21 s)",
  "call_num": 18233,
  "url": "http://100.64.0.5:8091/calls/wood/2026/10/22/1-1792687369_452562500.m4a",
  "time": 1792687369.42,
  "detail": { "path": "wood/2026/10/22/1-1792687369_452562500", "length": 21.4 }
}
```

## Discord

An embed, coloured by event (signal orange, emergency red, tone blue, recording green, new purple).
A recording's audio is attached as a file (multipart, the JSON as `payload_json`).

```json
{
  "username": "Trunk Recorder Pro",
  "embeds": [
    {
      "title": "LOCALTOWN FIRE PAGE",
      "description": "wood:FD_PAGE on TG 1 Fire Dispatch",
      "color": 16485376,
      "timestamp": "2026-10-22T13:22:49Z",
      "fields": [
        { "name": "System", "value": "wood", "inline": true },
        { "name": "Talkgroup", "value": "1 Fire Dispatch", "inline": true },
        { "name": "Radio", "value": "1675303", "inline": true },
        { "name": "Signal", "value": "wood:FD_PAGE", "inline": true },
        { "name": "Tones", "value": "two-tone 467.13 → 1032.3 Hz, 1 + 3.02 s (Quick Call II)", "inline": true }
      ],
      "footer": { "text": "Trunk Recorder Pro · Fire pages" }
    }
  ]
}
```

## Slack

```json
{
  "text": "LOCALTOWN FIRE PAGE: wood:FD_PAGE on TG 1 Fire Dispatch",
  "blocks": [
    { "type": "section", "text": { "type": "mrkdwn", "text": "*LOCALTOWN FIRE PAGE*\nwood:FD_PAGE on TG 1 Fire Dispatch" } },
    { "type": "context", "elements": [{ "type": "mrkdwn", "text": "*System:* wood   *Talkgroup:* 1 Fire Dispatch   *Signal:* wood:FD_PAGE" }] }
  ]
}
```

## ntfy

The words as the body, with headers: `Title` (the title), `Tags` (`bell`, `rotating_light`,
`speaker`, `new` or `radio`), `Priority: high` for signals and emergencies, and `Click` (the
recording's link).

## Commands

The notification's JSON on stdin, and:

| Variable | |
| -------- | - |
| `TRP_TITLE` | `LOCALTOWN FIRE PAGE` |
| `TRP_TEXT` | `wood:FD_PAGE on TG 1 Fire Dispatch` |
| `TRP_RULE` | `Fire pages` |
| `TRP_EVENT` | `signal` |
| `TRP_SYSTEM` | `wood` |
| `TRP_TALKGROUP` | `1` |
| `TRP_UNIT` | `1675303` |
| `TRP_SIGNALS` | `wood:FD_PAGE` |
| `TRP_URL` | a recording's link |
