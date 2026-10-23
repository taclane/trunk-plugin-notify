"""Check a plugin's manifest (`<plugin> --describe`) the way the registry will.

check_manifest.py manifest.json [--release]
  --release: also fail on what the template leaves for you to fill in.
"""
import json, re, sys

m = json.load(open(sys.argv[1]))
release = "--release" in sys.argv
problems = []
if not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,40}", m.get("id", "")):
    problems.append("id: lowercase letters, digits and dashes (2-41 characters)")
if not m.get("name"):
    problems.append("name: set a display name in manifest()")
if not re.fullmatch(r"\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?", m.get("version", "")):
    problems.append("version: semver (from Cargo.toml)")
if not m.get("description"):
    problems.append("description: set one in Cargo.toml")
if not m.get("repository") or (release and "YOUR-NAME" in m["repository"]):
    problems.append("repository: your plugin's GitHub URL, in Cargo.toml")
if not m.get("subscribe"):
    problems.append("subscribe: a plugin that subscribes to nothing never hears anything")
known = {"call.start", "call.end", "call.concluded", "unit", "audio", "status", "tone"}
for t in m.get("subscribe", []):
    if t not in known:
        problems.append(f"subscribe: unknown topic {t!r}")
for f in m.get("audio_formats", []):
    if f not in {"m4a"}:
        problems.append(f"audio_formats: unknown format {f!r}")
if problems:
    print("Manifest problems:\n  " + "\n  ".join(problems))
    sys.exit(1)
print(f"{m['id']} {m['version']}: ok")
