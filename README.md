# timeline

A local-first evidence timeline builder for detectives and defense attorneys who currently build case timelines by hand in PowerPoint.

`timeline` keeps every event in a single plain-text `timeline.json` file in your working directory. No accounts, no network, no telemetry — your case data never leaves your machine.

## Who it's for

- **Detectives** consolidating phone records, witness statements, camera hits, and dispatch logs into one chronological view.
- **Defense attorneys** looking for gaps, overlaps, and contradictions in the prosecution's timeline — including sightings of the same person in two places too close together in time to be physically possible.

## Install

```sh
cargo install --path .
```

## Usage

Create a store in the current directory:

```sh
timeline init
```

Add events one at a time:

```sh
timeline add \
  --at 2026-09-18T14:32:00Z \
  --desc "Phone call placed to unknown number" \
  --source "phone records" \
  --subject "Alex Merren" \
  --location "Dockside"

# Plain dates work too (treated as midnight UTC):
timeline add --at 2026-09-18 --desc "Seen near the pier" --source "witness statement #2"
```

Bulk import from CSV (columns: `at,desc,source,subject,location`):

```sh
timeline import evidence.csv
```

Show the unified, chronologically sorted timeline with source citations:

```sh
timeline show
timeline show --subject "Alex Merren"
```

Flag suspicious overlaps — impossible travel (same subject at two different locations within a window too short to travel, default 30 minutes) and exact-duplicate descriptions from different sources:

```sh
timeline conflicts
timeline conflicts --window-minutes 45
```

Export the timeline as a markdown document:

```sh
timeline export                 # to stdout
timeline export --out case.md   # to a file
```

## Data model

Each event has:

| Field | Required | Notes |
|---|---|---|
| `at` | yes | RFC3339 timestamp or `YYYY-MM-DD` date |
| `description` | yes | Free text |
| `source` | yes | Citation label, e.g. `phone records` |
| `subject` | no | Person-of-interest tag |
| `location` | no | Place tag |

## Roadmap

- A [Tauri](https://tauri.app) desktop GUI is planned, so timelines can be built and reviewed visually without touching the terminal.
- Richer conflict heuristics (travel-time estimates, confidence scoring).

## License

Apache-2.0 — see [LICENSE](LICENSE).


Part of [Blackshield Company](https://github.com/Blackshield-Company).
